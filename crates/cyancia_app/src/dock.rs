use std::sync::Arc;

use cyancia_actions::input_manager::InputManager;
use cyancia_canvas::{CanvasId, CanvasManager, render::CanvasRenderers, widget::CanvasWidget};
use cyancia_dock::dock::{Dock, DockId};
use cyancia_image::tile::GpuTileStorage;
use cyancia_input::action::ActionCollection;
use cyancia_runtime::Services;
use iced_core::{keyboard, mouse};
use iced_runtime::Task;

pub struct CanvasDock {
    canvas: CanvasId,
    runtime: Arc<Services>,
}

impl CanvasDock {
    pub fn new(canvas: CanvasId, runtime: Arc<Services>) -> Self {
        Self {
            canvas,
            runtime,
        }
    }
}

#[derive(Debug)]
pub enum CanvasDockMessage {
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
}

impl<Theme> Dock<Theme, iced_wgpu::Renderer> for CanvasDock
where
    Theme: 'static,
{
    type Message = CanvasDockMessage;

    fn id(&self) -> cyancia_dock::dock::DockId {
        DockId::new(format!("canvas_dock_{}", self.canvas).into())
    }

    fn view<'a>(&'a self) -> iced_core::Element<'a, Self::Message, Theme, iced_wgpu::Renderer> {
        let canvas_manager = self.runtime.service::<CanvasManager>();
        let renderers = self.runtime.service::<CanvasRenderers>();
        let canvas = canvas_manager.get(&self.canvas).unwrap();
        let renderer = renderers.get(&self.canvas).unwrap();

        CanvasWidget {
            canvas,
            renderer,
            tile_storage: self.runtime.service::<GpuTileStorage>().clone(),
        }
        .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {

        Task::none()
    }
}
