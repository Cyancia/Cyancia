use std::{fmt::Debug, sync::Arc};

use cyancia_actions::{ActionFunctionRegistry, actions_matcher::ActionsMatcher};
use cyancia_canvas::{
    CCanvas, CanvasId, CanvasManager,
    event::{CanvasCreated, CanvasRemoved},
    render::{CanvasRenderer, CanvasRenderers},
    widget::CanvasWidget,
};
use cyancia_dock::{
    DockManager, DockMessage,
    dock::{Dock, DockId},
};
use cyancia_image::{
    CImage,
    texel::TexelType,
    tile::{GpuLayerInfo, GpuTileStorage, GpuTileStorageInner},
};
use cyancia_input::action::ActionManifestCollection;
use cyancia_runtime::{
    Services,
    event::Event,
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
use parking_lot::Mutex;
use uuid::Uuid;

use crate::dock::{CanvasDock, construct_canvas_dock_id};

pub struct MainView {
    dock_manager: DockManager<Theme, Renderer>,
    actions_matcher: Arc<Mutex<ActionsMatcher>>,
}

pub enum MainViewMessage {
    Dock(DockMessage),
    WindowEvent(window::Id, window::Event),
    KeyboardEvent(window::Id, keyboard::Event),
    MouseEvent(window::Id, mouse::Event),
    CanvasCreated(CanvasCreated),
    CanvasRemoved(CanvasRemoved),
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
        let actions_matcher = Arc::new(Mutex::new(ActionsMatcher::new(actions)));

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
        let (mut dock_manager, dock_manager_task) = DockManager::new(main_window);
        let canvas_dock = CanvasDock::new(canvas.id, runtime.clone(), actions_matcher.clone());
        let canvas_dock_id = <CanvasDock as Dock<Theme, Renderer>>::id(&canvas_dock);
        dock_manager.register_dock(canvas_dock);
        dock_manager.register_dock(crate::dock::LayerDock);
        dock_manager.register_dock(crate::dock::ToolDock);
        dock_manager.register_dock(crate::dock::HistoryDock);

        let dock_tasks = Task::batch([
            dock_manager.open_dock(canvas_dock_id),
            dock_manager.open_dock(DockId::new(crate::dock::LAYER_DOCK_ID.into())),
            dock_manager.open_dock(DockId::new(crate::dock::TOOL_DOCK_ID.into())),
            dock_manager.open_dock(DockId::new(crate::dock::HISTORY_DOCK_ID.into())),
        ])
        .map(MainViewMessage::Dock);

        runtime.service_mut::<CanvasManager>().add_canvas(canvas);

        (
            Self {
                dock_manager,
                actions_matcher,
            },
            Task::batch([
                task.discard(),
                dock_manager_task.map(MainViewMessage::Dock),
                dock_tasks,
            ]),
        )
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
            MainViewMessage::Dock(m) => self.dock_manager.update(m).map(MainViewMessage::Dock),
            MainViewMessage::WindowEvent(id, event) => {
                match event {
                    window::Event::Focused => {
                        self.actions_matcher.lock().reset_keyboard_state();
                    }
                    _ => {}
                }

                self.dock_manager.on_window_event(id, event).discard()
            }

            MainViewMessage::KeyboardEvent(window, event) => {
                if let Some(action) = self.actions_matcher.lock().on_keyboard_event(event)
                    && let Some(action_func) = runtime
                        .service_mut::<ActionFunctionRegistry>()
                        .get(action.clone())
                {
                    log::info!("Triggering action: {}", action);
                    action_func.trigger(runtime.clone()).discard()
                } else {
                    Task::none()
                }
            }
            MainViewMessage::MouseEvent(window, event) => {
                match event {
                    mouse::Event::CursorMoved { position } => {
                        return self
                            .dock_manager
                            .on_cursor_moved(window, position)
                            .map(MainViewMessage::Dock);
                    }
                    mouse::Event::ButtonReleased(mouse::Button::Left) => {
                        return self
                            .dock_manager
                            .on_float_window_drag_end()
                            .map(MainViewMessage::Dock);
                    }
                    _ => {}
                }

                Task::none()
            }
            MainViewMessage::CanvasCreated(e) => {
                log::info!("Canvas created: {}", e.id);
                let dock = CanvasDock::new(e.id, runtime, self.actions_matcher.clone());
                let id = <CanvasDock as Dock<Theme, Renderer>>::id(&dock);
                self.dock_manager.register_dock(dock);
                self.dock_manager.open_dock(id).map(MainViewMessage::Dock)
            }
            MainViewMessage::CanvasRemoved(e) => {
                log::info!("Canvas removed: {}", e.id);
                let id = DockId::new(construct_canvas_dock_id(e.id).into());
                self.dock_manager.unregister_dock(&id);

                Task::none()
            }
        }
    }

    fn close(self, runtime: Arc<Services>) -> Task<()> {
        iced::exit()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let external = iced::event::listen_with(|event, status, window| match event {
            iced::Event::Window(e) => Some(MainViewMessage::WindowEvent(window, e)),
            iced::Event::Keyboard(e) => Some(MainViewMessage::KeyboardEvent(window, e)),
            iced::Event::Mouse(e) => Some(MainViewMessage::MouseEvent(window, e)),
            _ => None,
        });

        let dock = self.dock_manager.subscription().map(MainViewMessage::Dock);
        let canvas_create = CanvasCreated::listen_to().map(MainViewMessage::CanvasCreated);
        let canvas_remove = CanvasRemoved::listen_to().map(MainViewMessage::CanvasRemoved);

        Subscription::batch([external, dock, canvas_create, canvas_remove])
    }

    fn windows(&self) -> Arc<[iced_core::window::Id]> {
        self.dock_manager
            .window_infos()
            .map(|i| i.id)
            .collect::<Vec<_>>()
            .into()
    }

    fn root_window(&self) -> Option<iced_core::window::Id> {
        Some(self.dock_manager.main_window().id)
    }
}
