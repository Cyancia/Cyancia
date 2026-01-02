use std::{any::Any, cell::UnsafeCell, collections::HashMap};

use cyancia_id::Id;
use cyancia_input::{
    action::{Action, ActionCollection},
    key::{KeySequence, KeyboardState},
    mouse::PressedMouseState,
};
use iced_core::Point;
use parking_lot::RwLock;

use crate::shell::ActionShell;

pub mod canvas_control;
pub mod file;
pub mod shell;
pub mod task;
pub mod utils;

pub trait ActionFunction: Send + Sync + 'static {
    fn id(&self) -> Id<Action>;
    fn trigger(&self, shell: &mut ActionShell);
}

pub struct ActionFunctionCollection {
    actions: ActionCollection,
    functions: HashMap<Id<Action>, Box<dyn ActionFunction>>,
}

impl ActionFunctionCollection {
    pub fn new(actions: ActionCollection) -> Self {
        Self {
            actions,
            functions: HashMap::new(),
        }
    }

    pub fn register<A: ActionFunction + Default>(&mut self) {
        let action = A::default();
        self.functions.insert(action.id(), Box::new(action));
    }

    pub fn trigger(&self, keys: KeySequence, shell: &mut ActionShell) {
        let Some(id) = self.actions.get_action_id(keys) else {
            return;
        };

        if let Some(action) = self.functions.get(&id) {
            action.trigger(shell);
        }
    }
}
