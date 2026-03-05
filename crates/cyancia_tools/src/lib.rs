use std::{any::Any, collections::HashMap, sync::Arc, time::Instant};

use cyancia_input::{
    action::{Action, ActionId},
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_runtime::{Application, Runtime, Services, plugin::Plugin, service::Service};
use cyancia_utils::wrapper;
use iced_core::{Point, keyboard::key, mouse};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use uuid::Uuid;

pub struct ToolsPlugin;

impl Plugin for ToolsPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<ToolFunctionRegistry>()
            .add_service::<ToolProxies>();
    }
}

pub trait ToolsAppExt {
    fn add_tool_function<T: ToolFunction + Default>(&mut self) -> &mut Self;
}

impl ToolsAppExt for Application {
    fn add_tool_function<T: ToolFunction + Default>(&mut self) -> &mut Self {
        self.runtime()
            .services()
            .service_mut::<ToolFunctionRegistry>()
            .register::<T>();
        self
    }
}

wrapper! {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub ToolId : Arc<str>
}

pub struct Tool {
    pub binded_action: ActionId,
}

pub trait ToolFunction: Send + Sync + 'static {
    fn id(&self) -> ToolId;
    fn activate(&mut self, services: &Services) {}
    fn hover(&mut self, keyboard: &KeyboardState, mouse: &HoverMouseState, services: &Services) {}
    fn begin(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, services: &Services) {}
    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, services: &Services) {
    }
    fn end(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, services: &Services) {}
    fn deactivate(&mut self, services: &Services) {}
}

#[derive(Default)]
pub struct ToolFunctionRegistry {
    spawners: Vec<Box<dyn Fn() -> Box<dyn ToolFunction> + Send + Sync>>,
}

impl ToolFunctionRegistry {
    pub fn register<T: ToolFunction + Default>(&mut self) {
        self.spawners.push(Box::new(|| Box::new(T::default())));
    }

    pub fn create(&self) -> Vec<Box<dyn ToolFunction>> {
        self.spawners.iter().map(|spawner| spawner()).collect()
    }
}

impl Service for ToolFunctionRegistry {}

struct ToolProxyState {
    last: ToolId,
    current: ToolId,
    last_switch: Instant,
}

pub struct ToolProxy {
    state: ToolProxyState,
    tools: HashMap<ToolId, Box<dyn ToolFunction>>,
}

impl ToolProxy {
    pub fn new(initial: ToolId, collection: &ToolFunctionRegistry) -> Self {
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

    pub fn switch_tool(&mut self, tool: ToolId, services: &Services) {
        if let Some(current_tool) = self.tools.get_mut(&self.state.current) {
            current_tool.deactivate(services);
        }

        self.state.last = self.state.current.clone();
        self.state.current = tool;
        self.state.last_switch = Instant::now();

        if let Some(new_tool) = self.tools.get_mut(&self.state.current) {
            new_tool.activate(services);
        }
    }

    pub fn mouse_pressed(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &Services,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.state.current) {
            tool.begin(keyboard, mouse, services);
        }
    }

    pub fn mouse_moved_pressing(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &Services,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.state.current) {
            tool.update(keyboard, mouse, services);
        }
    }

    pub fn mouse_moved_hovering(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        services: &Services,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.state.current) {
            tool.hover(keyboard, mouse, services);
        }
    }

    pub fn mouse_released(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &Services,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.state.current) {
            tool.end(keyboard, mouse, services);
        }
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq,Hash)]
    pub ToolProxyId : Uuid
}

#[derive(Default)]
pub struct ToolProxies {
    proxies: HashMap<ToolProxyId, ToolProxy>,
}

impl Service for ToolProxies {}

impl ToolProxies {
    pub fn get(&self, id: &ToolProxyId) -> &ToolProxy {
        self.proxies.get(id).unwrap()
    }

    pub fn get_mut(&mut self, id: &ToolProxyId) -> &mut ToolProxy {
        self.proxies.get_mut(id).unwrap()
    }

    pub fn add(&mut self, collection: &ToolFunctionRegistry) -> ToolProxyId {
        let id = ToolProxyId::new(Uuid::new_v4());
        self.proxies.insert(
            id,
            // TODO don't hard code this
            ToolProxy::new(ToolId::new("pan_tool".into()), collection),
        );

        id
    }
}
