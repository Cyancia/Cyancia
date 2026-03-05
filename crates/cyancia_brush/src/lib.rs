use cyancia_assets::AssetAppExt;
use cyancia_runtime::{Application, plugin::Plugin};

use crate::asset::{BrushPresetSerializer, ImageSerializer};

pub mod asset;
pub mod browser;
pub mod editor;
pub mod render;
pub mod tool;

pub struct BrushPlugin;

impl Plugin for BrushPlugin {
    fn build(&self, app: &mut Application) {
        app.add_asset_serializer::<BrushPresetSerializer>()
            .add_asset_serializer::<ImageSerializer>();
    }
}
