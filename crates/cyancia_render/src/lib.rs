wesl::wesl_pkg!(pub render);

use std::sync::Arc;

use cyancia_runtime::{Application, Runtime, Services, plugin::Plugin};
use futures::executor::block_on;
use wgpu::{Backends, Device, Features, Limits, Queue};

use crate::resources::{FullscreenVertex, GlobalSamplers};

pub mod buffer;
pub mod resources;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<GlobalSamplers>()
            .add_service::<FullscreenVertex>();
    }
}
