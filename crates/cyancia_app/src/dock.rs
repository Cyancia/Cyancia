use std::{sync::Arc, time::Duration};

use bevy_math::{IRect, Rect};
use cyancia_actions::{ActionFunctionRegistry, actions_matcher::ActionsMatcher};
use cyancia_canvas::{
    CCanvas, CanvasId, CanvasManager,
    event::{CanvasRemoved, CanvasUpdate},
    widget::CanvasWidget,
};
use cyancia_dock::dock::{Dock, DockId};
use cyancia_image::{
    composite::{ImageCompositor, LayerPreviewOverriders},
    tile::GpuTileStorage,
    widget::LayerNodeWidget,
};
use cyancia_input::{
    action::ActionCollection,
    key::KeyboardState,
    mouse::{HoverMouseState, PressedMouseState},
};
use cyancia_runtime::{Services, event::Event, service::RenderContext};
use cyancia_tools::{ErasedToolFunctionMessage, ToolProxies};
use cyancia_widgets::drag_drop_column::DragDropColumn;
use iced::{
    Length, Subscription,
    overlay::menu::Menu,
    widget::{column, text},
};
use iced::{Theme, widget::space};
use iced_core::{Element, Point, keyboard, mouse};
use iced_futures::subscription::Recipe;
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

    compositor: ImageCompositor,
    is_pressed: bool,
    cursor_position: Point,
    actions_matcher: Arc<Mutex<ActionsMatcher>>,
}

impl CanvasDock {
    pub fn new(canvas: CanvasId, actions_matcher: Arc<Mutex<ActionsMatcher>>) -> Self {
        Self {
            canvas,
            compositor: ImageCompositor::default(),
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
    ToolFunctionMessage(ErasedToolFunctionMessage),
    Tick,
    CanvasUpdate(IRect),
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
                let tool_proxy_id = canvas.tool_proxy_id();

                let task = services.service_scope::<ToolProxies, _>(|tool_proxies, services| {
                    let tool_proxy = tool_proxies.get_mut(&tool_proxy_id);

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
                                services,
                            )
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
                                services,
                            )
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
                                )
                            } else {
                                tool_proxy.mouse_moved_hovering(
                                    &actions_matcher.keyboard_state(),
                                    &HoverMouseState {
                                        position: self.cursor_position,
                                    },
                                    services,
                                )
                            }
                        }
                        _ => Task::none(),
                    }
                });

                return task.map(CanvasDockMessage::ToolFunctionMessage);
            }
            CanvasDockMessage::CanvasFocus(cursor_pos) => {
                self.cursor_position = cursor_pos;
                services
                    .service_mut::<CanvasManager>()
                    .set_current(self.canvas);
            }
            CanvasDockMessage::WidgetRectChange(rect) => {
                let canvas_manager = services.service_mut::<CanvasManager>();
                if let Some(canvas) = canvas_manager.get_mut(&self.canvas) {
                    canvas.transform.widget_bounds = rect;
                }
            }
            CanvasDockMessage::ToolFunctionMessage(message) => {
                let Some(canvas) = services.service::<CanvasManager>().get(&self.canvas) else {
                    return Task::none();
                };

                let tool_proxy_id = canvas.tool_proxy_id();
                return services
                    .service_scope::<ToolProxies, _>(|tool_proxies, services| {
                        tool_proxies
                            .get_mut(&tool_proxy_id)
                            .handle_message(message, services)
                    })
                    .map(CanvasDockMessage::ToolFunctionMessage);
            }
            CanvasDockMessage::Tick => {
                services.service_scope::<CanvasManager, _>(|canvas_manager, services| {
                    services.service_scope::<LayerPreviewOverriders, _>(|overriders, services| {
                        let Some(canvas) = canvas_manager.get_mut(&self.canvas) else {
                            return;
                        };
                        let tiles = services.service::<GpuTileStorage>();
                        let render_context = services.service::<RenderContext>();
                        let dirty_tiles = canvas.clear_dirty();
                        if dirty_tiles.is_empty() {
                            return;
                        }
                        self.compositor.create_cache(
                            overriders,
                            &canvas.image,
                            tiles,
                            &render_context.device,
                            &render_context.queue,
                        );
                        self.compositor.composite(
                            overriders,
                            dirty_tiles,
                            &canvas.image,
                            tiles,
                            &render_context.device,
                            &render_context.queue,
                        );
                    });
                });
            }
            CanvasDockMessage::CanvasUpdate(rect) => {
                let Some(canvas) = services
                    .service_mut::<CanvasManager>()
                    .get_mut(&self.canvas)
                else {
                    return Task::none();
                };
                canvas.mark_dirty(rect);
            }
        }

        Task::none()
    }

    fn on_close(&mut self) -> Task<Self::Message> {
        CanvasRemoved::broadcast(CanvasRemoved { id: self.canvas });

        Task::none()
    }

    fn subscription(&self) -> iced::Subscription<Self::Message> {
        // TODO: Any better way to trigger image composition?
        let tick = iced::time::every(Duration::from_secs_f32(1.0 / 240.0))
            .map(|_| CanvasDockMessage::Tick);
        let canvas_update =
            CanvasUpdate::listen_to()
                .with(self.canvas)
                .filter_map(|(cur_id, event)| {
                    if cur_id == event.id {
                        Some(CanvasDockMessage::CanvasUpdate(event.dirty_tiles))
                    } else {
                        None
                    }
                });

        Subscription::batch([tick, canvas_update])
    }
}

pub struct CurrentCanvasLayersDock {}

impl CurrentCanvasLayersDock {
    pub fn new() -> Self {
        Self {}
    }
}

#[derive(Debug)]
pub enum CurrentCanvasLayersDockMessage {
    ActiveLayerChange(usize),
}

impl Dock<Theme, Renderer> for CurrentCanvasLayersDock {
    type Message = CurrentCanvasLayersDockMessage;

    fn id(&self) -> DockId {
        DockId::new("current_canvas_layers".into())
    }

    fn view<'a>(&'a self, services: &'a Services) -> Element<'a, Self::Message, Theme, Renderer> {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return space().into();
        };

        let active_layer = canvas.image.active_layer;
        DragDropColumn::with_children(
            canvas
                .image
                .layer_stack()
                .iter_layers_dfs_display_order_without_root()
                .map(|(layer, depth)| {
                    LayerNodeWidget::new(layer)
                        .depth(depth)
                        .is_active(active_layer == layer.id())
                })
                .map(Into::into),
        )
        .on_click(|index| Some(CurrentCanvasLayersDockMessage::ActiveLayerChange(index)))
        .into()
    }

    fn update(&mut self, message: Self::Message, services: &mut Services) -> Task<Self::Message> {
        match message {
            CurrentCanvasLayersDockMessage::ActiveLayerChange(layer_index) => {
                let Some(canvas) = services.service_mut::<CanvasManager>().current_mut() else {
                    return Task::none();
                };
                let Some((active_layer, _)) = canvas
                    .image
                    .layer_stack()
                    .iter_layers_dfs_display_order_without_root()
                    .nth(layer_index)
                else {
                    return Task::none();
                };

                canvas.image.active_layer = active_layer.id();

                Task::none()
            }
        }
    }
}
