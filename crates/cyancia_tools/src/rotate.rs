use cyancia_canvas::{CCanvas, control::CanvasTransform};
use cyancia_id::Id;
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_math::number::AngleDifference;
use glam::Vec2;

use crate::{CanvasTool, CanvasToolFunction};

#[derive(Default)]
pub struct RotateTool {
    center: Vec2,
    initial_angle: f32,
    original_transform: CanvasTransform,
}

impl CanvasToolFunction for RotateTool {
    fn id(&self) -> Id<CanvasTool> {
        Id::from_str("rotate_tool")
    }

    fn begin(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {
        let transform = canvas.transform.read();
        self.center = transform.widget_size * 0.5;
        let t = self.center - Vec2::new(mouse.position.x, mouse.position.y);
        self.initial_angle = t.y.atan2(t.x);
        self.original_transform = transform.clone();
    }

    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {
        let t = self.center - Vec2::new(mouse.position.x, mouse.position.y);
        let cur_angle = t.y.atan2(t.x);
        *canvas.transform.write() = self
            .original_transform
            .clone()
            .rotated_around(cur_angle.angle_difference(self.initial_angle), self.center);
    }
}
