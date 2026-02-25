use std::{fmt::Debug, sync::Arc};

use cyancia_actions::{
    ActionFunctionCollection,
    canvas_control::{
        BrushToolAction, CanvasToolSwitch, PanToolAction, RotateToolAction, ZoomToolAction,
    },
    file::OpenFileAction,
    shell::{ActionShell, DestructedShell},
    task::ActionTask,
};
use cyancia_assets::{loader::AssetSerializerRegistry, store::AssetRegistry};
use cyancia_canvas::{
    CCanvas, CanvasId,
    render::{CanvasRenderer, CanvasRenderers},
    widget::CanvasWidget,
};
use cyancia_image::{
    CImage,
    tile::{GpuTileStorage, GpuTileStorageInner},
};
use cyancia_input::{
    action::{Action, ActionCollection, ActionManifest},
    key::{KeySequence, KeyboardState},
};
use cyancia_runtime::{
    Runtime,
    service::FromRuntime,
    windows::{WindowView, WindowViewId},
};
use cyancia_tools::{
    CanvasToolFunctionCollection, CanvasToolId, ToolProxy, brush::BrushTool, pan::PanTool,
    rotate::RotateTool, zoom::ZoomTool,
};
use glam::UVec2;
use iced::{
    Element, Point, Renderer, Subscription, Task, Theme, event,
    keyboard::{self, key},
    mouse, window,
};
use uuid::Uuid;

use crate::input_manager::InputManager;

pub struct MainView {
    pub assets: AssetRegistry,
    pub input_manager: InputManager,
    pub canvas: Arc<CCanvas>,

    pub renderer_acquired: bool,
}

pub enum MainViewMessage {
    WindowOpened(window::Id),
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
    ActionTaskCompleted(Box<dyn ActionTask>),
}

impl Debug for MainViewMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WindowOpened(arg0) => f.debug_tuple("WindowOpened").field(arg0).finish(),
            Self::KeyboardEvent(arg0) => f.debug_tuple("KeyboardEvent").field(arg0).finish(),
            Self::MouseEvent(arg0) => f.debug_tuple("MouseEvent").field(arg0).finish(),
            Self::ActionTaskCompleted(arg0) => f.debug_tuple("ActionTaskCompleted").finish(),
        }
    }
}

impl MainView {
    pub async fn new(runtime: &Runtime) -> Self {
        let mut loaders = AssetSerializerRegistry::new();
        cyancia_input::register_loaders(&mut loaders);
        let assets = AssetRegistry::new("assets", loaders.into()).await.unwrap();

        let actions = {
            let manifests = futures::future::join_all(
                assets
                    .all_handles_of::<ActionManifest>()
                    .await
                    .unwrap()
                    .into_iter()
                    .map(async |h| h.get().await.unwrap()),
            )
            .await;
            let mut collection = ActionFunctionCollection::new(ActionCollection::new(manifests));
            collection.register::<OpenFileAction>();
            collection.register::<CanvasToolSwitch<PanToolAction>>();
            collection.register::<CanvasToolSwitch<RotateToolAction>>();
            collection.register::<CanvasToolSwitch<ZoomToolAction>>();
            collection.register::<CanvasToolSwitch<BrushToolAction>>();
            collection
        };
        let tool_functions = {
            let mut c = CanvasToolFunctionCollection::new();
            c.register::<BrushTool>();
            c.register::<PanTool>();
            c.register::<RotateTool>();
            c.register::<ZoomTool>();
            c
        };
        let tools = { ToolProxy::new(CanvasToolId::new("brush_tool".into()), tool_functions) };

        Self {
            assets,
            canvas: Arc::new(CCanvas {
                id: CanvasId::new(Uuid::new_v4()),
                image: Arc::new(CImage::new(UVec2 { x: 1024, y: 768 })),
                transform: Default::default(),
            }),
            input_manager: InputManager::new(actions, tools),

            renderer_acquired: false,
        }
    }

    fn apply_shell(&mut self, shell: DestructedShell) -> Task<MainViewMessage> {
        self.canvas = shell.current_canvas;
        Task::batch(shell.tasks).map(|t| MainViewMessage::ActionTaskCompleted(t))
    }
}

impl WindowView for MainView {
    type Message = MainViewMessage;

    fn id(&self) -> WindowViewId {
        WindowViewId::new("main_view")
    }

    fn view<'a>(
        &'a self,
        runtime: &'a Runtime,
    ) -> impl Into<Element<'a, Self::Message, Theme, iced_wgpu::Renderer>> {
        let renderers = runtime.service_mut::<CanvasRenderers>();
        if renderers.get(&self.canvas.id).is_none() {
            renderers.insert(self.canvas.id, CanvasRenderer::from_runtime(runtime));
        }
        let renderer = renderers.get(&self.canvas.id).unwrap();
        // let Some(renderer) = runtime.service::<CanvasRenderers>().get(&self.canvas.id) else {
        //     return None;
        // };

        Some(CanvasWidget {
            canvas: self.canvas.clone(),
            renderer,
            tile_storage: runtime.service::<GpuTileStorage>().clone(),
        })
    }

    fn update(
        &mut self,
        message: Self::Message,
        runtime: &Runtime,
    ) -> impl Into<Task<Self::Message>> {
        let mut shell = ActionShell::new(self.canvas.clone(), self.input_manager.tools.clone());

        match message {
            MainViewMessage::WindowOpened(id) => {}
            MainViewMessage::KeyboardEvent(event) => {
                self.input_manager.on_keyboard_event(event, &mut shell);
            }
            MainViewMessage::MouseEvent(event) => {
                self.input_manager.on_mouse_event(event, &self.canvas);
            }
            MainViewMessage::ActionTaskCompleted(action_task) => {
                action_task.apply(&mut shell);
            }
        }

        self.apply_shell(shell.destruct())
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        event::listen().filter_map(|event| match event {
            iced::Event::Keyboard(event) => Some(MainViewMessage::KeyboardEvent(event)),
            iced::Event::Mouse(event) => Some(MainViewMessage::MouseEvent(event)),
            _ => None,
        })
    }
}
