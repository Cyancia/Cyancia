use std::{
    collections::{HashMap, hash_map::Entry},
    rc::Rc,
};

use cyancia_utils::wrapper;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, Global, IntoElement, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Window, div,
};
use uuid::Uuid;
use wgpu::TextureView;

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

pub trait ToolFunction: Send + Sync + 'static + Sized {
    fn new(cx: &mut Context<Self>) -> Self;
    fn id() -> ToolId;
    fn activate(&mut self, _cx: &mut Context<Self>) {}
    fn hover(&mut self, _mouse: &MouseMoveEvent, _cx: &mut Context<Self>) {}
    fn begin(&mut self, _mouse: &MouseDownEvent, _cx: &mut Context<Self>) {}
    fn update(&mut self, _mouse: &MouseMoveEvent, _cx: &mut Context<Self>) {}
    fn end(&mut self, _mouse: &MouseUpEvent, _cx: &mut Context<Self>) {}
    fn deactivate(&mut self, _cx: &mut Context<Self>) {}
    // TODO Add on_keyboard that received keyboard events if the key strokes is not matching any
    //      actions.
    fn tool_option_widget(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }
    // TODO This should return gpui element and take canvas bounds + window + cx as parameter only,
    //      once gpui supports wgpu backend and allow custom shaders.
    fn canvas_overlay(
        &mut self,
        _canvas_surface: &TextureView,
        _window: &mut Window,
        _cx: &mut App,
    ) {
    }
}

pub struct ToolFunctionEntity<T: ToolFunction> {
    entity: Entity<T>,
}

pub trait ErasedToolFunction {
    fn id(&self) -> ToolId;
    fn activate(&mut self, cx: &mut App);
    fn hover(&mut self, mouse: &MouseMoveEvent, cx: &mut App);
    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut App);
    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut App);
    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut App);
    fn deactivate(&mut self, cx: &mut App);
    fn tool_option_widget(&mut self, window: &mut Window, cx: &mut App) -> AnyElement;
    fn canvas_overlay(&mut self, canvas_surface: &TextureView, window: &mut Window, cx: &mut App);
}

impl<T: ToolFunction> ErasedToolFunction for ToolFunctionEntity<T> {
    fn id(&self) -> ToolId {
        T::id()
    }

    fn activate(&mut self, cx: &mut App) {
        self.entity.update(cx, |entity, cx| entity.activate(cx));
    }

    fn hover(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {
        self.entity.update(cx, |entity, cx| entity.hover(mouse, cx));
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut App) {
        self.entity.update(cx, |entity, cx| entity.begin(mouse, cx));
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {
        self.entity
            .update(cx, |entity, cx| entity.update(mouse, cx));
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut App) {
        self.entity.update(cx, |entity, cx| entity.end(mouse, cx));
    }

    fn deactivate(&mut self, cx: &mut App) {
        self.entity.update(cx, |entity, cx| entity.deactivate(cx));
    }

    fn tool_option_widget(&mut self, window: &mut Window, cx: &mut App) -> AnyElement {
        self.entity
            .update(cx, |entity, cx| entity.tool_option_widget(window, cx))
    }

    fn canvas_overlay(&mut self, canvas_surface: &TextureView, window: &mut Window, cx: &mut App) {
        self.entity.update(cx, |entity, cx| {
            entity.canvas_overlay(canvas_surface, window, cx)
        });
    }
}

#[derive(Default)]
pub struct ToolFunctionRegistry {
    spawners: HashMap<ToolId, Rc<dyn Fn(&mut App) -> Box<dyn ErasedToolFunction> + Send + Sync>>,
}

impl Global for ToolFunctionRegistry {}

impl ToolFunctionRegistry {
    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    pub fn register<T: ToolFunction + Default>(&mut self) {
        self.spawners.insert(
            T::id(),
            Rc::new(|cx| {
                let entity = cx.new(|cx| T::new(cx));
                Box::new(ToolFunctionEntity { entity })
            }),
        );
    }
}

struct State {
    current_function: ToolId,
    is_updating: bool,
}

#[derive(Default)]
pub struct ToolProxy {
    state: Option<State>,
    tool_functions: HashMap<ToolId, Box<dyn ErasedToolFunction>>,
}

impl ToolProxy {
    pub fn switch_tool(&mut self, tool: ToolId, cx: &mut App) {
        if self
            .state
            .as_ref()
            .is_some_and(|s| s.current_function == tool)
        {
            return;
        }

        if let Some(st) = self.state.take() {
            self.tool_functions
                .get_mut(&st.current_function)
                .unwrap()
                .deactivate(cx);
        }

        let new_tool = match self.tool_functions.entry(tool) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let registry = ToolFunctionRegistry::global(cx);
                if let Some(new_tool) = registry.spawners.get(&tool).cloned() {
                    e.insert(new_tool(cx))
                } else {
                    log::error!(
                        "Unable to switch to tool {:?}: not found in registry.",
                        tool
                    );
                    return;
                }
            }
        };

        new_tool.activate(cx);
        self.state = Some(State {
            current_function: tool,
            is_updating: false,
        });
    }

    pub fn mouse_pressed(&mut self, mouse: &MouseDownEvent, cx: &mut App) {
        if let Some(state) = self.state.as_mut() {
            if state.is_updating {
                return;
            }
            state.is_updating = true;

            self.tool_functions
                .get_mut(&state.current_function)
                .unwrap()
                .begin(mouse, cx);
        }
    }

    pub fn mouse_moved(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {
        if let Some(state) = self.state.as_ref() {
            if state.is_updating {
                self.tool_functions
                    .get_mut(&state.current_function)
                    .unwrap()
                    .update(mouse, cx);
            } else {
                self.tool_functions
                    .get_mut(&state.current_function)
                    .unwrap()
                    .hover(mouse, cx);
            }
        }
    }

    pub fn mouse_released(&mut self, mouse: &MouseUpEvent, cx: &mut App) {
        if let Some(state) = self.state.as_mut() {
            if !state.is_updating {
                return;
            }

            state.is_updating = false;
            self.tool_functions
                .get_mut(&state.current_function)
                .unwrap()
                .end(mouse, cx);
        }
    }

    pub fn tool_option_widget(&mut self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let state = self.state.as_mut()?;
        Some(
            self.tool_functions
                .get_mut(&state.current_function)
                .unwrap()
                .tool_option_widget(window, cx),
        )
    }

    pub fn canvas_overlay(
        &mut self,
        canvas_surface: &TextureView,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(state) = self.state.as_mut() {
            self.tool_functions
                .get_mut(&state.current_function)
                .unwrap()
                .canvas_overlay(canvas_surface, window, cx);
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
