use std::{collections, sync::Arc, time::Instant};

use cyancia_id::Id;
use iced_core::keyboard::{Key, key};
use indexmap::IndexSet;

use crate::{
    action::{Action, ActionCollection, ActionType},
    key::KeySequence,
};

#[derive(Debug)]
pub struct ActionChange {
    pub finished: Option<(Id<Action>, Arc<Action>)>,
    pub started: Option<(Id<Action>, Arc<Action>)>,
}

pub struct ActionMatcher {
    collection: ActionCollection,
    current_keys: IndexSet<key::Code>,
    current_action: Option<(Id<Action>, Arc<Action>)>,
    last_matched: Instant,
}

impl ActionMatcher {
    pub fn new(collection: ActionCollection) -> Self {
        Self {
            collection,
            current_keys: IndexSet::new(),
            current_action: None,
            last_matched: Instant::now(),
        }
    }

    pub fn key_pressed(&mut self, key: key::Code) -> ActionChange {
        self.current_keys.insert(key);
        let previous = self.current_action.take();
        self.update_action();
        self.last_matched = Instant::now();
        ActionChange {
            finished: previous,
            started: self.current_action.clone(),
        }
    }

    pub fn key_released(&mut self, key: key::Code) -> ActionChange {
        self.current_keys.swap_remove(&key);
        let previous = self.current_action.take();
        self.update_action();
        ActionChange {
            finished: previous.filter(|(_, a)| match a.ty {
                ActionType::OneShot => false,
                ActionType::Toggle => self.last_matched.elapsed().as_secs_f32() > 0.2,
                ActionType::Hold => true,
            }),
            started: self.current_action.clone(),
        }
    }

    pub fn current_action(&self) -> Option<(Id<Action>, Arc<Action>)> {
        self.current_action.clone()
    }

    fn update_action(&mut self) {
        self.current_action = self.matched_action();
    }

    fn matched_action(&mut self) -> Option<(Id<Action>, Arc<Action>)> {
        let keys = KeySequence::from_codes(self.current_keys.iter().cloned()).ok()?;
        let id = self.collection.get_action_id(keys)?;
        let action = self.collection.get_action(id)?.clone();
        Some((id, action))
    }
}
