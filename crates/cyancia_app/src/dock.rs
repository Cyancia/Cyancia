use std::sync::Arc;

use bevy_math::Rect;
use cyancia_actions::{ActionFunctionRegistry, actions_matcher::ActionsMatcher};
use cyancia_canvas::{CanvasId, CanvasManager, event::CanvasRemoved, widget::CanvasWidget};
use cyancia_dock::dock::{Dock, DockId};
use cyancia_image::tile::GpuTileStorage;
use cyancia_input::{
    action::ActionCollection,
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_runtime::{Services, event::Event};
use cyancia_tools::ToolProxies;
use iced::widget::text;
use iced::{Theme, widget::space};
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

            fn view<'a>(
                &'a self,
                services: &'a Services,
            ) -> Element<'a, Self::Message, Theme, Renderer> {
                text($text).into()
            }

            fn update(&mut self, _message: (), services: &mut Services) -> Task<()> {
                Task::none()
            }
        }

        pub const $id: &'static str = $text;
    };
}

test_dummy_dock!(LayerDock, LAYER_DOCK_ID, "Layers");
test_dummy_dock!(ToolDock, TOOL_DOCK_ID, "Tools");
test_dummy_dock!(HistoryDock, HISTORY_DOCK_ID, "History");

pub fn construct_canvas_dock_id(canvas: CanvasId) -> String {
    format!("canvas_dock_{}", canvas)
}

pub struct CanvasDock {
    canvas: CanvasId,

    is_pressed: bool,
    cursor_position: Point,
    actions_matcher: Arc<Mutex<ActionsMatcher>>,
}

impl CanvasDock {
    pub fn new(canvas: CanvasId, actions_matcher: Arc<Mutex<ActionsMatcher>>) -> Self {
        Self {
            canvas,
            is_pressed: false,
            cursor_position: Point::default(),
            actions_matcher,
        }
    }
}

#[derive(Debug)]
pub enum CanvasDockMessage {
    CanvasFocus(Point),
    MouseEvent(mouse::Event),
    WidgetRectChange(Rect),
}

impl<Theme> Dock<Theme, iced_wgpu::Renderer> for CanvasDock
where
    Theme: 'static,
{
    type Message = CanvasDockMessage;

    fn id(&self) -> cyancia_dock::dock::DockId {
        DockId::new(construct_canvas_dock_id(self.canvas).into())
    }

    fn view<'a>(
        &'a self,
        services: &'a Services,
    ) -> iced_core::Element<'a, Self::Message, Theme, iced_wgpu::Renderer> {
        let canvas_manager = services.service::<CanvasManager>();
        let Some(canvas) = canvas_manager.get(&self.canvas) else {
            return space().into();
        };

        CanvasWidget {
            is_focusing: canvas_manager.current_id() == Some(self.canvas),
            canvas,
            tile_storage: services.service::<GpuTileStorage>().clone(),
            on_focus: Box::new(CanvasDockMessage::CanvasFocus),
            on_mouse_event: Box::new(CanvasDockMessage::MouseEvent),
            on_widget_rect_change: Box::new(CanvasDockMessage::WidgetRectChange),
        }
        .into()
    }

    fn update(&mut self, message: Self::Message, services: &mut Services) -> Task<Self::Message> {
        let actions_matcher = self.actions_matcher.lock();

        match message {
            CanvasDockMessage::MouseEvent(event) => {
                let canvas_manager = services.service_mut::<CanvasManager>();
                if canvas_manager.current_id() != Some(self.canvas) {
                    return Task::none();
                }

                let Some(canvas) = canvas_manager.current() else {
                    return Task::none();
                };
                let tool_proxy_id = canvas.tool_proxy_id;

                services.service_scope::<ToolProxies>(|tool_proxies, services| {
                    let tool_proxy = tool_proxies.get_mut(&tool_proxy_id);

                    match event {
                        mouse::Event::ButtonPressed(button) => {
                            if button != mouse::Button::Left {
                                return;
                            }

                            self.is_pressed = true;
                            tool_proxy.mouse_pressed(
                                &actions_matcher.keyboard_state(),
                                &PressedMouseState {
                                    position: self.cursor_position,
                                },
                                services,
                            );
                        }
                        mouse::Event::ButtonReleased(button) => {
                            if button != mouse::Button::Left {
                                return;
                            }

                            self.is_pressed = false;
                            tool_proxy.mouse_released(
                                &actions_matcher.keyboard_state(),
                                &PressedMouseState {
                                    position: self.cursor_position,
                                },
                                services,
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
                                    services,
                                );
                            } else {
                                tool_proxy.mouse_moved_hovering(
                                    &actions_matcher.keyboard_state(),
                                    &HoverMouseState {
                                        position: self.cursor_position,
                                    },
                                    services,
                                );
                            }
                        }
                        _ => {}
                    }
                });
            }
            CanvasDockMessage::CanvasFocus(cursor_pos) => {
                self.cursor_position = cursor_pos;
                let canvas_manager = services.service_mut::<CanvasManager>();
                canvas_manager.set_current(self.canvas);
            }
            CanvasDockMessage::WidgetRectChange(rect) => {
                let canvas_manager = services.service_mut::<CanvasManager>();
                if let Some(canvas) = canvas_manager.get_mut(&self.canvas) {
                    canvas.transform.widget_bounds = rect;
                }
            }
        }

        Task::none()
    }

    fn on_close(&mut self) -> Task<Self::Message> {
        CanvasRemoved::broadcast(CanvasRemoved { id: self.canvas });

        Task::none()
    }

    // fn subscription(&self) -> iced::Subscription<Self::Message> {
    //     iced::event::listen().filter_map(|e| {
    //         match e {
    //             iced::Event::Keyboard(event) => {
    //                 dbg!(event);
    //             }
    //             iced::Event::Mouse(event) => {}
    //             iced::Event::Window(event) => {}
    //             iced::Event::Touch(event) => {}
    //             iced::Event::InputMethod(event) => {}
    //         };
    //         None
    //     })
    // }
}
