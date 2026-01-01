use std::{any::Any, collections::HashMap, sync::Arc};

use cyancia_canvas::CCanvas;
use cyancia_id::Id;
use cyancia_input::{
    action::Action,
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use iced_core::{Point, keyboard::key, mouse};
use parking_lot::RwLock;

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
    actions: HashMap<Id<CanvasTool>, Box<dyn CanvasToolFunction>>,
}

impl CanvasToolFunctionCollection {
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
        }
    }

    pub fn register<A: CanvasToolFunction + Default>(&mut self) {
        let action = A::default();
        self.actions.insert(action.id(), Box::new(action));
    }

    pub fn get(&self, id: &Id<CanvasTool>) -> Option<&Box<dyn CanvasToolFunction>> {
        self.actions.get(id)
    }

    pub fn get_mut(&mut self, id: &Id<CanvasTool>) -> Option<&mut Box<dyn CanvasToolFunction>> {
        self.actions.get_mut(id)
    }
}

pub struct ToolProxy {
    last: Id<CanvasTool>,
    current: Id<CanvasTool>,
    tools: CanvasToolFunctionCollection,
}

impl ToolProxy {
    pub fn new(initial: Id<CanvasTool>, collection: CanvasToolFunctionCollection) -> Self {
        Self {
            last: initial.clone(),
            current: initial,
            tools: collection,
        }
    }

    pub fn switch_tool(&mut self, tool: Id<CanvasTool>, canvas: &CCanvas) {
        if let Some(current_tool) = self.tools.get_mut(&self.current) {
            current_tool.deactivate(canvas);
        }

        self.last = self.current;
        self.current = tool;

        if let Some(new_tool) = self.tools.get_mut(&self.current) {
            new_tool.activate(canvas);
        }
    }

    pub fn switch_tool_back(&mut self, canvas: &CCanvas) {
        self.switch_tool(self.last.clone(), canvas);
    }

    pub fn mouse_pressed(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.current) {
            tool.begin(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_moved_pressing(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.current) {
            tool.update(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_moved_hovering(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &HoverMouseState,
        canvas: &CCanvas,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.current) {
            tool.hover(keyboard, mouse, canvas);
        }
    }

    pub fn mouse_released(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        canvas: &CCanvas,
    ) {
        if let Some(tool) = self.tools.get_mut(&self.current) {
            tool.end(keyboard, mouse, canvas);
        }
    }
}
