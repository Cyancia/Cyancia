use std::sync::Arc;

use cyancia_actions::input_manager::InputManager;
use cyancia_canvas::{CanvasId, CanvasManager, render::CanvasRenderers, widget::CanvasWidget};
use cyancia_dock::dock::{Dock, DockId};
use cyancia_image::tile::GpuTileStorage;
use cyancia_input::action::ActionCollection;
use cyancia_runtime::Services;
use iced::Theme;
use iced::widget::text;
use iced_core::{Element, keyboard, mouse};
use iced_runtime::Task;
use iced_wgpu::Renderer;

macro_rules! test_dummy_dock {
    ($name:ident, $id:ident, $text:expr) => {
        pub struct $name;

        impl Dock<Theme, Renderer> for $name {
            type Message = ();

            fn id(&self) -> DockId {
                DockId::new($text.into())
            }

            fn view(&self) -> Element<'_, Self::Message, Theme, Renderer> {
                text($text).into()
            }

            fn update(&mut self, _message: ()) -> Task<()> {
                Task::none()
            }
        }

        pub const $id: &'static str = $text;
    };
}

test_dummy_dock!(LayerDock, LAYER_DOCK_ID, "Layers");
test_dummy_dock!(ToolDock, TOOL_DOCK_ID, "Tools");
test_dummy_dock!(HistoryDock, HISTORY_DOCK_ID, "History");

pub struct CanvasDock {
    canvas: CanvasId,
    runtime: Arc<Services>,
}

impl CanvasDock {
    pub fn new(canvas: CanvasId, runtime: Arc<Services>) -> Self {
        Self { canvas, runtime }
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
