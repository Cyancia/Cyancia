use std::{any::Any, collections::HashMap, sync::Arc, time::Instant};

use cyancia_input::{
    action::{Action, ActionId},
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_runtime::{Application, Runtime, Services, plugin::Plugin, service::Service};
use cyancia_utils::wrapper;
use futures::{
    SinkExt,
    channel::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender},
};
use iced_core::{Point, keyboard::key, mouse};
use iced_runtime::Task;
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
        self.runtime_mut()
            .services_mut()
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
    fn activate(&mut self, services: &mut Services) {}
    fn hover(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        services: &mut Services,
    ) {
    }
    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
    }
    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
    }
    fn end(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
    }
    fn deactivate(&mut self, services: &mut Services) {}
}

#[derive(Default)]
pub struct ToolFunctionRegistry {
    spawners: HashMap<ToolId, Box<dyn Fn() -> Box<dyn ToolFunction> + Send + Sync>>,
}

impl ToolFunctionRegistry {
    pub fn register<T: ToolFunction + Default>(&mut self) {
        self.spawners
            .insert(T::default().id(), Box::new(|| Box::new(T::default())));
    }
}

impl Service for ToolFunctionRegistry {}

struct State {
    last: ToolId,
    current: ToolId,
    last_switch: Instant,
    current_function: Box<dyn ToolFunction>,
}

pub struct ToolProxy {
    state: Option<State>,
}

impl ToolProxy {
    pub fn new() -> Self {
        Self { state: None }
    }

    pub fn switch_tool(&mut self, tool: ToolId, services: &mut Services) {
        let last = match self.state.take() {
            Some(mut st) => {
                st.current_function.deactivate(services);
                st.current
            }
            None => tool.clone(),
        };

        let registry = services.service::<ToolFunctionRegistry>();
        if let Some(new_tool) = registry.spawners.get(&tool) {
            self.state = Some(State {
                last,
                current: tool.clone(),
                last_switch: Instant::now(),
                current_function: new_tool(),
            });
        } else {
            log::error!(
                "Unable to switch to tool {:?}: not found in registry.",
                tool
            );
        }
    }

    pub fn mouse_pressed(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
        if let Some(f) = self.current_function() {
            f.begin(keyboard, mouse, services);
        }
    }

    pub fn mouse_moved_pressing(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
        if let Some(f) = self.current_function() {
            f.update(keyboard, mouse, services);
        }
    }

    pub fn mouse_moved_hovering(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        services: &mut Services,
    ) {
        if let Some(f) = self.current_function() {
            f.hover(keyboard, mouse, services);
        }
    }

    pub fn mouse_released(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
        if let Some(f) = self.current_function() {
            f.end(keyboard, mouse, services);
        }
    }

    fn current_function(&mut self) -> Option<&mut Box<dyn ToolFunction>> {
        self.state.as_mut().map(|st| &mut st.current_function)
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

    pub fn add(&mut self, tool_proxy: ToolProxy) -> ToolProxyId {
        let id = ToolProxyId::new(Uuid::new_v4());
        self.proxies.insert(id, tool_proxy);

        id
    }
}

pub enum ToolEvent {
    Activate,
    Hover {
        keyboard: KeyboardState,
        mouse: HoverMouseState,
    },
    Begin {
        keyboard: KeyboardState,
        mouse: PressedMouseState,
    },
    Update {
        keyboard: KeyboardState,
        mouse: PressedMouseState,
    },
    End {
        keyboard: KeyboardState,
        mouse: PressedMouseState,
    },
    Deactivate,
}
