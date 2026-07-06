use std::{
    collections::{HashMap, hash_map::Entry},
    rc::Rc,
    sync::Arc,
};

use cyancia_assets::AssetAppExt;
use cyancia_utils::wrapper;
use gpui::{
    AnyElement, App, AppContext, BorrowAppContext, Context, Entity, Global, InteractiveElement,
    IntoElement, Keystroke, Modifiers, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement,
    RenderOnce, Styled, Window, div,
};
use indexmap::IndexSet;
use log::info;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use wgpu::TextureView;

use crate::manifest::{ToolBinding, ToolBindingManifest, ToolBindingManifestSerializer};

pub mod manifest;

pub fn init(cx: &mut App) {
    cx.set_global(ToolFunctionRegistry::default());
    cx.set_global(ToolProxies::default());
    cx.set_global(TrackedKeys::default());

    cx.add_asset_serializer::<ToolBindingManifestSerializer>();
}

pub fn finish(cx: &mut App) {
    let manifests = cx.assets().all_handles_of::<ToolBindingManifest>().unwrap();
    // TODO select other manifests on demand
    let manifest = manifests.first().unwrap().get().unwrap();

    let mut bindings = GlobalToolBindings::default();
    for binding in &manifest.bindings {
        let keystroke = Keystroke::parse(&binding.shortcut).unwrap();
        bindings.bindings.insert(keystroke, binding.clone());
    }
    cx.set_global(bindings);
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
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Display, Serialize, Deserialize)]
    #[display("{0}")]
    pub ToolId : Arc<str>
}

pub trait ToolFunction: Send + Sync + 'static + Sized {
    fn new(cx: &mut Context<Self>) -> Self;
    fn id() -> ToolId;
    fn activate(&mut self, _: &mut Context<Self>) {}
    fn hover(&mut self, _: &MouseMoveEvent, _: &mut Context<Self>) {}
    fn begin(&mut self, _: &MouseDownEvent, _: &mut Context<Self>) {}
    fn update(&mut self, _: &MouseMoveEvent, _: &mut Context<Self>) {}
    fn end(&mut self, _: &MouseUpEvent, _: &mut Context<Self>) {}
    fn deactivate(&mut self, _: &mut Context<Self>) {}
    // TODO Add on_keyboard that received keyboard events if the key strokes is not matching any
    //      actions.
    fn tool_option_widget(&mut self, _: &mut Window, _: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }
    // TODO This should return gpui element and take canvas bounds + window + cx as parameter only,
    //      once gpui supports wgpu backend and allow custom shaders.
    fn canvas_overlay(&mut self, _: &TextureView, _: &mut Window, _: &mut App) {}
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
    function: ToolId,
    is_updating: bool,
}

#[derive(Default)]
pub struct ToolProxy {
    current_state: Option<State>,
    override_state: Option<State>,
    tool_functions: HashMap<ToolId, Box<dyn ErasedToolFunction>>,
}

impl ToolProxy {
    pub fn switch_tool(&mut self, tool: ToolId, cx: &mut App) {
        if Some(&tool) == self.current_tool() {
            return;
        }

        info!("Switched tool: {}", tool);

        if let Some(st) = self.current_state.take() {
            self.tool_functions
                .get_mut(&st.function)
                .unwrap()
                .deactivate(cx);
        }

        let new_tool = match self.tool_functions.entry(tool.clone()) {
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
        self.current_state = Some(State {
            function: tool,
            is_updating: false,
        });
    }

    pub fn switch_override_tool(&mut self, tool: Option<ToolId>, cx: &mut App) {
        if tool.as_ref() == self.override_tool() {
            return;
        }

        info!("Switched override tool: {:?}", tool);

        if let Some(tool) = tool {
            if let Some(state) = self.override_state.as_mut().or(self.current_state.as_mut()) {
                self.tool_functions
                    .get_mut(&state.function)
                    .unwrap()
                    .deactivate(cx);
            }

            let new_tool = match self.tool_functions.entry(tool.clone()) {
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

            self.override_state = Some(State {
                function: tool,
                is_updating: false,
            });
        } else {
            if let Some(state) = self.override_state.as_mut() {
                self.tool_functions
                    .get_mut(&state.function)
                    .unwrap()
                    .deactivate(cx);
            }
            if let Some(state) = self.current_state.as_mut() {
                self.tool_functions
                    .get_mut(&state.function)
                    .unwrap()
                    .activate(cx);
            }
            self.override_state = None;
        }
    }

    pub fn mouse_pressed(&mut self, mouse: &MouseDownEvent, cx: &mut App) {
        if let Some(state) = self.override_state.as_mut().or(self.current_state.as_mut()) {
            if state.is_updating {
                return;
            }
            state.is_updating = true;

            self.tool_functions
                .get_mut(&state.function)
                .unwrap()
                .begin(mouse, cx);
        }
    }

    pub fn mouse_moved(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {
        if let Some(state) = self.override_state.as_ref().or(self.current_state.as_ref()) {
            if state.is_updating {
                self.tool_functions
                    .get_mut(&state.function)
                    .unwrap()
                    .update(mouse, cx);
            } else {
                self.tool_functions
                    .get_mut(&state.function)
                    .unwrap()
                    .hover(mouse, cx);
            }
        }
    }

    pub fn mouse_released(&mut self, mouse: &MouseUpEvent, cx: &mut App) {
        if let Some(state) = self.override_state.as_mut().or(self.current_state.as_mut()) {
            if !state.is_updating {
                return;
            }

            state.is_updating = false;
            self.tool_functions
                .get_mut(&state.function)
                .unwrap()
                .end(mouse, cx);
        }
    }

    pub fn tool_option_widget(&mut self, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let state = self
            .override_state
            .as_mut()
            .or(self.current_state.as_mut())?;
        Some(
            self.tool_functions
                .get_mut(&state.function)
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
        if let Some(state) = self.override_state.as_mut().or(self.current_state.as_mut()) {
            self.tool_functions
                .get_mut(&state.function)
                .unwrap()
                .canvas_overlay(canvas_surface, window, cx);
        }
    }

    pub fn current_tool(&self) -> Option<&ToolId> {
        Some(&self.current_state.as_ref()?.function)
    }

    pub fn override_tool(&self) -> Option<&ToolId> {
        Some(&self.override_state.as_ref()?.function)
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

#[derive(Default)]
pub struct GlobalToolBindings {
    bindings: HashMap<Keystroke, ToolBinding>,
}

impl Global for GlobalToolBindings {}

#[derive(Default)]
pub struct TrackedKeys {
    keys: IndexSet<String>,
    modifiers: Modifiers,
}

impl Global for TrackedKeys {}

#[derive(IntoElement, Default)]
pub struct ToolLayer {
    children: Vec<AnyElement>,
    target_tool_proxy: Option<ToolProxyId>,
}

impl ToolLayer {
    pub fn tool_proxy(mut self, tool_proxy: ToolProxyId) -> Self {
        self.target_tool_proxy = Some(tool_proxy);
        self
    }
}

impl ParentElement for ToolLayer {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ToolLayer {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let target_tool_proxy = self.target_tool_proxy;

        // window.on_key_event(|event: &KeyDownEvent, phase, window, cx| {
        //     dbg!(&event.keystroke);
        // });

        div()
            .size_full()
            .children(self.children)
            .on_key_down(move |event, _, cx| {
                if event.is_held {
                    return;
                }

                cx.update_global::<TrackedKeys, _>(|tracked, cx| {
                    tracked.keys.insert(event.keystroke.key.clone());

                    switch_tool(target_tool_proxy, tracked, cx, true);
                });
            })
            .on_key_up(move |event, _, cx| {
                cx.update_global::<TrackedKeys, _>(|tracked, cx| {
                    tracked.keys.shift_remove(&event.keystroke.key);

                    switch_tool(target_tool_proxy, tracked, cx, false);
                });
            })
            .on_modifiers_changed(move |event, _, cx| {
                cx.update_global::<TrackedKeys, _>(|tracked, cx| {
                    let old_count = count_modifiers(&tracked.modifiers);
                    tracked.modifiers = event.modifiers;
                    let new_count = count_modifiers(&tracked.modifiers);
                    switch_tool(target_tool_proxy, tracked, cx, new_count > old_count);
                });
            })
    }
}

fn count_modifiers(m: &Modifiers) -> u32 {
    let mut n = 0;
    if m.shift {
        n += 1;
    }
    if m.control {
        n += 1;
    }
    if m.alt {
        n += 1;
    }
    if m.platform {
        n += 1;
    }
    if m.function {
        n += 1;
    }

    n
}

fn switch_tool(
    tool_proxy: Option<ToolProxyId>,
    tracked_keys: &TrackedKeys,
    cx: &mut App,
    is_keydown: bool,
) {
    let Some(tool_proxy) = tool_proxy else {
        return;
    };

    let current_key = tracked_keys.keys.last().cloned();
    let mut modifiers = tracked_keys.modifiers;

    let current_keystroke = if let Some(key) = current_key {
        Some(Keystroke {
            modifiers,
            key,
            key_char: None,
        })
    } else {
        use std::mem;

        let key = if mem::take(&mut modifiers.shift) {
            Some("shift".to_string())
        } else if mem::take(&mut modifiers.control) {
            Some("control".to_string())
        } else if mem::take(&mut modifiers.alt) {
            Some("alt".to_string())
        } else if mem::take(&mut modifiers.platform) {
            Some("platform".to_string())
        } else if mem::take(&mut modifiers.function) {
            Some("function".to_string())
        } else {
            None
        };

        key.map(|key| Keystroke {
            modifiers,
            key,
            key_char: None,
        })
    };

    let bindings = cx.global::<GlobalToolBindings>();
    let Some(config) = current_keystroke.and_then(|k| bindings.bindings.get(&k)) else {
        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            let tool_proxy = tool_proxies.get_mut(&tool_proxy);
            tool_proxy.switch_override_tool(None, cx);
        });
        return;
    };

    let tool_id = config.tool.clone();
    if config.is_temporary {
        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            let tool_proxy = tool_proxies.get_mut(&tool_proxy);
            tool_proxy.switch_override_tool(Some(tool_id), cx);
        });
    } else if is_keydown {
        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            let tool_proxy = tool_proxies.get_mut(&tool_proxy);
            tool_proxy.switch_tool(tool_id, cx);
        });
    }

    // let tool_id = config.tool_id;
    // cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
    //     let tool_proxy = tool_proxies.get_mut(&tool_proxy);
    //     if is_override {
    //         tool_proxy.switch_override_tool(Some(tool_id), cx);
    //     } else {
    //         tool_proxy.switch_tool(tool_id, cx);
    //     }
    // });
}
