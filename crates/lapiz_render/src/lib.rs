wesl::wesl_pkg!(pub render);

use lapiz_assets::AssetAppExt;
use lapiz_runtime::{Application, plugin::Plugin};

use crate::{
    resources::{FullscreenVertex, GlobalSamplers},
    texture::ImageSerializer,
};

pub mod bind_group_entries;
pub mod bind_group_layout_entries;
pub mod buffer;
pub mod readback;
pub mod render_context;
pub mod resources;
pub mod texture;
pub mod texture_atlas;
pub mod util;
pub mod wesl_jit;

pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut Application) {
        let mut runtime = app.runtime_mut();
        runtime.add_service::<GlobalSamplers>();
        runtime
            .services_mut()
            .add_asset_serializer::<ImageSerializer>();
        runtime.add_service::<FullscreenVertex>();
    }
}
