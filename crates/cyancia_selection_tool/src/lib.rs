use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use gpui::{App, Context};

use crate::rectangle::RectangularSelectionTool;

pub mod rectangle;

pub fn init(cx: &mut App) {
    cx.add_tool_function::<RectangularSelectionTool>();
}
