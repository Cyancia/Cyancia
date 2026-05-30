use std::sync::Arc;

use cyancia_actions::ActionAppExt;
use cyancia_assets::AssetAppExt;
use cyancia_tools::ToolsAppExt;
use gpui::App;
use parking_lot::Mutex;

use crate::{asset::{BrushPreset, BrushPresetSerializer}, instance::BrushPresetInstance, render::BrushPresetOperator, tool::{BrushTool, CurrentBrushPresetOperator}};

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
