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
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub ToolId : &'static str
}

pub struct Tool {
    pub binded_action: ActionId,
}

pub trait ToolFunction: Send + Sync + 'static {
    type Message: Send + Sync + 'static;

    fn id() -> ToolId;
    fn activate(&mut self, services: &mut Services) -> Task<Self::Message> {
        Task::none()
    }
    fn hover(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        Task::none()
    }
    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        Task::none()
    }
    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        Task::none()
    }
    fn end(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        Task::none()
    }
    fn deactivate(&mut self, services: &mut Services) -> Task<Self::Message> {
        Task::none()
    }
    fn handle_message(
        &mut self,
        message: Self::Message,
        services: &mut Services,
    ) -> Task<Self::Message> {
        Task::none()
    }
}

pub trait ErasedToolFunction: Send + Sync + 'static {
    fn id(&self) -> ToolId;
    fn activate(&mut self, services: &mut Services) -> Task<Box<dyn Any + Send + Sync>>;
    fn hover(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn end(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn deactivate(&mut self, services: &mut Services) -> Task<Box<dyn Any + Send + Sync>>;
    fn handle_message(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>>;
}

impl<T: ToolFunction> ErasedToolFunction for T {
    fn id(&self) -> ToolId {
        T::id()
    }

    fn activate(&mut self, services: &mut Services) -> Task<Box<dyn Any + Send + Sync>> {
        self.activate(services)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn hover(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        self.hover(keyboard, mouse, services)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        self.begin(keyboard, mouse, services)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        self.update(keyboard, mouse, services)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn end(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        self.end(keyboard, mouse, services)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn deactivate(&mut self, services: &mut Services) -> Task<Box<dyn Any + Send + Sync>> {
        self.deactivate(services)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn handle_message(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        let message = message
            .downcast::<T::Message>()
            .expect("Invalid message type passed to tool function.");
        self.handle_message(*message, services)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }
}

#[derive(Debug)]
pub struct ErasedToolFunctionMessage {
    pub tool_id: ToolId,
    pub message: Box<dyn Any + Send + Sync>,
}

#[derive(Default)]
pub struct ToolFunctionRegistry {
    spawners: HashMap<ToolId, Box<dyn Fn() -> Box<dyn ErasedToolFunction> + Send + Sync>>,
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
    current_function: Box<dyn ErasedToolFunction>,
}

pub struct ToolProxy {
    state: Option<State>,
}

impl ToolProxy {
    pub fn new() -> Self {
        Self { state: None }
    }

    pub fn switch_tool(
        &mut self,
        tool: ToolId,
        services: &mut Services,
    ) -> Task<ErasedToolFunctionMessage> {
        let (deactivate, last) = match self.state.take() {
            Some(mut st) => (
                st.current_function
                    .deactivate(services)
                    .map(move |message| ErasedToolFunctionMessage {
                        tool_id: st.current,
                        message,
                    }),
                st.current,
            ),
            None => (Task::none(), tool),
        };

        let registry = services.service::<ToolFunctionRegistry>();
        if let Some(new_tool) = registry.spawners.get(&tool) {
            let mut f = new_tool();
            let active = f
                .activate(services)
                .map(move |message| ErasedToolFunctionMessage {
                    tool_id: tool,
                    message,
                });

            self.state = Some(State {
                last,
                current: tool,
                last_switch: Instant::now(),
                current_function: f,
            });

            deactivate.chain(active)
        } else {
            log::error!(
                "Unable to switch to tool {:?}: not found in registry.",
                tool
            );

            deactivate
        }
    }

    pub fn mouse_pressed(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<ErasedToolFunctionMessage> {
        if let Some((id, f)) = self.current_function() {
            f.begin(keyboard, mouse, services)
                .map(move |message| ErasedToolFunctionMessage {
                    tool_id: id,
                    message,
                })
        } else {
            Task::none()
        }
    }

    pub fn mouse_moved_pressing(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<ErasedToolFunctionMessage> {
        if let Some((id, f)) = self.current_function() {
            f.update(keyboard, mouse, services)
                .map(move |message| ErasedToolFunctionMessage {
                    tool_id: id,
                    message,
                })
        } else {
            Task::none()
        }
    }

    pub fn mouse_moved_hovering(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        services: &mut Services,
    ) -> Task<ErasedToolFunctionMessage> {
        if let Some((id, f)) = self.current_function() {
            f.hover(keyboard, mouse, services)
                .map(move |message| ErasedToolFunctionMessage {
                    tool_id: id,
                    message,
                })
        } else {
            Task::none()
        }
    }

    pub fn mouse_released(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<ErasedToolFunctionMessage> {
        if let Some((id, f)) = self.current_function() {
            f.end(keyboard, mouse, services)
                .map(move |message| ErasedToolFunctionMessage {
                    tool_id: id,
                    message,
                })
        } else {
            Task::none()
        }
    }

    pub fn handle_message(
        &mut self,
        message: ErasedToolFunctionMessage,
        services: &mut Services,
    ) -> Task<ErasedToolFunctionMessage> {
        // TODO: Store all tools to avoid discarding message.
        if let Some((id, f)) = self.current_function() {
            if id == message.tool_id {
                f.handle_message(message.message, services)
                    .map(move |message| ErasedToolFunctionMessage {
                        tool_id: id,
                        message,
                    })
            } else {
                Task::none()
            }
        } else {
            Task::none()
        }
    }

    fn current_function(&mut self) -> Option<(ToolId, &mut Box<dyn ErasedToolFunction>)> {
        self.state
            .as_mut()
            .map(|st| (st.current, &mut st.current_function))
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
