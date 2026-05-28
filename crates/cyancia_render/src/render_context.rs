use std::sync::Arc;

use gpui::{Global, block_on};
use wgpu::{Adapter, Backends, Device, Features, Instance, Queue};

pub struct RenderContext {
    pub instance: Instance,
    pub adapter: Adapter,
    pub device: Device,
    pub queue: Queue,
}

impl Global for RenderContext {}

impl RenderContext {
    pub fn request_new() -> Self {
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
            log::info!("Adapter limits: {:#?}", adapter.limits());
            log::info!("Adapter features: {:#?}", adapter.features());
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    required_features: Features::CLEAR_TEXTURE,
                    required_limits: adapter.limits(),
                    ..Default::default()
                })
                .await
                .unwrap();

            device.on_uncaptured_error(Arc::new(|err| {
                log::error!("WGPU device error:\n{err}");
            }));
            device.set_device_lost_callback(|reason, err| {
                log::error!("WGPU device lost: {reason:?} {err}");
            });

            RenderContext {
                instance: instance.into(),
                adapter: adapter.into(),
                device: device.into(),
                queue: queue.into(),
            }
        })
    }
}
