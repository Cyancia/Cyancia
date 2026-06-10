use cyancia_math::number::AngleDifference;
use cyancia_tools::{ToolFunction, ToolId};
use glam::Vec2;
use gpui::{App, AppContext, Context, MouseDownEvent, MouseMoveEvent, Pixels, Point, px};

use crate::{CanvasAppExt, CanvasManager, control::CanvasTransform};

fn mouse_position(position: Point<Pixels>) -> Vec2 {
    Vec2::new(position.x / px(1.), position.y / px(1.))
}

#[derive(Default)]
pub struct PanTool {
    start_pos: Vec2,
    original_transform: CanvasTransform,
}

impl ToolFunction for PanTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("pan_tool")
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        self.start_pos = mouse_position(mouse.position);
        self.original_transform = canvas.transform.clone();
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        cx.update_current_canvas(|canvas, _| {
            let delta = mouse_position(mouse.position) - self.start_pos;
            canvas.transform = self.original_transform.clone().translated(delta);
        });
    }
}

#[derive(Default)]
pub struct RotateTool {
    center: Vec2,
    initial_angle: f32,
    original_transform: CanvasTransform,
}

impl ToolFunction for RotateTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("rotate_tool")
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        self.center = canvas.transform.widget_bounds.size() * 0.5;
        let t = self.center - mouse_position(mouse.position);
        self.initial_angle = t.y.atan2(t.x);
        self.original_transform = canvas.transform.clone();
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        cx.update_current_canvas(|canvas, _| {
            let t = self.center - mouse_position(mouse.position);
            let cur_angle = t.y.atan2(t.x);
            canvas.transform = self
                .original_transform
                .clone()
                .rotated_around(cur_angle.angle_difference(self.initial_angle), self.center);
        });
    }
}

#[derive(Default)]
pub struct ZoomTool {
    start_pos: Vec2,
    original_transform: CanvasTransform,
}

impl ToolFunction for ZoomTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("zoom_tool")
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        self.start_pos = mouse_position(mouse.position);
        self.original_transform = canvas.transform.clone();
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        cx.update_current_canvas(|canvas, _| {
            let d = mouse_position(mouse.position).y - self.start_pos.y;
            let f = d / self.original_transform.widget_bounds.size().y + 1.0;
            canvas.transform = self
                .original_transform
                .clone()
                .scaled_around(f, self.start_pos);
        });
    }
}
