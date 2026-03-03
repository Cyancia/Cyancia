use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use downcast_rs::DowncastSync;
use futures::executor::block_on;
use parking_lot::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use wgpu::{Adapter, Backends, Device, Features, Instance, Limits, Queue};

use crate::Services;

pub trait Service: DowncastSync {}

downcast_rs::impl_downcast!(sync Service);

pub trait FromRuntime {
    fn from_runtime(runtime: &Services) -> Self;
}

impl<T: Default> FromRuntime for T {
    fn from_runtime(_runtime: &Services) -> Self {
        Self::default()
    }
}

/// A read guard for a service held in an `Arc<RwLock<dyn Service>>`.  
/// SAFETY: `guard` is declared before `_arc` so it is dropped first, releasing
/// the read lock before the Arc (and thus the RwLock) is freed.
pub struct ServiceRef<T: Service> {
    // Field order matters: guard must be dropped before _arc.
    guard: MappedRwLockReadGuard<'static, T>,
    _arc: Arc<RwLock<dyn Service>>,
}

impl<T: Service> ServiceRef<T> {
    /// # Safety
    /// `arc` must keep the same `RwLock` alive for the entire lifetime of the
    /// returned `ServiceRef`. This is guaranteed by storing `arc` in the struct.
    pub(crate) fn from_arc(arc: Arc<RwLock<dyn Service>>) -> Self {
        let guard = unsafe {
            // `arc` lives on the heap and will be stored in the struct alongside
            // this guard. Because `guard` is declared before `_arc`, it is
            // dropped first, so the RwLock is always valid while the guard lives.
            let raw: MappedRwLockReadGuard<'_, T> = RwLockReadGuard::map(
                arc.read(),
                |x: &dyn Service| x.downcast_ref::<T>().unwrap(),
            );
            std::mem::transmute::<MappedRwLockReadGuard<'_, T>, MappedRwLockReadGuard<'static, T>>(
                raw,
            )
        };
        ServiceRef { guard, _arc: arc }
    }

    pub fn as_ref(&self) -> &T {
        &self.guard
    }
}

impl<T: Service> Deref for ServiceRef<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// A write guard for a service held in an `Arc<RwLock<dyn Service>>`.
/// SAFETY: same field-order drop guarantee as `ServiceRef`.
pub struct ServiceMut<T: Service> {
    // Field order matters: guard must be dropped before _arc.
    guard: MappedRwLockWriteGuard<'static, T>,
    _arc: Arc<RwLock<dyn Service>>,
}

impl<T: Service> ServiceMut<T> {
    pub(crate) fn from_arc(arc: Arc<RwLock<dyn Service>>) -> Self {
        let guard = unsafe {
            let raw: MappedRwLockWriteGuard<'_, T> = RwLockWriteGuard::map(
                arc.write(),
                |x: &mut dyn Service| x.downcast_mut::<T>().unwrap(),
            );
            std::mem::transmute::<
                MappedRwLockWriteGuard<'_, T>,
                MappedRwLockWriteGuard<'static, T>,
            >(raw)
        };
        ServiceMut { guard, _arc: arc }
    }

    pub fn as_ref(&self) -> &T {
        &self.guard
    }

    pub fn as_mut(&mut self) -> &mut T {
        &mut self.guard
    }
}

impl<T: Service> Deref for ServiceMut<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<T: Service> DerefMut for ServiceMut<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
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
