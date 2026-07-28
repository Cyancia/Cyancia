use cyancia_assets::{AssetAppExt, index_db::AssetFilter};
use cyancia_brush::{
    asset::BrushPreset, tool::CurrentBrushPresetHandle, widget::BrushPresetListDelegate,
};
use cyancia_canvas::{
    CanvasAppExt, CanvasId,
    event::CurrentCanvasChanged,
    tools::PanTool,
    widget::{canvas::CanvasWidget, layer_stack::LayerStackWidget},
};
use cyancia_color::{Color, model::rgb::Rgb};
use cyancia_color_selector::{
    ColorModel, ColorSelectorState, GradientPlaneShape,
    config::{
        ColorSelectorConfig, ColorSelectorConfigEditorState, ColorSelectorConfigEvent,
        GradientBarConfig, GradientPlaneConfig, GradientPlaneFlipAxis,
    },
};
use cyancia_tools::{ToolFunction, ToolProxies, ToolProxyId};
use cyancia_utils::log_err::LogErr;
use gpui::{
    AnyWindowHandle, App, AppContext, BorrowAppContext, Bounds, ClickEvent, Context, Entity,
    EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement, Point, Render, SharedString,
    Size, Styled, Subscription, Window, WindowBounds, WindowOptions, div, px,
};
use gpui_component::{
    IconName, IndexPath, Root, Sizable,
    button::Button,
    dock::{Panel, PanelEvent},
    list::{List, ListEvent, ListState},
    scroll::ScrollableElement,
    v_flex,
};
use log::info;
use moxcms::ColorProfile;

macro_rules! test_dummy_dock {
    ($name:ident) => {
        pub struct $name {
            focus_handle: FocusHandle,
        }

        impl $name {
            pub fn new(cx: &mut Context<Self>) -> Self {
                Self {
                    focus_handle: cx.focus_handle(),
                }
            }
        }

        impl Panel for $name {
            fn panel_name(&self) -> &'static str {
                stringify!($name)
            }
        }

        impl EventEmitter<PanelEvent> for $name {}

        impl Focusable for $name {
            fn focus_handle(&self, _: &App) -> FocusHandle {
                self.focus_handle.clone()
            }
        }

        impl Render for $name {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                format!("Hello {}", stringify!($name))
            }
        }
    };
}

test_dummy_dock!(LayersDock);
test_dummy_dock!(FiltersDock);

pub struct CanvasDock {
    canvas: CanvasId,
    focus_handle: FocusHandle,
    canvas_state: Entity<CanvasWidget>,
}

impl CanvasDock {
    pub fn new(
        canvas_id: CanvasId,
        tool_proxy_id: ToolProxyId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            tool_proxies
                .get_mut(&tool_proxy_id)
                .switch_tool(PanTool::id(), cx);
        });

        let canvas_state =
            cx.new(|cx| CanvasWidget::new(canvas_id, tool_proxy_id, window, cx, true).unwrap());

        canvas_state.update(cx, |widget, cx| {
            widget.recomposite(cx);
        });

        Self {
            canvas: canvas_id,
            focus_handle: cx.focus_handle(),
            canvas_state,
        }
    }
}

impl EventEmitter<PanelEvent> for CanvasDock {}

impl Focusable for CanvasDock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for CanvasDock {
    fn panel_name(&self) -> &'static str {
        construct_canvas_dock_id(self.canvas).leak()
    }

    fn tab_name(&self, _: &App) -> Option<SharedString> {
        // TODO Record the filename of an image and display it
        Some(format!("Canvas {}", self.canvas).into())
    }
}

pub fn construct_canvas_dock_id(canvas: CanvasId) -> String {
    format!("canvas_dock_{}", canvas)
}

impl Render for CanvasDock {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.canvas_state.clone()
    }
}

pub struct CurrentCanvasLayersDock {
    widget: Option<Entity<LayerStackWidget>>,
    focus_handle: FocusHandle,
}

impl CurrentCanvasLayersDock {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        cx.subscribe_in(
            &cx.global_canvas_events_entity(),
            window,
            |dock, _, _: &CurrentCanvasChanged, window, cx| {
                if let Some(canvas) = cx.current_canvas().and_then(|e| e.upgrade()) {
                    dock.widget = Some(cx.new(|cx| LayerStackWidget::new(canvas, window, cx)));
                }
            },
        )
        .detach();

        Self {
            widget: None,
            focus_handle,
        }
    }
}

impl EventEmitter<PanelEvent> for CurrentCanvasLayersDock {}

impl Focusable for CurrentCanvasLayersDock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for CurrentCanvasLayersDock {
    fn panel_name(&self) -> &'static str {
        "current_canvas_layers"
    }

    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some("Current Canvas Layers".into())
    }
}

impl Render for CurrentCanvasLayersDock {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.widget
            .as_ref()
            .map(|w| w.clone().into_any_element())
            .unwrap_or_else(|| div().into_any_element())
    }
}

pub struct ToolOptionsDock {
    focus_handle: FocusHandle,
}

impl ToolOptionsDock {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for ToolOptionsDock {}

impl Focusable for ToolOptionsDock {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ToolOptionsDock {
    fn panel_name(&self) -> &'static str {
        "tool_options"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Tool Options"
    }
}

impl Render for ToolOptionsDock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(canvas) = cx.read_current_canvas() else {
            return div().into_any_element();
        };

        let tool_proxy_id = canvas.tool_proxy_id();
        cx.update_global::<ToolProxies, _>(|tool_proxies, cx| {
            let tool_proxy = tool_proxies.get_mut(&tool_proxy_id);
            tool_proxy
                .tool_option_widget(window, cx)
                .unwrap_or_else(|| div().into_any_element())
        })
    }
}

pub struct BrushPresetDock {
    // TODO Use this after tag hierarchy is implemented.
    _filter_condition: AssetFilter<BrushPreset>,
    list_state: Entity<ListState<BrushPresetListDelegate>>,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl BrushPresetDock {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let list_state = cx.new(|cx| {
            ListState::new(
                BrushPresetListDelegate::new(cx.assets().all_handles_of::<BrushPreset>().unwrap()),
                window,
                cx,
            )
        });

        let _subscriptions = vec![
            cx.subscribe_in(&list_state, window, Self::on_select_brush_preset),
            cx.observe_global_in::<CurrentBrushPresetHandle>(
                window,
                Self::on_active_brush_preset_changed,
            ),
        ];

        Self {
            _filter_condition: Default::default(),
            list_state,
            focus_handle: cx.focus_handle(),
            _subscriptions,
        }
    }

    fn on_select_brush_preset(
        &mut self,
        state: &Entity<ListState<BrushPresetListDelegate>>,
        event: &ListEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ListEvent::Confirm(ix) => {
                let handle = state.read_with(cx, |state, _| {
                    let item = state.delegate().get(*ix)?;
                    Some(item.handle.clone())
                });

                if let Some(handle) = handle {
                    if let Ok(metadata) = handle.metadata() {
                        info!(
                            "Selected new brush {} at {} revision {}",
                            handle.id(),
                            metadata.relative_path,
                            metadata.revision
                        );
                    }
                    cx.set_global(CurrentBrushPresetHandle::new(handle));
                }
            }
            ListEvent::Select(_) | ListEvent::Cancel => {}
        }
    }

    fn on_active_brush_preset_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let brush_preset = cx
            .try_global::<CurrentBrushPresetHandle>()
            .map(|h| h.0.clone());
        self.list_state.update(cx, |state, cx| {
            let ix = brush_preset
                .and_then(|preset| {
                    state
                        .delegate()
                        .items()
                        .iter()
                        .position(|i| i.handle.id() == preset.id())
                })
                .map(IndexPath::new);

            state.set_selected_index(ix, window, cx);
        });
    }
}

impl EventEmitter<PanelEvent> for BrushPresetDock {}

impl Focusable for BrushPresetDock {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for BrushPresetDock {
    fn panel_name(&self) -> &'static str {
        "brush_presets"
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Brush Presets"
    }
}

impl Render for BrushPresetDock {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p_1()
            .size_full()
            .child(List::new(&self.list_state).small().size_full())
    }
}

pub struct ColorSelectorDock {
    focus_handle: FocusHandle,
    color_selector: Entity<ColorSelectorState>,
    config_editor: Option<Entity<ColorSelectorConfigEditorState>>,
    editor_window: Option<AnyWindowHandle>,
    _subscriptions: Vec<Subscription>,
}

impl ColorSelectorDock {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let config = ColorSelectorConfig {
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
                    model: ColorModel::Hsv,
                    shape: GradientPlaneShape::Triangle,
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
            clip_to_gamut: false,
        };

        let color_selector = cx.new(|cx| {
            ColorSelectorState::new(
                Color::Rgb(Rgb::new(0.0, 0.0, 0.0)),
                ColorProfile::new_srgb(),
                vec![config.clone()],
                0,
                window,
                cx,
            )
        });

        let dock = cx.entity().downgrade();
        let subscriptions = vec![cx.on_window_closed(move |cx, window_id| {
            let dock = dock.clone();
            cx.defer(move |cx| {
                dock.update(cx, |dock, cx| {
                    if dock
                        .editor_window
                        .is_some_and(|window| window.window_id() == window_id)
                    {
                        dock.editor_window = None;
                        cx.notify();
                    }
                })
                .ok();
            });
        })];

        Self {
            focus_handle: cx.focus_handle(),
            color_selector,
            config_editor: None,
            editor_window: None,
            _subscriptions: subscriptions,
        }
    }

    fn on_config_editor_event(
        &mut self,
        editor: &Entity<ColorSelectorConfigEditorState>,
        event: &ColorSelectorConfigEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ColorSelectorConfigEvent::Confirm => {
                let configs = editor.read(cx).configs().to_vec();
                self.color_selector.update(cx, |selector, cx| {
                    selector.set_configs(configs, window, cx);
                    cx.notify();
                });
                cx.refresh_windows();
            }
            ColorSelectorConfigEvent::Cancel => {
                if let Some(editor_window) = self.editor_window.take() {
                    editor_window
                        .update(cx, |_, window, _| window.remove_window())
                        .ok();
                }
            }
        }
    }

    fn on_open_editor(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor_window.is_some() {
            return;
        }

        let (configs, selected_config) = self.color_selector.read_with(cx, |selector, _| {
            (selector.configs().to_vec(), selector.selected_config())
        });
        let config_editor =
            cx.new(|cx| ColorSelectorConfigEditorState::new(configs, selected_config, window, cx));
        cx.subscribe_in(&config_editor, window, Self::on_config_editor_event)
            .detach();

        let parent_center = window.bounds().center();
        let size = Size::new(px(500.0), px(1080.0));

        let editor_window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    parent_center - Point::new(size.width, size.height) * 0.5,
                    size,
                ))),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Root::new(config_editor.clone(), window, cx)),
        );

        self.config_editor = Some(config_editor);

        let Ok(editor_window) = editor_window.logged_err() else {
            return;
        };

        self.editor_window = Some(editor_window.into());
    }
}

impl EventEmitter<PanelEvent> for ColorSelectorDock {}

impl Focusable for ColorSelectorDock {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for ColorSelectorDock {
    fn panel_name(&self) -> &'static str {
        "color_selector"
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        "Color Selector"
    }
}

impl Render for ColorSelectorDock {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(
                v_flex()
                    .size_full()
                    .overflow_y_scrollbar()
                    .child(self.color_selector.clone())
                    .child(
                        div().flex_shrink_0().child(
                            Button::new("open-editor")
                                .icon(IconName::Settings)
                                .on_click(cx.listener(Self::on_open_editor)),
                        ),
                    ),
            )
    }
}
