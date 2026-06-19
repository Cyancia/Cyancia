use cyancia_tools::ToolsAppExt;
use gpui::App;

use crate::{
    freehand::{FreehandSelectionTool, PolygonSelectionTool},
    magic_wand::MagicWandSelectionTool,
    shape::{EllipticalSelectionTool, RectangularSelectionTool},
};

pub mod freehand;
pub mod magic_wand;
pub mod render;
pub mod shape;

pub fn init(cx: &mut App) {
    cx.add_tool_function::<RectangularSelectionTool>();
    cx.add_tool_function::<EllipticalSelectionTool>();
    cx.add_tool_function::<FreehandSelectionTool>();
    cx.add_tool_function::<PolygonSelectionTool>();
    cx.add_tool_function::<MagicWandSelectionTool>();
}
