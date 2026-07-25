use std::sync::Arc;

use anyhow::Result;
use gpui::{App, Global, Window};
use wgpu::{Device, Queue};

pub trait RenderContextAppExt {
    fn render_context(&self) -> &RenderContext;
    fn render_device(&self) -> &Device;
    fn render_queue(&self) -> &Queue;
}

impl RenderContextAppExt for App {
    fn render_context(&self) -> &RenderContext {
        self.global::<RenderContext>()
    }

    fn render_device(&self) -> &Device {
        &self.render_context().device
    }

    fn render_queue(&self) -> &Queue {
        &self.render_context().queue
    }
}

pub struct RenderContext {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
}

impl Global for RenderContext {}

impl RenderContext {
    pub fn from_window(window: &Window) -> Result<Self> {
        let context = window.gpu_context().unwrap();
        let (device, queue) = *context.downcast::<(Arc<Device>, Arc<Queue>)>().unwrap();

        Ok(Self { device, queue })
    }
}
