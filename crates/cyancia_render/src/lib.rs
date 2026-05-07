wesl::wesl_pkg!(pub render);

use std::sync::Arc;

use cyancia_assets::AssetAppExt;
use cyancia_runtime::{Application, Runtime, Services, plugin::Plugin};
use futures::executor::block_on;
use wgpu::{Backends, Device, Features, Limits, Queue, TextureFormat};

use crate::{
    resources::{FullscreenVertex, GlobalSamplers},
    texture::ImageSerializer,
};

pub mod buffer;
pub mod resources;
pub mod texture;
pub mod texture_atlas;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<GlobalSamplers>()
            .add_asset_serializer::<ImageSerializer>()
            .add_service::<FullscreenVertex>();
    }
}
