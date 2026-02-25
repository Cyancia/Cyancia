use cyancia_canvas::CCanvas;
use cyancia_input::{
    action::Action,
    key::{KeySequence, KeyboardState},
    mouse::PressedMouseState,
};
use iced_core::Point;

use crate::{CanvasTool, CanvasToolFunction, CanvasToolId};

#[derive(Default)]
pub struct BrushTool;

impl CanvasToolFunction for BrushTool {
    fn id(&self) -> CanvasToolId {
        CanvasToolId::new("brush_tool".into())
    }

    fn activate(&mut self, canvas: &CCanvas) {
        println!("Switched to brush!");
    }

    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {
        println!("Painting at: {:?}", mouse.position);
    }

    fn deactivate(&mut self, canvas: &CCanvas) {
        println!("Exited brush!");
    }
}
