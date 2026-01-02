use std::{any::Any, collections::HashMap, sync::Arc, time::Instant};

use cyancia_canvas::CCanvas;
use cyancia_id::Id;
use cyancia_input::{
    action::Action,
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use iced_core::{Point, keyboard::key, mouse};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};

pub mod brush;
pub mod pan;
pub mod rotate;
pub mod zoom;

pub struct CanvasTool {
    pub binded_action: Id<Action>,
}

pub trait CanvasToolFunction: Send + Sync + 'static {
    fn id(&self) -> Id<CanvasTool>;
    fn activate(&mut self, canvas: &CCanvas) {}
    fn hover(&mut self, keyboard: &KeyboardState, mouse: &HoverMouseState, canvas: &CCanvas) {}
    fn begin(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {}
    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {}
    fn end(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, canvas: &CCanvas) {}
    fn deactivate(&mut self, canvas: &CCanvas) {}
}

pub struct CanvasToolFunctionCollection {
    actions: HashMap<Id<CanvasTool>, Arc<RwLock<dyn CanvasToolFunction>>>,
}

impl CanvasToolFunctionCollection {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    pub fn register<A: CanvasToolFunction + Default>(&mut self) {
        let action = A::default();
        self.actions
            .insert(action.id(), Arc::new(RwLock::new(action)));
    }

    pub fn get(&self, id: &Id<CanvasTool>) -> Option<RwLockReadGuard<'_, dyn CanvasToolFunction>> {
        self.actions.get(id).map(|l| l.read())
    }

    pub fn get_mut(
        &self,
        id: &Id<CanvasTool>,
    ) -> Option<RwLockWriteGuard<'_, dyn CanvasToolFunction>> {
        self.actions.get(id).map(|l| l.write())
    }
}

struct ToolProxyState {
    last: Id<CanvasTool>,
    current: Id<CanvasTool>,
    last_switch: Instant,
}

pub struct ToolProxy {
    state: RwLock<ToolProxyState>,
    tools: CanvasToolFunctionCollection,
}

impl ToolProxy {
    pub fn new(initial: Id<CanvasTool>, collection: CanvasToolFunctionCollection) -> Self {
        Self {
            state: RwLock::new(ToolProxyState {
                last: initial.clone(),
                current: initial,
                last_switch: Instant::now(),
            }),
            tools: collection,
        }
    }

    pub fn switch_tool(&self, tool: Id<CanvasTool>, canvas: &CCanvas) {
        let mut state = self.state.write();
        if let Some(mut current_tool) = self.tools.get_mut(&state.current) {
            current_tool.deactivate(canvas);
        }

        state.last = state.current;
        state.current = tool;
        state.last_switch = Instant::now();

        if let Some(mut new_tool) = self.tools.get_mut(&state.current) {
            new_tool.activate(canvas);
        }
    }

    pub fn mouse_pressed(
        &self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        let state = self.state.read();
        if let Some(mut tool) = self.tools.get_mut(&state.current) {
            tool.begin(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_moved_pressing(
        &self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        let state = self.state.read();
        if let Some(mut tool) = self.tools.get_mut(&state.current) {
            tool.update(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_moved_hovering(
        &self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        canvas: &CCanvas,
    ) {
        let state = self.state.read();
        if let Some(mut tool) = self.tools.get_mut(&state.current) {
            tool.hover(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_released(
        &self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        let state = self.state.read();
        if let Some(mut tool) = self.tools.get_mut(&state.current) {
            tool.end(keyboard, mouse, canvas);
        }
    }
}
