use cyancia_image::texel::TexelType;
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use gpui::{App, Context};

use crate::{render::SelectionPipeline, shape::RectangularSelectionTool};

pub mod render;
pub mod shape;

pub fn init(cx: &mut App) {
    cx.add_tool_function::<RectangularSelectionTool>();
}
