use std::sync::Arc;

use cyancia_canvas::{CCanvas, CanvasManager};
use cyancia_input::{
    action::{ActionCollection, ActionId},
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_runtime::Services;
use cyancia_tools::{ToolProxies, ToolProxy};
use iced_core::{
    Point,
    keyboard::{self, key},
    mouse,
};
use iced_runtime::Task;

use crate::ActionFunctionRegistry;

pub struct ActionsMatcher {
    actions: ActionCollection,
    keyboard_state: KeyboardState,
}

impl ActionsMatcher {
    pub fn new(actions: ActionCollection) -> Self {
        Self {
            actions,
            keyboard_state: KeyboardState::default(),
        }
    }

    pub fn on_keyboard_event(&mut self, event: keyboard::Event) -> Option<ActionId> {
        match event {
            keyboard::Event::KeyPressed {
                physical_key,
                repeat,
                ..
            } => {
                if repeat {
                    return None;
                }

                match physical_key {
                    key::Physical::Code(code) => {
                        self.keyboard_state.press(code);

                        let seq = self.keyboard_state.get_sequence().ok()?;
                        return self.actions.get_action_id(seq);
                    }
                    key::Physical::Unidentified(native_code) => {
                        log::error!("Unidentified key pressed: {:?}", native_code);
                    }
                }
            }
            keyboard::Event::KeyReleased { physical_key, .. } => match physical_key {
                key::Physical::Code(code) => {
                    self.keyboard_state.release(code);
                }
                key::Physical::Unidentified(native_code) => {
                    log::error!("Unidentified key released: {:?}", native_code);
                }
            },
            _ => {}
        }

        None
    }

    pub fn keyboard_state(&self) -> &KeyboardState {
        &self.keyboard_state
    }

    pub fn reset_keyboard_state(&mut self) {
        self.keyboard_state = Default::default();
    }
}
