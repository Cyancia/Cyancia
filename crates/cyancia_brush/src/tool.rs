use cyancia_canvas::CCanvas;
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_runtime::Services;
use cyancia_tools::{ToolFunction, ToolId};

#[derive(Default)]
pub struct BrushTool;

impl ToolFunction for BrushTool {
    fn id(&self) -> ToolId {
        ToolId::new("brush_tool".into())
    }

    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, services: &Services) {
        dbg!();
    }
}
