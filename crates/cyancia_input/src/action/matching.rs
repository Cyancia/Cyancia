use cyancia_assets::id::AssetId;
use iced_core::keyboard::key;
use indexmap::IndexSet;

use crate::{
    action::{Action, ActionCollection},
    key::KeySequence,
};

pub struct ActionMatcher {
    collection: ActionCollection,
    current_keys: IndexSet<key::Code>,
    current_action: Option<AssetId<Action>>,
}

impl ActionMatcher {
    pub fn new(collection: ActionCollection) -> Self {
        Self {
            collection,
            current_keys: IndexSet::new(),
            current_action: None,
        }
    }

    pub fn key_pressed(&mut self, key: key::Code) {
        self.current_keys.insert(key);
        self.update_current_action();
    }

    pub fn key_released(&mut self, key: key::Code) {
        self.current_keys.swap_remove(&key);
        self.update_current_action();
    }

    pub fn current_action(&self) -> Option<AssetId<Action>> {
        self.current_action
    }

    fn update_current_action(&mut self) {
        let Ok(keys) = KeySequence::from_codes(self.current_keys.clone().into_iter()) else {
            return;
        };

        self.current_action = self.collection.get_action(keys);
    }
}
