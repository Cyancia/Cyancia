use std::{fmt::Debug, sync::Arc};

use cyancia_actions::input_manager::InputManager;
use cyancia_canvas::{
    CCanvas, CanvasId, CanvasManager,
    render::{CanvasRenderer, CanvasRenderers},
    widget::CanvasWidget,
};
use cyancia_dock::{DockManager, DockMessage, dock::Dock};
use cyancia_image::{
    CImage,
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage, GpuTileStorageInner},
};
use cyancia_input::action::ActionManifestCollection;
use cyancia_runtime::{
    Services,
    service::FromRuntime,
    windows::{WindowCommandBuffer, WindowView, WindowViewId},
};
use cyancia_tools::{ToolId, ToolProxies, ToolProxy};

use glam::UVec2;
use iced::{
    Element, Point, Subscription, Task, Theme, event,
    keyboard::{self},
    mouse, window,
};
use iced_wgpu::Renderer;
use uuid::Uuid;

use crate::dock::CanvasDock;

pub struct MainView {
    pub dock_manager: DockManager<Theme, Renderer>,
}

pub enum MainViewMessage {
    Dock(DockMessage),
    WindowEvent(window::Id, window::Event),
    CursorMoved(window::Id, Point),
    CursorReleased,
}

impl WindowView for MainView {
    type Message = MainViewMessage;

    fn id() -> WindowViewId {
        WindowViewId::new("main_view")
    }

    fn boot(runtime: Arc<Services>) -> (Self, Task<Self::Message>) {
        let actions = runtime
            .service::<ActionManifestCollection>()
            .subset_for_view("main_view");

        let canvas = CCanvas {
            id: CanvasId::new(Uuid::new_v4()),
            tool_proxy_id: runtime.service_mut::<ToolProxies>().add(ToolProxy::new()),
            image: Arc::new(CImage::new(UVec2 { x: 1024, y: 768 })),
            transform: Default::default(),
        };
        runtime
            .service_mut::<CanvasRenderers>()
            .insert(canvas.id, CanvasRenderer::from_runtime(&runtime));
        // TODO this should not be done here
        runtime.service::<GpuTileStorage>().declare_layer(
            canvas.image.root().id(),
            GpuLayerInfo {
                texel_type: TexelType::RGBA8,
            },
        );

        let (main_window, task) = window::open(Default::default());
        let mut dock_manager = DockManager::new(main_window);
        let canvas_dock = CanvasDock::new(canvas.id, actions, runtime.clone());
        let canvas_dock_id = <CanvasDock as Dock<Theme, Renderer>>::id(&canvas_dock);
        dock_manager.register_dock(canvas_dock);
        dock_manager.open_dock(canvas_dock_id);

        runtime.service_mut::<CanvasManager>().add_canvas(canvas);

        (Self { dock_manager }, task.discard())
    }

    fn view<'a>(
        &'a self,
        window: window::Id,
        runtime: Arc<Services>,
    ) -> impl Into<Element<'a, Self::Message, Theme, iced_wgpu::Renderer>> {
        Some(self.dock_manager.view(window)?.map(MainViewMessage::Dock))
    }

    fn update(
        &mut self,
        message: Self::Message,
        runtime: Arc<Services>,
    ) -> impl Into<Task<Self::Message>> {
        match message {
            MainViewMessage::Dock(m) => self.dock_manager.update(m).discard(),
            MainViewMessage::WindowEvent(id, event) => {
                self.dock_manager.on_window_event(id, event).discard()
            }
            MainViewMessage::CursorMoved(window, position) => self
                .dock_manager
                .on_cursor_moved(window, position)
                .discard(),
            MainViewMessage::CursorReleased => {
                self.dock_manager.on_float_window_drag_end().discard()
            }
        }
    }

    fn close(self, runtime: Arc<Services>) -> Task<()> {
        iced::exit()
    }

    fn subscription(&self) -> Subscription<(window::Id, Self::Message)> {
        iced::event::listen_with(|event, _status, window| match event {
            iced::Event::Window(e) => Some((window, MainViewMessage::WindowEvent(window, e))),
            iced::Event::Mouse(e) => match e {
                iced::mouse::Event::CursorMoved { position } => {
                    Some((window, MainViewMessage::CursorMoved(window, position)))
                }
                iced::mouse::Event::ButtonReleased(_) => {
                    Some((window, MainViewMessage::CursorReleased))
                }
                _ => None,
            },
            _ => None,
        })
    }

    fn windows(&self) -> Vec<window::Id> {
        self.dock_manager.window_infos().map(|i| i.id).collect()
    }
}
