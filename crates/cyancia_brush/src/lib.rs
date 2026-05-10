use cyancia_actions::{ActionAppExt, canvas_control::CanvasToolSwitch};
use cyancia_assets::AssetAppExt;
use cyancia_runtime::{Application, plugin::Plugin};
use cyancia_tools::ToolsAppExt;

use crate::{asset::BrushPresetSerializer, tool::BrushTool};

pub mod asset;
pub mod browser;
pub mod editor;
pub mod input_processing;
pub mod instance;
pub mod render;
pub mod tool;

pub struct BrushPlugin;

impl Plugin for BrushPlugin {
    fn build(&self, app: &mut Application) {
        app.add_asset_serializer::<BrushPresetSerializer>()
            .add_tool_function::<BrushTool>();
    }
}
