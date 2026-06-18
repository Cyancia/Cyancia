use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use gpui::{App, Context};

#[derive(Default)]
pub struct RectangularSelectionTool {}

impl ToolFunction for RectangularSelectionTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("rectangular_selection_tool")
    }
}
