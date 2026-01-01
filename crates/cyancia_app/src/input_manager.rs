use std::sync::Arc;

use cyancia_actions::{
    ActionFunctionCollection,
    shell::{CShell, DestructedShell},
};
use cyancia_canvas::CCanvas;
use cyancia_input::{
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_tools::ToolProxy;
use iced::{
    Point,
    keyboard::{self, key},
    mouse,
};

pub struct InputManager {
    pub actions: ActionFunctionCollection,
    pub tools: ToolProxy,

    keyboard_state: KeyboardState,

    is_pressed: bool,
    cursor_position: Point,
}

impl InputManager {
    pub fn new(actions: ActionFunctionCollection, tools: ToolProxy) -> Self {
        Self {
            actions,
            tools,
            keyboard_state: KeyboardState::default(),
            is_pressed: false,
            cursor_position: Point::default(),
        }
    }

    pub fn on_keyboard_event(
        &mut self,
        event: keyboard::Event,
        canvas: Arc<CCanvas>,
    ) -> DestructedShell {
        let mut shell = CShell::new(canvas, &mut self.tools);

        loop {
            match event {
                keyboard::Event::KeyPressed {
                    physical_key,
                    repeat,
                    ..
                } => {
                    if repeat {
                        break;
                    }

                    match physical_key {
                        key::Physical::Code(code) => {
                            self.keyboard_state.press(code);

                            if let Ok(keys) = self.keyboard_state.get_sequence() {
                                self.actions.trigger(keys, &mut shell);
                            }
                        }
                        key::Physical::Unidentified(native_code) => {
                            log::error!("Unidentified key pressed: {:?}", native_code);
                        }
                    }
                }
                keyboard::Event::KeyReleased { physical_key, .. } => match physical_key {
                    key::Physical::Code(code) => {
                        if let Ok(keys) = self.keyboard_state.get_sequence() {
                            self.actions.end(keys, &mut shell);
                        }

                        self.keyboard_state.release(code);
                    }
                    key::Physical::Unidentified(native_code) => {
                        log::error!("Unidentified key released: {:?}", native_code);
                    }
                },
                _ => {}
            }

            break;
        }

        shell.destruct()
    }

    pub fn on_mouse_event(&mut self, event: mouse::Event, canvas: &CCanvas) {
        match event {
            mouse::Event::ButtonPressed(button) => {
                if button != mouse::Button::Left {
                    return;
                }

                self.is_pressed = true;
                self.tools.mouse_pressed(
                    &self.keyboard_state,
                    &PressedMouseState {
                        position: self.cursor_position,
                    },
                    canvas,
                );
            }
            mouse::Event::ButtonReleased(button) => {
                if button != mouse::Button::Left {
                    return;
                }

                self.is_pressed = false;
                self.tools.mouse_released(
                    &self.keyboard_state,
                    &PressedMouseState {
                        position: self.cursor_position,
                    },
                    canvas,
                );
            }
            mouse::Event::CursorMoved { position } => {
                self.cursor_position = position;

                if self.is_pressed {
                    self.tools.mouse_moved_pressing(
                        &self.keyboard_state,
                        &PressedMouseState {
                            position: self.cursor_position,
                        },
                        canvas,
                    );
                } else {
                    self.tools.mouse_moved_hovering(
                        &self.keyboard_state,
                        &HoverMouseState {
                            position: self.cursor_position,
                        },
                        canvas,
                    );
                }
            }
            _ => {}
        }
    }
}
