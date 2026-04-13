use std::sync::Arc;

use cyancia_actions::{ActionFunctionRegistry, actions_matcher::ActionsMatcher};
use cyancia_canvas::{
    CanvasId, CanvasManager, event::CanvasRemoved, render::CanvasRenderers, widget::CanvasWidget,
};
use cyancia_dock::dock::{Dock, DockId};
use cyancia_image::tile::GpuTileStorage;
use cyancia_input::{
    action::ActionCollection,
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_runtime::{Services, event::Event};
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

pub fn construct_canvas_dock_id(canvas: CanvasId) -> String {
    format!("canvas_dock_{}", canvas)
}

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
    CanvasFocus(Point),
    MouseEvent(mouse::Event),
}

impl<Theme> Dock<Theme, iced_wgpu::Renderer> for CanvasDock
where
    Theme: 'static,
{
    type Message = CanvasDockMessage;

    fn id(&self) -> cyancia_dock::dock::DockId {
        DockId::new(construct_canvas_dock_id(self.canvas).into())
    }

    fn view<'a>(&'a self) -> iced_core::Element<'a, Self::Message, Theme, iced_wgpu::Renderer> {
        let canvas_manager = self.runtime.service::<CanvasManager>();
        let renderers = self.runtime.service::<CanvasRenderers>();
        let canvas = canvas_manager.get(&self.canvas).unwrap();
        let renderer = renderers.get(&self.canvas).unwrap();

        CanvasWidget {
            is_focusing: canvas_manager.current_id() == Some(self.canvas),
            canvas,
            renderer,
            tile_storage: self.runtime.service::<GpuTileStorage>().clone(),
            on_focus: Box::new(CanvasDockMessage::CanvasFocus),
            on_mouse_event: Box::new(CanvasDockMessage::MouseEvent),
        }
        .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        let actions_matcher = self.actions_matcher.lock();

        match message {
            CanvasDockMessage::MouseEvent(event) => {
                let canvas_manager = self.runtime.service_mut::<CanvasManager>();
                if canvas_manager.current_id() != Some(self.canvas) {
                    return Task::none();
                }

                let Some(canvas) = canvas_manager.current() else {
                    return Task::none();
                };

                let mut tool_proxies = self.runtime.service_mut::<ToolProxies>();
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
                    _ => {}
                }
            }
            CanvasDockMessage::CanvasFocus(cursor_pos) => {
                self.cursor_position = cursor_pos;
                let mut canvas_manager = self.runtime.service_mut::<CanvasManager>();
                canvas_manager.set_current(self.canvas);
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
