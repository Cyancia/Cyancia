use std::{any::Any, collections::HashMap, sync::Arc, time::Instant};

use cyancia_utils::wrapper;
use futures::{
    SinkExt,
    channel::mpsc::{Receiver, Sender, UnboundedReceiver, UnboundedSender},
};
use gpui::{App, Global, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use uuid::Uuid;

pub fn init(cx: &mut App) {
    cx.set_global(ToolFunctionRegistry::default());
    cx.set_global(ToolProxies::default());
}

// pub struct ToolsPlugin;

// impl Plugin for ToolsPlugin {
//     fn build(&self, app: &mut Application) {
//         app.add_service::<ToolFunctionRegistry>()
//             .add_service::<ToolProxies>();
//     }
// }

pub trait ToolsAppExt {
    fn add_tool_function<T: ToolFunction + Default>(&mut self);
}

impl ToolsAppExt for App {
    fn add_tool_function<T: ToolFunction + Default>(&mut self) {
        self.global_mut::<ToolFunctionRegistry>().register::<T>();
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub ToolId : &'static str
}

pub trait ToolFunction: Send + Sync + 'static {
    fn id() -> ToolId;
    fn activate(&mut self, cx: &mut App) {}
    fn hover(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {}
    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut App) {}
    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {}
    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut App) {}
    fn deactivate(&mut self, cx: &mut App) {}
}

pub trait ErasedToolFunction: Send + Sync + 'static {
    fn id(&self) -> ToolId;
    fn activate(&mut self, cx: &mut App);
    fn hover(&mut self, mouse: &MouseMoveEvent, cx: &mut App);
    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut App);
    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut App);
    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut App);
    fn deactivate(&mut self, cx: &mut App);
}

impl<T: ToolFunction> ErasedToolFunction for T {
    fn id(&self) -> ToolId {
        T::id()
    }

    fn activate(&mut self, cx: &mut App) {
        <T as ToolFunction>::activate(self, cx)
    }

    fn hover(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {
        <T as ToolFunction>::hover(self, mouse, cx)
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut App) {
        <T as ToolFunction>::begin(self, mouse, cx)
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {
        <T as ToolFunction>::update(self, mouse, cx)
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut App) {
        <T as ToolFunction>::end(self, mouse, cx)
    }

    fn deactivate(&mut self, cx: &mut App) {
        <T as ToolFunction>::deactivate(self, cx)
    }
}

#[derive(Default)]
pub struct ToolFunctionRegistry {
    spawners: HashMap<ToolId, Box<dyn Fn() -> Box<dyn ErasedToolFunction> + Send + Sync>>,
}

impl Global for ToolFunctionRegistry {}

impl ToolFunctionRegistry {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn register<T: ToolFunction + Default>(&mut self) {
        self.spawners
            .insert(T::default().id(), Box::new(|| Box::new(T::default())));
    }
}

// impl Service for ToolFunctionRegistry {}

struct State {
    last: ToolId,
    current: ToolId,
    last_switch: Instant,
    current_function: Box<dyn ErasedToolFunction>,
    is_updating: bool,
}

pub struct ToolProxy {
    state: Option<State>,
}

impl ToolProxy {
    pub fn new() -> Self {
        Self { state: None }
    }

    pub fn switch_tool(&mut self, tool: ToolId, cx: &mut App) {
        let last = match self.state.take() {
            Some(mut st) => {
                st.current_function.deactivate(cx);
                st.current
            }
            None => tool,
        };

        let registry = ToolFunctionRegistry::global(cx);
        if let Some(new_tool) = registry.spawners.get(&tool) {
            let mut f = new_tool();
            f.activate(cx);

            self.state = Some(State {
                last,
                current: tool,
                last_switch: Instant::now(),
                current_function: f,
                is_updating: false,
            });
        } else {
            log::error!(
                "Unable to switch to tool {:?}: not found in registry.",
                tool
            );
        }
    }

    pub fn mouse_pressed(&mut self, mouse: &MouseDownEvent, cx: &mut App) {
        if let Some(state) = self.state.as_mut() {
            if state.is_updating {
                return;
            }
            state.is_updating = true;
            state.current_function.begin(mouse, cx);
        }
    }

    pub fn mouse_moved(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {
        if let Some(state) = self.state.as_mut() {
            if state.is_updating {
                state.current_function.update(mouse, cx);
            } else {
                state.current_function.hover(mouse, cx);
            }
        }
    }

    pub fn mouse_released(&mut self, mouse: &MouseUpEvent, cx: &mut App) {
        if let Some(state) = self.state.as_mut() {
            if !state.is_updating {
                return;
            }

            state.is_updating = false;
            state.current_function.end(mouse, cx);
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

impl Global for ToolProxies {}

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
