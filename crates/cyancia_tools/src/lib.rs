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
    tx: UnboundedSender<ToolEvent>,
}

pub struct ToolProxy {
    state: Option<State>,
}

impl ToolProxy {
    pub fn new() -> Self {
        Self { state: None }
    }

    pub fn switch_tool(&mut self, tool: ToolId, services: Arc<Services>) -> Task<()> {
        let last = match self.state.take() {
            Some(st) => {
                self.try_send(ToolEvent::Deactivate);
                st.tx.close_channel();
                st.current
            }
            None => tool.clone(),
        };

        let (tx, rx) = futures::channel::mpsc::unbounded();
        self.state = Some(State {
            last,
            current: tool.clone(),
            last_switch: Instant::now(),
            tx,
        });

        let registry = services.service::<ToolFunctionRegistry>();
        if let Some(new_tool) = registry.spawners.get(&tool) {
            Task::future(run_tool_function(services, rx, new_tool()))
        } else {
            log::error!(
                "Unable to switch to tool {:?}: not found in registry.",
                tool
            );
            Task::none()
        }
    }

    pub fn mouse_pressed(&self, keyboard: &KeyboardState, mouse: &PressedMouseState) {
        self.try_send(ToolEvent::Begin {
            keyboard: keyboard.clone(),
            mouse: mouse.clone(),
        });
    }

    pub fn mouse_moved_pressing(&self, keyboard: &KeyboardState, mouse: &PressedMouseState) {
        self.try_send(ToolEvent::Update {
            keyboard: keyboard.clone(),
            mouse: mouse.clone(),
        });
    }

    pub fn mouse_moved_hovering(&self, keyboard: &KeyboardState, mouse: &HoverMouseState) {
        self.try_send(ToolEvent::Hover {
            keyboard: keyboard.clone(),
            mouse: mouse.clone(),
        });
    }

    pub fn mouse_released(&self, keyboard: &KeyboardState, mouse: &PressedMouseState) {
        self.try_send(ToolEvent::End {
            keyboard: keyboard.clone(),
            mouse: mouse.clone(),
        });
    }

    fn try_send(&self, event: ToolEvent) {
        if let Some(state) = self.state.as_ref() {
            match state.tx.unbounded_send(event) {
                Ok(_) => {}
                Err(err) => {
                    log::error!("Failed to send tool event: {:?}", err);
                }
            }
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

async fn run_tool_function(
    services: Arc<Services>,
    mut rx: UnboundedReceiver<ToolEvent>,
    mut tool: Box<dyn ToolFunction>,
) {
    while let Ok(ev) = rx.recv().await {
        match ev {
            ToolEvent::Activate => tool.activate(&services),
            ToolEvent::Hover { keyboard, mouse } => tool.hover(&keyboard, &mouse, &services),
            ToolEvent::Begin { keyboard, mouse } => tool.begin(&keyboard, &mouse, &services),
            ToolEvent::Update { keyboard, mouse } => tool.update(&keyboard, &mouse, &services),
            ToolEvent::End { keyboard, mouse } => tool.end(&keyboard, &mouse, &services),
            ToolEvent::Deactivate => tool.deactivate(&services),
        }
    }
}
