use cyancia_canvas::{CCanvas, control::CanvasTransform};
use cyancia_id::Id;
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use glam::Vec2;

use crate::{CanvasTool, CanvasToolFunction};

#[derive(Default)]
pub struct PanTool {
    start_pos: Vec2,
    original_transform: CanvasTransform,
}

impl CanvasToolFunction for PanTool {
    fn id(&self) -> Id<CanvasTool> {
        Id::from_str("pan_tool")
    }

    fn begin(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {
        self.start_pos = Vec2::new(mouse.position.x, mouse.position.y);
        self.original_transform = canvas.transform.read().clone();
    }

    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {
        let delta = Vec2::new(mouse.position.x, mouse.position.y) - self.start_pos;
        *canvas.transform.write() = self.original_transform.clone().translated(delta);
    }
}
