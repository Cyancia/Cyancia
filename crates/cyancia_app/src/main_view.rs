use std::{fmt::Debug, sync::Arc};

use cyancia_actions::input_manager::InputManager;
use cyancia_canvas::{
    CCanvas, CanvasId, CanvasManager,
    render::{CanvasRenderer, CanvasRenderers},
    widget::CanvasWidget,
};
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
    Element, Subscription, Task, Theme, event,
    keyboard::{self},
    mouse, window,
};
use uuid::Uuid;

pub struct MainView {
    pub input_manager: InputManager,
}

#[derive(Debug)]
pub enum MainViewMessage {
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
}

impl MainView {
    pub fn new(services: Arc<Services>) -> Self {
        let actions = services
            .service::<ActionManifestCollection>()
            .subset_for_view("main_view");

        let canvas = CCanvas {
            id: CanvasId::new(Uuid::new_v4()),
            tool_proxy_id: services.service_mut::<ToolProxies>().add(ToolProxy::new()),
            image: Arc::new(CImage::new(UVec2 { x: 1024, y: 768 })),
            transform: Default::default(),
        };
        services
            .service_mut::<CanvasRenderers>()
            .insert(canvas.id, CanvasRenderer::from_runtime(&services));
        // TODO this should not be done here
        services.service::<GpuTileStorage>().declare_layer(
            canvas.image.root().id(),
            GpuLayerInfo {
                texel_type: TexelType::RGBA8,
            },
        );
        services.service_mut::<CanvasManager>().add_canvas(canvas);

        Self {
            input_manager: InputManager::new(actions),
        }
    }
}

impl WindowView for MainView {
    type Message = MainViewMessage;

    fn id(&self) -> WindowViewId {
        WindowViewId::new("main_view")
    }

    fn view<'a>(
        &'a self,
        runtime: Arc<Services>,
    ) -> impl Into<Element<'a, Self::Message, Theme, iced_wgpu::Renderer>> {
        let canvas_manager = runtime.service::<CanvasManager>();
        let renderers = runtime.service::<CanvasRenderers>();
        let current_canvas = canvas_manager.current().unwrap();
        let renderer = renderers.get(&current_canvas.id).unwrap();

        Some(CanvasWidget {
            canvas: current_canvas,
            renderer,
            tile_storage: runtime.service::<GpuTileStorage>().clone(),
        })
    }

    fn update(
        &mut self,
        message: Self::Message,
        runtime: Arc<Services>,
    ) -> impl Into<Task<Self::Message>> {
        match message {
            MainViewMessage::KeyboardEvent(event) => {
                return self
                    .input_manager
                    .on_keyboard_event(event, runtime)
                    .discard();
            }
            MainViewMessage::MouseEvent(event) => {
                self.input_manager.on_mouse_event(event, &runtime);
            }
        }

        Task::none()
    }

    fn subscription(&self) -> Subscription<(window::Id, MainViewMessage)> {
        event::listen_with(|event, _, window| match event {
            iced::Event::Keyboard(event) => Some((window, MainViewMessage::KeyboardEvent(event))),
            iced::Event::Mouse(event) => Some((window, MainViewMessage::MouseEvent(event))),
            _ => None,
        })
    }
}
