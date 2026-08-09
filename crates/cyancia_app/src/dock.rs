use std::cell::RefCell;

use bevy_math::{IRect, Rect};
use cyancia_assets::AssetAppExt;
use cyancia_brush::{asset::BrushPreset, tool::BrushServicesExt, widget::BrushPresetListDelegate};
use cyancia_canvas::{
    CanvasAppExt, CanvasId, CanvasManager, CanvasUndoStackAppExt,
    command::{LayerPropertyChangeCommand, MoveLayersCommand},
    event::{CanvasRemoved, CanvasUpdated},
    widget::{
        canvas::CanvasWidget,
        layer_stack::{DropInfo, LayerStackMessage, LayerStackView},
    },
};
use cyancia_color::{
    BackgroundColorChanged, Color, ForegroundBackgroundColorExt, ForegroundColorChanged,
    model::rgb::Rgb,
};
use cyancia_color_selector::{
    ColorModel, ColorSelector, ColorSelectorMessage, ColorSelectorState, GradientPlaneShape,
    config::{
        ColorSelectorConfig, ColorSelectorConfigEditorState, ColorSelectorConfigMessage,
        GradientBarConfig, GradientPlaneConfig, GradientPlaneFlipAxis,
    },
};
use cyancia_dock::dock::{Dock, DockId};
use cyancia_image::{
    composite::{BlendFunctionRegistry, ImageCompositor, LayerPreviewOverriders},
    layer::{
        LayerId,
        properties::{LayerProperties, NamePropertyExt},
    },
    tile::{GpuTileStorage, TileStorageAppExt},
};
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_render::render_context::RenderContextAppExt;
use cyancia_runtime::{Services, event::Event};
use cyancia_tools::{ErasedToolFunctionMessage, ToolProxies, ToolProxyId};
use cyancia_utils::log_err::LogErr;
use iced::{
    Element, Length, Size, Subscription, Task, Theme,
    event::listen_with,
    keyboard::{self, Modifiers},
    mouse,
    widget::{Space, button, column, text},
    window,
};
use iced_core::Point;
use iced_runtime::task;
use iced_wgpu::Renderer;
use iced_widget::{container, scrollable, stack};
use moxcms::ColorProfile;

#[derive(Clone)]
pub enum ColorSelectorDockMessage {
    RawWindowId(u64),
    WindowMoved,
    ColorSelector(ColorSelectorMessage),
    ConfigEditor(ColorSelectorConfigMessage),
    OpenSettings,
    SettingsWindowClosed,
    ForegroundColorChanged(ForegroundColorChanged),
    BackgroundColorChanged(BackgroundColorChanged),
}

pub const COLOR_SELECTOR_DOCK_ID: &str = "color_selector";

pub struct ColorSelectorDock {
    selector: ColorSelectorState,
    config_editor: ColorSelectorConfigEditorState,
    window_id: RefCell<window::Id>,
    settings_window_id: Option<window::Id>,

    last_color: Color,
    is_foreground_color: bool,
}

impl ColorSelectorDock {
    pub fn new(services: &Services) -> Self {
        let configs = vec![ColorSelectorConfig {
            name: "RGB".to_string(),
            max_plane_size: 512,
            max_planes_per_row: 2,
            planes: vec![
                GradientPlaneConfig {
                    model: ColorModel::Rgb,
                    shape: GradientPlaneShape::Square,
                    variable_channels: 0b110,
                    flip_axis: GradientPlaneFlipAxis::empty(),
                    rotation: 0.0,
                    show_primary_channel_ring: false,
                    primary_channel_ring_width: 20.0,
                    ring_bar_saturated_hue_channel: false,
                    ring_rotation: 0.0,
                    reversed_ring: false,
                },
                GradientPlaneConfig {
                    model: ColorModel::OkLab,
                    shape: GradientPlaneShape::Square,
                    variable_channels: 0b110,
                    flip_axis: GradientPlaneFlipAxis::empty(),
                    rotation: 0.0,
                    show_primary_channel_ring: true,
                    primary_channel_ring_width: 20.0,
                    ring_bar_saturated_hue_channel: true,
                    ring_rotation: std::f32::consts::FRAC_PI_2,
                    reversed_ring: false,
                },
            ],
            bars: vec![
                GradientBarConfig {
                    model: ColorModel::Rgb,
                    channel: 0,
                    bar_height: 20.0,
                    show_channel_label: true,
                    show_precise_spin_box: true,
                    show_primary_channel_lock: true,
                },
                GradientBarConfig {
                    model: ColorModel::Rgb,
                    channel: 1,
                    bar_height: 20.0,
                    show_channel_label: true,
                    show_precise_spin_box: false,
                    show_primary_channel_lock: true,
                },
                GradientBarConfig {
                    model: ColorModel::Rgb,
                    channel: 2,
                    bar_height: 20.0,
                    show_channel_label: false,
                    show_precise_spin_box: true,
                    show_primary_channel_lock: true,
                },
                GradientBarConfig {
                    model: ColorModel::Hsv,
                    channel: 0,
                    bar_height: 20.0,
                    show_channel_label: true,
                    show_precise_spin_box: true,
                    show_primary_channel_lock: false,
                },
            ],
            out_of_gamut_color: Rgb::new(0.5, 0.5, 0.5),
            use_out_of_gamut_color: true,
            clip_to_gamut: true,
        }];

        Self {
            selector: ColorSelectorState::new(
                Color::Rgb(Rgb::new(0.0, 0.0, 0.0)),
                ColorProfile::new_srgb(),
                configs.clone(),
                0,
                services,
            ),
            config_editor: ColorSelectorConfigEditorState::new(configs, Some(0)),
            window_id: RefCell::new(window::Id::unique()),
            settings_window_id: None,
            last_color: **services.foreground_color(),
            is_foreground_color: true,
        }
    }
}

impl Dock<Theme, Renderer> for ColorSelectorDock {
    type Message = ColorSelectorDockMessage;

    fn id(&self) -> DockId {
        DockId::new(COLOR_SELECTOR_DOCK_ID.into())
    }

    fn view<'a>(
        &'a self,
        window_id: window::Id,
        _services: &'a Services,
    ) -> Element<'a, Self::Message, Theme, Renderer> {
        self.window_id.replace(window_id);
        let content = if self.settings_window_id == Some(window_id) {
            self.config_editor
                .view()
                .map(ColorSelectorDockMessage::ConfigEditor)
        } else {
            column![
                scrollable(ColorSelector::new(
                    &self.selector,
                    ColorSelectorDockMessage::ColorSelector
                ))
                .width(Length::Fill)
                .height(Length::Fill),
                button("Settings").on_press(ColorSelectorDockMessage::OpenSettings),
            ]
            .into()
        };

        container(content).padding(2).into()
    }

    fn update(&mut self, message: Self::Message, services: &mut Services) -> Task<Self::Message> {
        match message {
            ColorSelectorDockMessage::WindowMoved => {
                let window_id = *self.window_id.borrow();
                window::raw_id::<()>(window_id).map(ColorSelectorDockMessage::RawWindowId)
            }
            ColorSelectorDockMessage::RawWindowId(id) => self
                .selector
                .set_output_profile(id, services)
                .map(ColorSelectorDockMessage::ColorSelector),
            ColorSelectorDockMessage::ColorSelector(ColorSelectorMessage::Confirmed(color)) => {
                if self.is_foreground_color {
                    **services.foreground_color_mut() = color;
                    ForegroundColorChanged::broadcast(ForegroundColorChanged::new(
                        self.last_color,
                        color,
                    ));
                } else {
                    **services.background_color_mut() = color;
                    BackgroundColorChanged::broadcast(BackgroundColorChanged::new(
                        self.last_color,
                        color,
                    ));
                }

                Task::none()
            }
            ColorSelectorDockMessage::ColorSelector(m) => self
                .selector
                .update(m, services)
                .map(ColorSelectorDockMessage::ColorSelector),
            ColorSelectorDockMessage::OpenSettings => {
                if let Some(id) = self.settings_window_id {
                    window::gain_focus(id)
                } else {
                    let (id, task) = window::open(window::Settings {
                        size: Size {
                            width: 700.0,
                            height: 900.0,
                        },
                        ..Default::default()
                    });
                    self.settings_window_id = Some(id);
                    self.config_editor = ColorSelectorConfigEditorState::new(
                        self.selector.configs().to_vec(),
                        Some(0),
                    );
                    task.discard()
                }
            }
            ColorSelectorDockMessage::SettingsWindowClosed => {
                self.settings_window_id = None;
                Task::none()
            }
            ColorSelectorDockMessage::ConfigEditor(ColorSelectorConfigMessage::Cancelled) => {
                if let Some(id) = self.settings_window_id {
                    window::close(id)
                } else {
                    Task::none()
                }
            }
            ColorSelectorDockMessage::ConfigEditor(ColorSelectorConfigMessage::Confirmed) => self
                .selector
                .set_configs(self.config_editor.configs().to_vec(), services)
                .map(ColorSelectorDockMessage::ColorSelector),
            ColorSelectorDockMessage::ConfigEditor(m) => {
                self.config_editor.update(m);
                Task::none()
            }
            ColorSelectorDockMessage::ForegroundColorChanged(event) => {
                if !self.is_foreground_color {
                    return Task::none();
                }

                dbg!(event.new);
                self.last_color = event.new;
                self.selector
                    .set_color(event.new, services)
                    .map(ColorSelectorDockMessage::ColorSelector)
            }
            ColorSelectorDockMessage::BackgroundColorChanged(event) => {
                if self.is_foreground_color {
                    return Task::none();
                }

                dbg!(event.new);
                self.last_color = event.new;
                self.selector
                    .set_color(event.new, services)
                    .map(ColorSelectorDockMessage::ColorSelector)
            }
        }
    }

    fn on_open(&mut self) -> Task<Self::Message> {
        Task::done(ColorSelectorDockMessage::WindowMoved)
    }

    fn subscription(&self, _services: &Services) -> Subscription<Self::Message> {
        let cur_window = *self.window_id.borrow();

        let window_moved =
            window::events()
                .with(cur_window)
                .filter_map(|(cur_window, (window_id, event))| {
                    if matches!(event, window::Event::Moved(_)) && cur_window == window_id {
                        Some(ColorSelectorDockMessage::WindowMoved)
                    } else {
                        None
                    }
                });

        let settings_window_closed = window::events().with(self.settings_window_id).filter_map(
            |(settings_window_id, (window_id, event))| {
                if matches!(event, window::Event::Closed) && Some(window_id) == settings_window_id {
                    Some(ColorSelectorDockMessage::SettingsWindowClosed)
                } else {
                    None
                }
            },
        );

        let foreground_color_changed = ForegroundColorChanged::listen_to()
            .map(ColorSelectorDockMessage::ForegroundColorChanged);
        let background_color_changed = BackgroundColorChanged::listen_to()
            .map(ColorSelectorDockMessage::BackgroundColorChanged);

        Subscription::batch([
            window_moved,
            settings_window_closed,
            foreground_color_changed,
            background_color_changed,
        ])
    }

    fn sub_windows(&self) -> Vec<window::Id> {
        if let Some(id) = self.settings_window_id {
            vec![id]
        } else {
            Vec::new()
        }
    }
}

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
                _window_id: window::Id,
                _services: &'a Services,
            ) -> Element<'a, Self::Message, Theme, Renderer> {
                text($text).into()
            }

            fn update(&mut self, _message: (), _services: &mut Services) -> Task<()> {
                Task::none()
            }
        }

        pub const $id: &'static str = $text;
    };
}

test_dummy_dock!(FiltersDock, FILTERS_DOCK_ID, "Filters");

pub const LAYER_DOCK_ID: &str = "Layers";

pub struct LayersDock {
    renaming_layer: Option<LayerId>,
    rename_value: String,
    drop_preview: Option<DropInfo>,
}

impl LayersDock {
    pub fn new() -> Self {
        Self {
            renaming_layer: None,
            rename_value: String::new(),
            drop_preview: None,
        }
    }

    fn push_property_change(
        services: &mut Services,
        layer_id: LayerId,
        apply: impl FnOnce(&mut LayerProperties),
    ) {
        let Some(canvas_id) = services.current_canvas_id() else {
            return;
        };
        let cmd = services.update_canvas(&canvas_id, |canvas, _services| {
            let layer = canvas.image.layer_stack().get_layer(&layer_id)?;
            let old = layer.properties().clone();
            let new = {
                let mut props = old.clone();
                apply(&mut props);
                props
            };
            Some(LayerPropertyChangeCommand {
                canvas: canvas_id,
                layer_id,
                old,
                new,
            })
        });
        if let Some(cmd) = cmd.flatten() {
            services.push_undo_command(&canvas_id, cmd).log_err();
        }
    }
}

#[derive(Debug, Clone)]
pub enum LayersDockMessage {
    Layer(LayerStackMessage),
    EscapePressed,
}

impl Dock<Theme, Renderer> for LayersDock {
    type Message = LayersDockMessage;

    fn id(&self) -> DockId {
        DockId::new(LAYER_DOCK_ID.into())
    }

    fn view<'a>(
        &'a self,
        _window_id: window::Id,
        services: &'a Services,
    ) -> Element<'a, Self::Message, Theme, Renderer> {
        let Some(canvas) = services.current_canvas() else {
            return Space::new().into();
        };
        let blend_functions = services.service::<BlendFunctionRegistry>();
        let tile_storage = services.tile_storage();
        LayerStackView::new(
            canvas,
            blend_functions,
            tile_storage,
            self.renaming_layer,
            &self.rename_value,
            self.drop_preview.clone(),
            &|m| LayersDockMessage::Layer(m),
        )
        .into()
    }

    fn update(&mut self, message: Self::Message, services: &mut Services) -> Task<Self::Message> {
        match message {
            LayersDockMessage::EscapePressed => {
                if self.renaming_layer.is_some() {
                    self.renaming_layer = None;
                    self.rename_value.clear();
                }
            }
            LayersDockMessage::Layer(LayerStackMessage::LayerPropertyChanged(command)) => {
                let canvas_id = command.canvas;
                services.push_undo_command(&canvas_id, command).log_err();
            }
            LayersDockMessage::Layer(LayerStackMessage::DropPreview(drop_preview)) => {
                self.drop_preview = drop_preview;
            }
            LayersDockMessage::Layer(LayerStackMessage::SelectLayer(layer_id)) => {
                let Some(canvas_id) = services.current_canvas_id() else {
                    return Task::none();
                };
                let modifiers = services.service::<KeyboardState>().modifiers();
                services.update_canvas(&canvas_id, |canvas, _| {
                    if modifiers.contains(Modifiers::CTRL) {
                        canvas.toggle_layer_selection_and_active(layer_id);
                    } else if modifiers.contains(Modifiers::SHIFT) {
                        let active_layer = canvas.active_layer_id();
                        if layer_id == active_layer {
                            return;
                        }
                        let tree = canvas
                            .image
                            .layer_stack()
                            .iter_layers_dfs_display_order_without_root()
                            .map(|(n, _)| *n.id())
                            .collect::<Vec<_>>();
                        let mut on_select = false;
                        for layer in tree {
                            if on_select {
                                canvas.select_layer(layer);
                            }
                            if layer == layer_id || layer == active_layer {
                                on_select = !on_select;
                            }
                        }
                        canvas.set_active_layer(layer_id);
                    } else if !canvas.selected_layer_ids().contains(&layer_id) {
                        canvas.set_active_layer_and_clear_select(layer_id);
                    } else {
                        canvas.set_active_layer(layer_id);
                    }
                });
            }
            LayersDockMessage::Layer(LayerStackMessage::MoveLayers {
                layer_ids,
                new_parent,
                new_position,
            }) => {
                self.drop_preview = None;
                let Some(canvas_id) = services.current_canvas_id() else {
                    return Task::none();
                };
                let cmd = services.update_canvas(&canvas_id, |canvas, _services| {
                    let dragged = layer_ids.first()?;
                    let original_parent = canvas
                        .image
                        .layer_stack()
                        .get_layer(dragged)
                        .and_then(|n| n.parent().copied())?;
                    let original_index = canvas
                        .image
                        .layer_stack()
                        .get_layer(&original_parent)
                        .and_then(|p| p.child_index(dragged))
                        .unwrap_or(0);
                    let resolved_index = canvas
                        .image
                        .layer_stack()
                        .get_layer(&new_parent)
                        .and_then(|p| p.resolve_index(new_position));
                    if let Some(resolved_index) = resolved_index
                        && original_parent == new_parent
                        && original_index == resolved_index
                    {
                        return None;
                    }
                    Some(MoveLayersCommand::new(
                        canvas,
                        layer_ids.iter().copied(),
                        new_parent,
                        new_position,
                    ))
                });
                if let Some(cmd) = cmd.flatten() {
                    services.push_undo_command(&canvas_id, cmd).log_err();
                }
            }
            LayersDockMessage::Layer(LayerStackMessage::RenameLayer(layer_id)) => {
                let name = services.current_canvas().and_then(|canvas| {
                    canvas
                        .image
                        .layer_stack()
                        .get_layer(&layer_id)
                        .and_then(|layer| layer.properties().get_name())
                        .map(ToOwned::to_owned)
                });
                if let Some(name) = name {
                    self.renaming_layer = Some(layer_id);
                    self.rename_value = name;
                }
            }
            LayersDockMessage::Layer(LayerStackMessage::RenameChanged(value)) => {
                if self.renaming_layer.is_some() {
                    self.rename_value = value;
                }
            }
            LayersDockMessage::Layer(LayerStackMessage::RenameCommit(layer_id)) => {
                if self.renaming_layer != Some(layer_id) {
                    return Task::none();
                }
                let name = std::mem::take(&mut self.rename_value);
                self.renaming_layer = None;
                Self::push_property_change(services, layer_id, move |props| {
                    props.set_name(name);
                });
            }
        }
        Task::none()
    }

    fn subscription(&self, _services: &Services) -> Subscription<Self::Message> {
        listen_with(|event, _status, _window| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) => Some(LayersDockMessage::EscapePressed),
            _ => None,
        })
    }
}

pub const TOOL_OPTIONS_DOCK_ID: &str = "tool_options";
pub const BRUSH_PRESETS_DOCK_ID: &str = "brush_presets";

pub fn construct_canvas_dock_id(canvas: CanvasId) -> String {
    format!("canvas_dock_{}", canvas)
}

pub struct CanvasDock {
    canvas: CanvasId,
    tool_proxy: ToolProxyId,

    compositor: ImageCompositor,
    cursor_position: Point,

    window_id: RefCell<window::Id>,
    raw_window_id: Option<u64>,
    monitor_name: Option<String>,
}

impl CanvasDock {
    pub fn new(canvas: CanvasId, tool_proxy: ToolProxyId, window_id: window::Id) -> Self {
        Self {
            canvas,
            tool_proxy,
            compositor: ImageCompositor::default(),
            cursor_position: Point::default(),
            window_id: RefCell::new(window_id),
            raw_window_id: None,
            monitor_name: None,
        }
    }
}

pub enum CanvasDockMessage {
    WindowMoved,
    CanvasUpdated(Option<IRect>),
    CanvasFocus(Point),
    MouseEvent(mouse::Event),
    WidgetRectChange(Rect),
    ToolFunctionMessage(ErasedToolFunctionMessage),
    RawWindowIdUpdate(u64),
    MonitorNameUpdate(Option<String>),
}

impl Dock<Theme, Renderer> for CanvasDock {
    type Message = CanvasDockMessage;

    fn id(&self) -> DockId {
        DockId::new(construct_canvas_dock_id(self.canvas).into())
    }

    fn view<'a>(
        &'a self,
        window_id: window::Id,
        services: &'a Services,
    ) -> Element<'a, Self::Message, Theme, Renderer> {
        let canvas_manager = services.service::<CanvasManager>();
        self.window_id.replace(window_id);

        let (Some(canvas), Some(window_id), Some(monitor_name)) = (
            canvas_manager.get(&self.canvas),
            self.raw_window_id,
            self.monitor_name.clone(),
        ) else {
            return Space::new().into();
        };

        let canvas_overlay = services
            .service::<ToolProxies>()
            .get(&canvas.tool_proxy_id())
            .canvas_overlay(services)
            .map(CanvasDockMessage::ToolFunctionMessage);

        let canvas = CanvasWidget {
            is_focusing: canvas_manager.current_id() == Some(self.canvas),
            canvas,
            tile_storage: services.service::<GpuTileStorage>().clone(),
            on_focus: Box::new(CanvasDockMessage::CanvasFocus),
            on_mouse_event: Box::new(CanvasDockMessage::MouseEvent),
            on_widget_rect_change: Box::new(CanvasDockMessage::WidgetRectChange),
            // TODO wrap in arc?
            color_profile: canvas.image.profile().clone(),
            window_id,
            monitor_name,
        };

        stack!(canvas, canvas_overlay).into()
    }

    fn update(&mut self, message: Self::Message, services: &mut Services) -> Task<Self::Message> {
        match message {
            CanvasDockMessage::CanvasUpdated(dirty_tiles) => {
                services.service_scope::<LayerPreviewOverriders, _>(|overriders, services| {
                    let Some(canvas) = services.canvas(&self.canvas) else {
                        return;
                    };
                    let tiles = services.tile_storage();
                    let blend_functions = services.service::<BlendFunctionRegistry>();
                    let device = services.render_device();
                    let queue = services.render_queue();
                    self.compositor.create_cache(
                        overriders,
                        &canvas.image,
                        tiles,
                        blend_functions,
                        device,
                        queue,
                    );
                    self.compositor.composite(
                        overriders,
                        dirty_tiles.unwrap_or_else(|| canvas.image.image_tile_rect()),
                        &canvas.image,
                        tiles,
                        device,
                        queue,
                    );
                });

                Task::none()
            }
            CanvasDockMessage::MouseEvent(event) => {
                let canvas_manager = services.service_mut::<CanvasManager>();
                if canvas_manager.current_id() != Some(self.canvas) {
                    return Task::none();
                }

                let task = services.service_scope::<ToolProxies, _>(|tool_proxies, services| {
                    let tool_proxy = tool_proxies.get_mut(&self.tool_proxy);
                    let keyboard_state = services.service::<KeyboardState>().clone();

                    match event {
                        mouse::Event::ButtonPressed(button) => {
                            if button != mouse::Button::Left {
                                return Task::none();
                            }

                            tool_proxy.mouse_pressed(
                                &keyboard_state,
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

                            tool_proxy.mouse_released(
                                &keyboard_state,
                                &PressedMouseState {
                                    position: self.cursor_position,
                                },
                                services,
                            )
                        }
                        mouse::Event::CursorMoved { position } => {
                            self.cursor_position = position;
                            tool_proxy.mouse_moved(&keyboard_state, position, services)
                        }
                        _ => Task::none(),
                    }
                });

                task.map(CanvasDockMessage::ToolFunctionMessage)
            }
            CanvasDockMessage::CanvasFocus(cursor_pos) => {
                self.cursor_position = cursor_pos;
                services
                    .service_mut::<CanvasManager>()
                    .set_current(self.canvas);
                Task::none()
            }
            CanvasDockMessage::WidgetRectChange(rect) => {
                let canvas_manager = services.service_mut::<CanvasManager>();
                if let Some(canvas) = canvas_manager.get_mut(&self.canvas) {
                    canvas.transform.widget_bounds = rect;
                }
                Task::none()
            }
            CanvasDockMessage::ToolFunctionMessage(message) => {
                let Some(canvas) = services.service::<CanvasManager>().get(&self.canvas) else {
                    return Task::none();
                };

                let tool_proxy_id = canvas.tool_proxy_id();
                services
                    .service_scope::<ToolProxies, _>(|tool_proxies, services| {
                        tool_proxies
                            .get_mut(&tool_proxy_id)
                            .handle_message(message, services)
                    })
                    .map(CanvasDockMessage::ToolFunctionMessage)
            }
            CanvasDockMessage::WindowMoved => {
                let window_id = *self.window_id.borrow();

                let monitor_name = task::oneshot(move |channel| {
                    iced_runtime::Action::Window(window::Action::GetMonitorName(window_id, channel))
                })
                .map(CanvasDockMessage::MonitorNameUpdate);

                let window_raw_id =
                    window::raw_id::<()>(window_id).map(CanvasDockMessage::RawWindowIdUpdate);

                Task::batch([monitor_name, window_raw_id])
            }
            CanvasDockMessage::RawWindowIdUpdate(id) => {
                self.raw_window_id = Some(id);
                Task::none()
            }
            CanvasDockMessage::MonitorNameUpdate(name) => {
                self.monitor_name = name;
                Task::none()
            }
        }
    }

    fn on_open(&mut self) -> Task<Self::Message> {
        Task::batch([
            Task::done(CanvasDockMessage::WindowMoved),
            Task::done(CanvasDockMessage::CanvasUpdated(None)),
        ])
    }

    fn on_close(&mut self) -> Task<Self::Message> {
        CanvasRemoved::broadcast(CanvasRemoved { id: self.canvas });

        Task::none()
    }

    fn subscription(&self, services: &Services) -> Subscription<Self::Message> {
        let cur_window = *self.window_id.borrow();

        let canvas_update = CanvasUpdated::listen_to()
            .map(|e| CanvasDockMessage::CanvasUpdated(Some(e.dirty_tiles)));
        let window_moved =
            window::events()
                .with(cur_window)
                .filter_map(|(cur_window, (window_id, event))| {
                    if matches!(event, window::Event::Moved(_)) && cur_window == window_id {
                        Some(CanvasDockMessage::WindowMoved)
                    } else {
                        None
                    }
                });
        let tool = services
            .service::<ToolProxies>()
            .get(&self.tool_proxy)
            .subscription()
            .unwrap_or_else(Subscription::none)
            .map(CanvasDockMessage::ToolFunctionMessage);

        Subscription::batch([canvas_update, window_moved, tool])
    }
}

pub struct ToolOptionsDock;

pub enum ToolOptionsDockMessage {
    ToolFunction(ErasedToolFunctionMessage),
}

impl ToolOptionsDock {
    pub fn new(_: &Services) -> Self {
        Self
    }
}

impl Dock<Theme, iced_wgpu::Renderer> for ToolOptionsDock {
    type Message = ToolOptionsDockMessage;

    fn id(&self) -> DockId {
        DockId::new(TOOL_OPTIONS_DOCK_ID.into())
    }

    fn view<'a>(
        &'a self,
        _window_id: window::Id,
        services: &'a Services,
    ) -> Element<'a, Self::Message, Theme, iced_wgpu::Renderer> {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return Space::new().into();
        };

        let tool_proxy_id = canvas.tool_proxy_id();
        let tool_proxy = services.service::<ToolProxies>().get(&tool_proxy_id);
        let indicator = text(format!(
            "Tool: {} | override: {}",
            tool_proxy
                .current_tool()
                .map(ToString::to_string)
                .unwrap_or_else(|| "-".into()),
            tool_proxy
                .override_tool()
                .map(ToString::to_string)
                .unwrap_or_else(|| "-".into()),
        ));

        let Some(widget) = tool_proxy.tool_option_widget(services) else {
            return column![indicator].into();
        };

        column![indicator, widget.map(ToolOptionsDockMessage::ToolFunction)]
            .spacing(4)
            .into()
    }

    fn update(&mut self, message: Self::Message, services: &mut Services) -> Task<Self::Message> {
        match message {
            ToolOptionsDockMessage::ToolFunction(message) => {
                let Some(canvas) = services.service::<CanvasManager>().current() else {
                    return Task::none();
                };

                let tool_proxy_id = canvas.tool_proxy_id();
                services
                    .service_scope::<ToolProxies, _>(|tool_proxies, services| {
                        tool_proxies
                            .get_mut(&tool_proxy_id)
                            .handle_message(message, services)
                    })
                    .map(ToolOptionsDockMessage::ToolFunction)
            }
        }
    }
}

pub struct BrushPresetDock {
    brushes: BrushPresetListDelegate,
}

#[derive(Clone)]
pub enum BrushPresetDockMessage {
    SelectBrush(usize),
}

impl BrushPresetDock {
    pub fn new(services: &Services) -> Self {
        Self {
            brushes: BrushPresetListDelegate::new(
                services.assets().all_handles_of::<BrushPreset>().unwrap(),
            ),
        }
    }
}

impl Dock<Theme, Renderer> for BrushPresetDock {
    type Message = BrushPresetDockMessage;

    fn id(&self) -> DockId {
        DockId::new(BRUSH_PRESETS_DOCK_ID.into())
    }

    fn view<'a>(
        &'a self,
        _window_id: window::Id,
        _services: &'a Services,
    ) -> Element<'a, Self::Message, Theme, Renderer> {
        let buttons = self
            .brushes
            .items()
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let mut brush_button = button(text(item.name.clone()))
                    .width(Length::Fill)
                    .on_press(BrushPresetDockMessage::SelectBrush(index));
                if item.selected {
                    brush_button = brush_button.style(move |theme: &Theme, _| {
                        let palette = theme.extended_palette();
                        button::Style {
                            background: Some(palette.primary.strong.color.into()),
                            text_color: palette.primary.strong.text,
                            ..Default::default()
                        }
                    });
                }
                brush_button.into()
            })
            .collect::<Vec<Element<'a, _, Theme, Renderer>>>();

        column(buttons).spacing(2).into()
    }

    fn update(&mut self, message: Self::Message, services: &mut Services) -> Task<Self::Message> {
        match message {
            BrushPresetDockMessage::SelectBrush(index) => {
                self.brushes.select(index);
                let handle = self.brushes.get(index).map(|item| item.brush.clone());
                if let Some(handle) = handle {
                    services.set_current_brush_preset(handle);
                }
                Task::none()
            }
        }
    }
}
