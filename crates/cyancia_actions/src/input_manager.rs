use std::sync::Arc;

use cyancia_canvas::{CCanvas, CanvasManager};
use cyancia_input::{
    action::ActionCollection,
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

pub struct InputManager {
    actions: ActionCollection,
    keyboard_state: KeyboardState,

    is_pressed: bool,
    cursor_position: Point,
}

impl InputManager {
    pub fn new(actions: ActionCollection) -> Self {
        Self {
            actions,
            keyboard_state: KeyboardState::default(),
            is_pressed: false,
            cursor_position: Point::default(),
        }
    }

    pub fn on_keyboard_event(
        &mut self,
        event: keyboard::Event,
        runtime: Arc<Services>,
    ) -> Task<()> {
        match event {
            keyboard::Event::KeyPressed {
                physical_key,
                repeat,
                ..
            } => {
                if repeat {
                    return Task::none();
                }

                match physical_key {
                    key::Physical::Code(code) => {
                        self.keyboard_state.press(code);

                        if let Some(action) = self
                            .keyboard_state
                            .get_sequence()
                            .ok()
                            .and_then(|k| self.actions.get_action_id(k))
                            .and_then(|id| runtime.service::<ActionFunctionRegistry>().get(id))
                        {
                            log::info!("Triggering action: {}", action.id());
                            return action.trigger(runtime);
                        }
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

        Task::none()
    }

    pub fn on_mouse_event(&mut self, event: mouse::Event, services: &Services) {
        let mut tool_proxies = services.service_mut::<ToolProxies>();
        let canvas_manager = services.service::<CanvasManager>();
        let Some(canvas) = canvas_manager.current() else {
            return;
        };
        let canvas = canvas.as_ref();
        let tool_proxy = tool_proxies.get_mut(&canvas.tool_proxy_id);

        match event {
            mouse::Event::ButtonPressed(button) => {
                if button != mouse::Button::Left {
                    return;
                }

                self.is_pressed = true;
                tool_proxy.mouse_pressed(
                    &self.keyboard_state,
                    &PressedMouseState {
                        position: self.cursor_position,
                    },
                );
            }
            mouse::Event::ButtonReleased(button) => {
                if button != mouse::Button::Left {
                    return;
                }

                self.is_pressed = false;
                tool_proxy.mouse_released(
                    &self.keyboard_state,
                    &PressedMouseState {
                        position: self.cursor_position,
                    },
                );
            }
            mouse::Event::CursorMoved { position } => {
                self.cursor_position = position;

                if self.is_pressed {
                    tool_proxy.mouse_moved_pressing(
                        &self.keyboard_state,
                        &PressedMouseState {
                            position: self.cursor_position,
                        },
                    );
                } else {
                    tool_proxy.mouse_moved_hovering(
                        &self.keyboard_state,
                        &HoverMouseState {
                            position: self.cursor_position,
                        },
                    );
                }
            }
            mouse::Event::CursorLeft => {
                // FIXME
                // This is a workaround. When we pressed ctrl+o to open a file dialog,
                // the release event failed to be captured, causing the keyboard state to be stuck.
                self.keyboard_state = KeyboardState::default();
            }
            _ => {}
        }
    }
}
