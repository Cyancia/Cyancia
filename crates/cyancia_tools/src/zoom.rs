use cyancia_canvas::{CCanvas, control::CanvasTransform};
use cyancia_id::Id;
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_math::number::AngleDifference;
use glam::Vec2;

use crate::{CanvasTool, CanvasToolFunction};

#[derive(Default)]
pub struct ZoomTool {
    start_pos: Vec2,
    original_transform: CanvasTransform,
}

impl CanvasToolFunction for ZoomTool {
    fn id(&self) -> Id<CanvasTool> {
        Id::from_str("zoom_tool")
    }

    fn begin(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {
        self.start_pos = Vec2::new(mouse.position.x, mouse.position.y);
        self.original_transform = canvas.transform.read().clone();
    }

    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {
        let d = mouse.position.y - self.start_pos.y;
        let f = d / self.original_transform.widget_size.y + 1.0;
        *canvas.transform.write() = self
            .original_transform
            .clone()
            .scaled_around(f, self.start_pos);
    }
}
