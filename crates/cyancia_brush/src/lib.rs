use cyancia_assets::AssetAppExt;
use cyancia_tools::ToolsAppExt;
use gpui::App;

use crate::{
    asset::BrushPresetSerializer,
    tool::{BrushTool, CurrentBrushPresetOperator},
};

pub mod asset;
pub mod editor;
pub mod input_processing;
pub mod instance;
pub mod render;
pub mod tool;
pub mod widget;

pub fn init(cx: &mut App) {
    cx.add_asset_serializer::<BrushPresetSerializer>();
    cx.add_tool_function::<BrushTool>();
    cx.set_global(CurrentBrushPresetOperator::new(None));

    editor::init(cx);
}

// pub struct BrushPlugin;

// impl Plugin for BrushPlugin {
//     fn build(&self, app: &mut Application) {
//         app.add_asset_serializer::<BrushPresetSerializer>()
//             .add_tool_function::<BrushTool>();
//     }
// }
