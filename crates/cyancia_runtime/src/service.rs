use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use downcast_rs::DowncastSync;
use iced_futures::futures::executor::block_on;
use parking_lot::{
    MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLockReadGuard, RwLockWriteGuard,
};
use wgpu::{Adapter, Backends, Device, Features, Instance, Limits, Queue};

use crate::Runtime;

pub trait Service: DowncastSync {}

downcast_rs::impl_downcast!(sync Service);

pub trait FromRuntime {
    fn from_runtime(runtime: &Runtime) -> Self;
}

impl<T: Default> FromRuntime for T {
    fn from_runtime(_runtime: &Runtime) -> Self {
        Self::default()
    }
}

pub struct ServiceRef<'a, T: Service> {
    service: MappedRwLockReadGuard<'a, T>,
}

impl<'a, T: Service> ServiceRef<'a, T> {
    pub fn from_dynamic(x: RwLockReadGuard<'a, dyn Service>) -> Self {
        ServiceRef {
            service: RwLockReadGuard::map(x, |x| x.downcast_ref::<T>().unwrap()),
        }
    }

    pub fn as_ref(&self) -> &T {
        &self.service
    }
}

impl<'a, T: Service> Deref for ServiceRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

pub struct ServiceMut<'a, T: Service> {
    service: MappedRwLockWriteGuard<'a, T>,
}

impl<'a, T: Service> ServiceMut<'a, T> {
    pub fn from_dynamic(x: RwLockWriteGuard<'a, dyn Service>) -> Self {
        ServiceMut {
            service: RwLockWriteGuard::map(x, |x| x.downcast_mut::<T>().unwrap()),
        }
    }

    pub fn as_ref(&self) -> &T {
        &self.service
    }

    pub fn as_mut(&mut self) -> &mut T {
        &mut self.service
    }
}

impl<'a, T: Service> Deref for ServiceMut<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

impl<'a, T: Service> DerefMut for ServiceMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.service
    }
}

pub struct RenderContext {
    pub instance: Arc<Instance>,
    pub adapter: Arc<Adapter>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
}

impl Service for RenderContext {}

impl Default for RenderContext {
    fn default() -> Self {
        block_on(async {
            let instance =
                wgpu::util::new_instance_with_webgpu_detection(&wgpu::InstanceDescriptor {
                    backends: Backends::from_env().unwrap_or_default(),
                    ..Default::default()
                })
                .await;
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await
                .unwrap();
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_features: Features::empty(),
                    required_limits: Limits::downlevel_defaults(),
                    ..Default::default()
                })
                .await
                .unwrap();

            RenderContext {
                instance: instance.into(),
                adapter: adapter.into(),
                device: device.into(),
                queue: queue.into(),
            }
        })
    }
}
