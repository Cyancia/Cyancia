wesl::wesl_pkg!(pub render);

use std::sync::Arc;

use cyancia_utils::global_instance::GlobalInstance;
use wgpu::{Device, Queue};

pub mod buffer;
pub mod renderer_acquire;
pub mod resources;

pub struct RenderContext {
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
}

pub static RENDER_CONTEXT: GlobalInstance<RenderContext> = GlobalInstance::new();
