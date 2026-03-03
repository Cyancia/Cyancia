use std::{any::Any, collections::HashMap, sync::Arc, time::Instant};

use cyancia_canvas::{CCanvas, CanvasId};
use cyancia_input::{
    action::{Action, ActionId},
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_runtime::{Application, Runtime, plugin::Plugin, service::Service};
use cyancia_utils::wrapper;
use iced_core::{Point, keyboard::key, mouse};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{brush::BrushTool, pan::PanTool, rotate::RotateTool, zoom::ZoomTool};

pub mod brush;
pub mod pan;
pub mod rotate;
pub mod zoom;

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<CanvasToolFunctionRegistry>()
            .add_service::<CanvasToolProxies>()
            .add_canvas_tool_function::<BrushTool>()
            .add_canvas_tool_function::<PanTool>()
            .add_canvas_tool_function::<RotateTool>()
            .add_canvas_tool_function::<ZoomTool>();
    }
}

pub trait ToolsAppExt {
    fn add_canvas_tool_function<T: CanvasToolFunction + Default>(&mut self) -> &mut Self;
}

impl ToolsAppExt for Application {
    fn add_canvas_tool_function<T: CanvasToolFunction + Default>(&mut self) -> &mut Self {
        self.runtime()
            .services()
            .service_mut::<CanvasToolFunctionRegistry>()
            .register::<T>();
        self
    }
}

wrapper! {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub CanvasToolId : Arc<str>
}

pub struct CanvasTool {
    pub binded_action: ActionId,
}

pub trait CanvasToolFunction: Send + Sync + 'static {
    fn id(&self) -> CanvasToolId;
    fn activate(&mut self, canvas: &CCanvas) {}
    fn hover(&mut self, keyboard: &KeyboardState, mouse: &HoverMouseState, canvas: &CCanvas) {}
    fn begin(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {}
    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {}
    fn end(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {}
    fn deactivate(&mut self, canvas: &CCanvas) {}
}

#[derive(Default)]
pub struct CanvasToolFunctionRegistry {
    spawners: Vec<Box<dyn Fn() -> Box<dyn CanvasToolFunction> + Send + Sync>>,
}

impl CanvasToolFunctionRegistry {
    pub fn register<T: CanvasToolFunction + Default>(&mut self) {
        self.spawners.push(Box::new(|| Box::new(T::default())));
    }

    pub fn create(&self) -> Vec<Box<dyn CanvasToolFunction>> {
        self.spawners.iter().map(|spawner| spawner()).collect()
    }
}

impl Service for CanvasToolFunctionRegistry {}

struct ToolProxyState {
    last: CanvasToolId,
    current: CanvasToolId,
    last_switch: Instant,
}

pub struct ToolProxy {
    state: ToolProxyState,
    tools: HashMap<CanvasToolId, Box<dyn CanvasToolFunction>>,
}

impl ToolProxy {
    pub fn new(initial: CanvasToolId, collection: &CanvasToolFunctionRegistry) -> Self {
        Self {
            state: ToolProxyState {
                last: initial.clone(),
                current: initial,
                last_switch: Instant::now(),
            },
            tools: collection
                .create()
                .into_iter()
                .map(|tool| (tool.id(), tool))
                .collect(),
        }
    }

    pub fn switch_tool(&mut self, tool: CanvasToolId, canvas: &CCanvas) {
        if let Some(current_tool) = self.tools.get_mut(&self.state.current) {
            current_tool.deactivate(canvas);
        }

        self.state.last = self.state.current.clone();
        self.state.current = tool;
        self.state.last_switch = Instant::now();

        if let Some(new_tool) = self.tools.get_mut(&self.state.current) {
            new_tool.activate(canvas);
        }
    }

    pub fn mouse_pressed(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.state.current) {
            tool.begin(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_moved_pressing(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.state.current) {
            tool.update(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_moved_hovering(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        canvas: &CCanvas,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.state.current) {
            tool.hover(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_released(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.state.current) {
            tool.end(keyboard, mouse, canvas);
        }
    }
}

#[derive(Default)]
pub struct CanvasToolProxies {
    proxies: HashMap<CanvasId, ToolProxy>,
}

impl Service for CanvasToolProxies {}

impl CanvasToolProxies {
    pub fn get(&self, canvas_id: &CanvasId) -> &ToolProxy {
        self.proxies.get(canvas_id).unwrap()
    }

    pub fn get_mut(&mut self, canvas_id: &CanvasId) -> &mut ToolProxy {
        self.proxies.get_mut(canvas_id).unwrap()
    }

    pub fn add(&mut self, canvas_id: &CanvasId, collection: &CanvasToolFunctionRegistry) {
        self.proxies.insert(
            *canvas_id,
            // TODO don't hard code this
            ToolProxy::new(CanvasToolId::new("pan_tool".into()), collection),
        );
    }
}
