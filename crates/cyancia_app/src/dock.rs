use std::sync::Arc;

use cyancia_actions::{ActionFunctionRegistry, actions_matcher::ActionsMatcher};
use cyancia_canvas::{CanvasId, CanvasManager, render::CanvasRenderers, widget::CanvasWidget};
use cyancia_dock::dock::{Dock, DockId};
use cyancia_image::tile::GpuTileStorage;
use cyancia_input::{
    action::ActionCollection,
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_runtime::Services;
use cyancia_tools::ToolProxies;
use iced::Theme;
use iced::widget::text;
use iced_core::{Element, Point, keyboard, mouse};
use iced_runtime::Task;
use iced_wgpu::Renderer;
use parking_lot::Mutex;

macro_rules! test_dummy_dock {
    ($name:ident, $id:ident, $text:expr) => {
        pub struct $name;

        impl Dock<Theme, Renderer> for $name {
            type Message = ();

            fn id(&self) -> DockId {
                DockId::new($text.into())
            }

            fn view(&self) -> Element<'_, Self::Message, Theme, Renderer> {
                text($text).into()
            }

            fn update(&mut self, _message: ()) -> Task<()> {
                Task::none()
            }
        }

        pub const $id: &'static str = $text;
    };
}

test_dummy_dock!(LayerDock, LAYER_DOCK_ID, "Layers");
test_dummy_dock!(ToolDock, TOOL_DOCK_ID, "Tools");
test_dummy_dock!(HistoryDock, HISTORY_DOCK_ID, "History");

pub struct CanvasDock {
    canvas: CanvasId,
    runtime: Arc<Services>,

    is_pressed: bool,
    cursor_position: Point,
    actions_matcher: Arc<Mutex<ActionsMatcher>>,
}

impl CanvasDock {
    pub fn new(
        canvas: CanvasId,
        runtime: Arc<Services>,
        actions_matcher: Arc<Mutex<ActionsMatcher>>,
    ) -> Self {
        Self {
            canvas,
            runtime,
            is_pressed: false,
            cursor_position: Point::default(),
            actions_matcher,
        }
    }
}

#[derive(Debug)]
pub enum CanvasDockMessage {
    KeyboardEvent(keyboard::Event),
    MouseEvent(mouse::Event),
}

impl<Theme> Dock<Theme, iced_wgpu::Renderer> for CanvasDock
where
    Theme: 'static,
{
    type Message = CanvasDockMessage;

    fn id(&self) -> cyancia_dock::dock::DockId {
        DockId::new(format!("canvas_dock_{}", self.canvas).into())
    }

    fn view<'a>(&'a self) -> iced_core::Element<'a, Self::Message, Theme, iced_wgpu::Renderer> {
        let canvas_manager = self.runtime.service::<CanvasManager>();
        let renderers = self.runtime.service::<CanvasRenderers>();
        let canvas = canvas_manager.get(&self.canvas).unwrap();
        let renderer = renderers.get(&self.canvas).unwrap();

        CanvasWidget {
            canvas,
            renderer,
            tile_storage: self.runtime.service::<GpuTileStorage>().clone(),
            on_keyboard_event: Box::new(CanvasDockMessage::KeyboardEvent),
            on_mouse_event: Box::new(CanvasDockMessage::MouseEvent),
        }
        .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        let mut actions_matcher = self.actions_matcher.lock();

        match message {
            CanvasDockMessage::KeyboardEvent(event) => {
                // TODO: It's a bad idea to handle keyboard events inside both canvas dock and inside main view.
                let task = if let Some(action) = actions_matcher.on_keyboard_event(event.clone())
                    && let Some(action_func) = self
                        .runtime
                        .service_mut::<ActionFunctionRegistry>()
                        .get(action.clone())
                {
                    log::info!("Triggering action: {}", action);
                    action_func.trigger(self.runtime.clone()).discard()
                } else {
                    Task::none()
                };

                // Don't borrow any service to avoid deadlock with trigger function.
                let mut tool_proxies = self.runtime.service_mut::<ToolProxies>();
                let canvas_manager = self.runtime.service::<CanvasManager>();
                let Some(canvas) = canvas_manager.current() else {
                    return Task::none();
                };
                let canvas = canvas.as_ref();
                let tool_proxy = tool_proxies.get_mut(&canvas.tool_proxy_id);
                if self.is_pressed {
                    tool_proxy.mouse_moved_pressing(
                        actions_matcher.keyboard_state(),
                        &PressedMouseState {
                            position: self.cursor_position,
                        },
                    );
                } else {
                    tool_proxy.mouse_moved_hovering(
                        actions_matcher.keyboard_state(),
                        &HoverMouseState {
                            position: self.cursor_position,
                        },
                    );
                }

                return task;
            }
            CanvasDockMessage::MouseEvent(event) => {
                let mut tool_proxies = self.runtime.service_mut::<ToolProxies>();
                let canvas_manager = self.runtime.service::<CanvasManager>();
                let Some(canvas) = canvas_manager.current() else {
                    return Task::none();
                };
                let canvas = canvas.as_ref();
                let tool_proxy = tool_proxies.get_mut(&canvas.tool_proxy_id);

                match event {
                    mouse::Event::ButtonPressed(button) => {
                        if button != mouse::Button::Left {
                            return Task::none();
                        }

                        self.is_pressed = true;
                        tool_proxy.mouse_pressed(
                            &actions_matcher.keyboard_state(),
                            &PressedMouseState {
                                position: self.cursor_position,
                            },
                        );
                    }
                    mouse::Event::ButtonReleased(button) => {
                        if button != mouse::Button::Left {
                            return Task::none();
                        }

                        self.is_pressed = false;
                        tool_proxy.mouse_released(
                            &actions_matcher.keyboard_state(),
                            &PressedMouseState {
                                position: self.cursor_position,
                            },
                        );
                    }
                    mouse::Event::CursorMoved { position } => {
                        self.cursor_position = position;

                        if self.is_pressed {
                            tool_proxy.mouse_moved_pressing(
                                &actions_matcher.keyboard_state(),
                                &PressedMouseState {
                                    position: self.cursor_position,
                                },
                            );
                        } else {
                            tool_proxy.mouse_moved_hovering(
                                &actions_matcher.keyboard_state(),
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
                        actions_matcher.reset_keyboard_state();
                    }
                    _ => {}
                }
            }
        }

        Task::none()
    }
}
