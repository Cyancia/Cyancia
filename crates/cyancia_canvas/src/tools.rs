use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_math::number::AngleDifference;
use cyancia_runtime::{Runtime, Services};
use cyancia_tools::{ToolFunction, ToolId};
use glam::Vec2;
use iced_runtime::Task;

use crate::{CanvasManager, control::CanvasTransform};

#[derive(Default)]
pub struct PanTool {
    start_pos: Vec2,
    original_transform: CanvasTransform,
}

impl ToolFunction for PanTool {
    type Message = ();

    fn id() -> ToolId {
        ToolId::new("pan_tool")
    }

    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return Task::none();
        };

        self.start_pos = Vec2::new(mouse.position.x, mouse.position.y);
        self.original_transform = canvas.transform.clone();
        Task::none()
    }

    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(canvas) = services.service_mut::<CanvasManager>().current_mut() else {
            return Task::none();
        };

        let delta = Vec2::new(mouse.position.x, mouse.position.y) - self.start_pos;
        canvas.transform = self.original_transform.clone().translated(delta);

        Task::none()
    }
}

#[derive(Default)]
pub struct RotateTool {
    center: Vec2,
    initial_angle: f32,
    original_transform: CanvasTransform,
}

impl ToolFunction for RotateTool {
    type Message = ();

    fn id() -> ToolId {
        ToolId::new("rotate_tool")
    }

    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return Task::none();
        };

        self.center = canvas.transform.widget_bounds.size() * 0.5;
        let t = self.center - Vec2::new(mouse.position.x, mouse.position.y);
        self.initial_angle = t.y.atan2(t.x);
        self.original_transform = canvas.transform.clone();

        Task::none()
    }

    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(canvas) = services.service_mut::<CanvasManager>().current_mut() else {
            return Task::none();
        };

        let t = self.center - Vec2::new(mouse.position.x, mouse.position.y);
        let cur_angle = t.y.atan2(t.x);
        canvas.transform = self
            .original_transform
            .clone()
            .rotated_around(cur_angle.angle_difference(self.initial_angle), self.center);

        Task::none()
    }
}

#[derive(Default)]
pub struct ZoomTool {
    start_pos: Vec2,
    original_transform: CanvasTransform,
}

impl ToolFunction for ZoomTool {
    type Message = ();

    fn id() -> ToolId {
        ToolId::new("zoom_tool")
    }

    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return Task::none();
        };

        self.start_pos = Vec2::new(mouse.position.x, mouse.position.y);
        self.original_transform = canvas.transform.clone();

        Task::none()
    }

    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(canvas) = services.service_mut::<CanvasManager>().current_mut() else {
            return Task::none();
        };

        let d = mouse.position.y - self.start_pos.y;
        let f = d / self.original_transform.widget_bounds.size().y + 1.0;
        canvas.transform = self
            .original_transform
            .clone()
            .scaled_around(f, self.start_pos);

        Task::none()
    }
}
