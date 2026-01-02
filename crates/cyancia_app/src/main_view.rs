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
use cyancia_assets::store::{AssetLoaderRegistry, AssetRegistry};
use cyancia_canvas::{CCanvas, widget::CanvasWidget};
use cyancia_id::Id;
use cyancia_image::{
    CImage,
    tile::{GPU_TILE_STORAGE, GpuTileStorage},
};
use cyancia_input::{
    action::{Action, ActionCollection, ActionManifest},
    key::{KeySequence, KeyboardState},
};
use cyancia_render::{
    RENDER_CONTEXT, RenderContext,
    renderer_acquire::RendererAcquire,
    resources::{FULLSCREEN_VERTEX, FullscreenVertex, GLOBAL_SAMPLERS, GlobalSamplers},
};
use cyancia_tools::{
    CanvasToolFunctionCollection, ToolProxy, brush::BrushTool, pan::PanTool, rotate::RotateTool,
    zoom::ZoomTool,
};
use glam::UVec2;
use iced::{
    Element, Point, Renderer, Subscription, Task, Theme, event,
    keyboard::{self, key},
    mouse, window,
};

use crate::input_manager::InputManager;

pub struct MainView {
    pub assets: AssetRegistry,
    pub input_manager: InputManager,
    pub canvas: Arc<CCanvas>,

    pub renderer_acquired: bool,
}

pub enum MainViewMessage {
    RendererAcquired(Arc<wgpu::Device>, Arc<wgpu::Queue>),
    WindowOpened(window::Id),
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
    ActionTaskCompleted(Box<dyn ActionTask>),
}

impl Debug for MainViewMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RendererAcquired(arg0, arg1) => f
                .debug_tuple("RendererAcquired")
                .field(arg0)
                .field(arg1)
                .finish(),
            Self::WindowOpened(arg0) => f.debug_tuple("WindowOpened").field(arg0).finish(),
            Self::KeyboardEvent(arg0) => f.debug_tuple("KeyboardEvent").field(arg0).finish(),
            Self::MouseEvent(arg0) => f.debug_tuple("MouseEvent").field(arg0).finish(),
            Self::ActionTaskCompleted(arg0) => f.debug_tuple("ActionTaskCompleted").finish(),
        }
    }
}

impl MainView {
    pub fn new() -> Self {
        let mut loaders = AssetLoaderRegistry::new();
        cyancia_input::register_loaders(&mut loaders);
        let assets = AssetRegistry::new("assets", &loaders);

        let actions = {
            let mut collection = ActionFunctionCollection::new(ActionCollection::new(
                assets.store::<ActionManifest>().clone(),
            ));
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
        let tools = { ToolProxy::new(Id::from_str("brush_tool"), tool_functions) };

        Self {
            assets,
            canvas: Arc::new(CCanvas {
                image: Arc::new(CImage::new(UVec2 { x: 1024, y: 768 })),
                transform: Default::default(),
            }),
            input_manager: InputManager::new(actions, tools),

            renderer_acquired: false,
        }
    }

    pub fn view(&self) -> Element<'_, MainViewMessage, Theme, iced_wgpu::Renderer> {
        if self.renderer_acquired {
            self.view_internal()
        } else {
            Element::new(RendererAcquire {
                on_acquire: Box::new(|device, queue| {
                    log::info!("Renderer acquired!");
                    MainViewMessage::RendererAcquired(Arc::new(device), Arc::new(queue))
                }),
            })
        }
    }

    fn view_internal(&self) -> Element<'_, MainViewMessage, Theme, iced_wgpu::Renderer> {
        CanvasWidget {
            canvas: self.canvas.clone(),
            gpu_tile_storage: GPU_TILE_STORAGE.clone_arc(),
        }
        .into()
    }

    pub fn update(&mut self, message: MainViewMessage) -> Task<MainViewMessage> {
        let mut shell = ActionShell::new(self.canvas.clone(), self.input_manager.tools.clone());

        match message {
            MainViewMessage::WindowOpened(id) => {}
            MainViewMessage::RendererAcquired(device, queue) => {
                if !self.renderer_acquired {
                    self.renderer_acquired = true;

                    GLOBAL_SAMPLERS.init(GlobalSamplers::new(&device));
                    FULLSCREEN_VERTEX.init(FullscreenVertex::new(&device));
                    GPU_TILE_STORAGE.init(GpuTileStorage::new(device.clone(), queue.clone()));
                    RENDER_CONTEXT.init(RenderContext { device, queue });
                }
            }
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

    pub fn subscription(&self) -> Subscription<MainViewMessage> {
        event::listen().filter_map(|event| match event {
            iced::Event::Keyboard(event) => Some(MainViewMessage::KeyboardEvent(event)),
            iced::Event::Mouse(event) => Some(MainViewMessage::MouseEvent(event)),
            _ => None,
        })
    }

    fn apply_shell(&mut self, shell: DestructedShell) -> Task<MainViewMessage> {
        self.canvas = shell.current_canvas;
        Task::batch(shell.tasks).map(|t| MainViewMessage::ActionTaskCompleted(t))
    }
}
