wesl::wesl_pkg!(pub render);

use cyancia_assets::AssetAppExt;
use gpui::App;

use crate::{
    render_context::RenderContext,
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

pub fn init(cx: &mut App) {
    cx.set_global(RenderContext::request_new());
    cx.set_global(GlobalSamplers::from_app(cx));
    cx.add_asset_serializer::<ImageSerializer>();
    cx.set_global(FullscreenVertex::from_app(cx));
}

// pub struct RenderPlugin;

// impl Plugin for RenderPlugin {
//     fn build(&self, app: &mut Application) {
//         app.add_service::<GlobalSamplers>()
//             .add_asset_serializer::<ImageSerializer>()
//             .add_service::<FullscreenVertex>();
//     }
// }
