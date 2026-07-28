use std::sync::Arc;

use cyancia_color::{
    Color,
    model::{
        gray::Gray, hsl::Hsl, hsv::Hsv, lab::Lab, lch::Lch, okhsl::OkHsl, okhsv::OkHsv,
        oklab::OkLab, oklch::OkLch, rgb::Rgb, xyz::Xyz,
    },
};
use cyancia_render::render_context::RenderContextAppExt;
use glam::{Vec2, Vec3};
use gpui::{
    AppContext, Bounds, Context, DisplayId, DragMoveEvent, Entity, EventEmitter,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, ObjectFit,
    ParentElement, Pixels, Render, Size, StatefulInteractiveElement, Styled, Subscription,
    SurfaceSource, Window, div, px, relative, surface,
};
use gpui_component::{
    ElementExt, Sizable, h_flex,
    input::{Input, InputEvent, InputState, MaskPattern},
    radio::{Radio, RadioGroup},
    v_flex,
};
use moxcms::ColorProfile;
use parse_display::Display;
use wgpu::{Texture, TextureView};

use crate::{
    config::{ColorSelectorConfig, GradientBarConfig, GradientPlaneConfig},
    control::{ActiveSelection, SurfaceDrag, SurfaceDragPreview, SurfaceTarget},
    pipeline::{ComputeBoundsPipeline, GradientMesh, GradientPipeline, GradientRingPipeline},
};

pub mod config;
mod control;
mod pipeline;
mod render;

#[derive(Debug, Clone, Copy)]
pub enum ColorSelectorEvent {
    Confirmed(Color),
}

const GRADIENT_RING_GAP: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[repr(u32)]
pub enum GradientPlaneShape {
    Square,
    Triangle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[repr(u32)]
pub enum ColorModel {
    #[display("Gray")]
    Gray,
    #[display("HSL")]
    Hsl,
    #[display("HSV")]
    Hsv,
    #[display("Lab")]
    Lab,
    #[display("LCh")]
    Lch,
    #[display("OkHSL")]
    OkHsl,
    #[display("OkHSV")]
    OkHsv,
    #[display("OkLab")]
    OkLab,
    #[display("OkLCh")]
    OkLch,
    #[display("RGB")]
    Rgb,
    #[display("XYZ")]
    Xyz,
}

impl ColorModel {
    pub const ALL: [Self; 11] = [
        Self::Gray,
        Self::Hsl,
        Self::Hsv,
        Self::Lab,
        Self::Lch,
        Self::OkHsl,
        Self::OkHsv,
        Self::OkLab,
        Self::OkLch,
        Self::Rgb,
        Self::Xyz,
    ];

    pub const PLANE_MODELS: [Self; 10] = [
        Self::Hsl,
        Self::Hsv,
        Self::Lab,
        Self::Lch,
        Self::OkHsl,
        Self::OkHsv,
        Self::OkLab,
        Self::OkLch,
        Self::Rgb,
        Self::Xyz,
    ];

    pub const fn channel_labels(self) -> &'static [&'static str] {
        match self {
            Self::Gray => &["V"],
            Self::Hsl => &["H", "S", "L"],
            Self::Hsv => &["H", "S", "V"],
            Self::Lab => &["L", "a", "b"],
            Self::Lch => &["L", "C", "h"],
            Self::OkHsl => &["H", "S", "L"],
            Self::OkHsv => &["H", "S", "V"],
            Self::OkLab => &["L", "a", "b"],
            Self::OkLch => &["L", "C", "h"],
            Self::Rgb => &["R", "G", "B"],
            Self::Xyz => &["X", "Y", "Z"],
        }
    }

    pub(crate) const fn hue_channel(self) -> Option<u8> {
        match self {
            Self::Hsl | Self::Hsv | Self::OkHsl | Self::OkHsv => Some(0),
            Self::Lch | Self::OkLch => Some(2),
            _ => None,
        }
    }

    pub fn channel_ranges(&self) -> [Vec2; 3] {
        match self {
            ColorModel::Gray => [Vec2::new(0.0, 1.0), Vec2::ZERO, Vec2::ZERO],
            ColorModel::Lab => [
                Vec2::new(0.0, 100.0),
                Vec2::new(-128.0, 127.0),
                Vec2::new(-128.0, 127.0),
            ],
            ColorModel::Lch => [
                Vec2::new(0.0, 100.0),
                Vec2::new(0.0, 150.0),
                Vec2::new(0.0, 360.0),
            ],
            ColorModel::Hsl | ColorModel::Hsv | ColorModel::OkHsl | ColorModel::OkHsv => [
                Vec2::new(0.0, 360.0),
                Vec2::new(0.0, 1.0),
                Vec2::new(0.0, 1.0),
            ],
            ColorModel::OkLab => [
                Vec2::new(0.0, 1.0),
                Vec2::new(-0.5, 0.5),
                Vec2::new(-0.5, 0.5),
            ],
            ColorModel::OkLch => [
                Vec2::new(0.0, 1.0),
                Vec2::new(0.0, 0.4),
                Vec2::new(0.0, 360.0),
            ],
            ColorModel::Rgb => [Vec2::new(0.0, 1.0); 3],
            ColorModel::Xyz => [
                Vec2::new(0.0, 1.5),
                Vec2::new(0.0, 1.0),
                Vec2::new(0.0, 1.5),
            ],
        }
    }

    pub const fn display_scale(&self) -> &'static [f32] {
        match self {
            Self::Gray => &[100.0],
            Self::Hsl | Self::Hsv | Self::OkHsl | Self::OkHsv => &[1.0, 100.0, 100.0],
            Self::Lab => &[1.0, 100.0 / 128.0, 100.0 / 128.0],
            Self::Lch => &[1.0, 1.0, 1.0],
            Self::OkLab => &[100.0, 200.0, 200.0],
            Self::OkLch => &[100.0, 100.0, 1.0],
            Self::Rgb => &[100.0, 100.0, 100.0],
            Self::Xyz => &[100.0 / 1.5, 100.0, 100.0 / 1.5],
        }
    }

    pub fn channels(self, color: Color, profile: &ColorProfile) -> Vec3 {
        let rgb_to_xyz = profile.rgb_to_xyz_matrix().to_f32();
        let xyz = match color {
            Color::Gray(gray) => gray.into_xyz(),
            Color::Hsl(hsl) => hsl.into_rgb().into_xyz(rgb_to_xyz),
            Color::Hsv(hsv) => hsv.into_rgb().into_xyz(rgb_to_xyz),
            Color::Lab(lab) => lab.into_xyz(),
            Color::Lch(lch) => lch.into_xyz(),
            Color::OkHsl(ok_hsl) => ok_hsl.into_xyz(),
            Color::OkHsv(ok_hsv) => ok_hsv.into_xyz(),
            Color::OkLab(ok_lab) => ok_lab.into_xyz(),
            Color::OkLch(ok_lch) => ok_lch.into_xyz(),
            Color::Rgb(rgb) => rgb.into_xyz(rgb_to_xyz),
            Color::Xyz(xyz) => xyz,
        };

        match self {
            Self::Gray => {
                let gray = Gray::from_xyz(xyz);
                Vec3::new(gray.v, 0.0, 0.0)
            }
            Self::Hsl => {
                let hsl = Hsl::from_rgb(Rgb::from_xyz(xyz, rgb_to_xyz.inverse()));
                Vec3::new(hsl.h, hsl.s, hsl.l)
            }
            Self::Hsv => {
                let hsv = Hsv::from_rgb(Rgb::from_xyz(xyz, rgb_to_xyz.inverse()));
                Vec3::new(hsv.h, hsv.s, hsv.v)
            }
            Self::Lab => {
                let lab = Lab::from_xyz(xyz);
                Vec3::new(lab.l, lab.a, lab.b)
            }
            Self::Lch => {
                let lch = Lch::from_xyz(xyz);
                Vec3::new(lch.l, lch.c, lch.h)
            }
            Self::OkHsl => {
                let okhsl = OkHsl::from_xyz(xyz);
                Vec3::new(okhsl.h, okhsl.s, okhsl.l)
            }
            Self::OkHsv => {
                let okhsv = OkHsv::from_xyz(xyz);
                Vec3::new(okhsv.h, okhsv.s, okhsv.v)
            }
            Self::OkLab => {
                let oklab = OkLab::from_xyz(xyz);
                Vec3::new(oklab.l, oklab.a, oklab.b)
            }
            Self::OkLch => {
                let oklch = OkLch::from_xyz(xyz);
                Vec3::new(oklch.l, oklch.c, oklch.h)
            }
            Self::Rgb => {
                let rgb = Rgb::from_xyz(xyz, rgb_to_xyz.inverse());
                Vec3::new(rgb.r, rgb.g, rgb.b)
            }
            Self::Xyz => Vec3::new(xyz.x, xyz.y, xyz.z),
        }
    }

    pub fn color_from_channels(self, channels: Vec3) -> Color {
        match self {
            Self::Gray => Color::Gray(Gray::new(channels.x)),
            Self::Hsl => Color::Hsl(Hsl::new(channels.x, channels.y, channels.z)),
            Self::Hsv => Color::Hsv(Hsv::new(channels.x, channels.y, channels.z)),
            Self::Lab => Color::Lab(Lab::new(channels.x, channels.y, channels.z)),
            Self::Lch => Color::Lch(Lch::new(channels.x, channels.y, channels.z)),
            Self::OkHsl => Color::OkHsl(OkHsl::new(channels.x, channels.y, channels.z)),
            Self::OkHsv => Color::OkHsv(OkHsv::new(channels.x, channels.y, channels.z)),
            Self::OkLab => Color::OkLab(OkLab::new(channels.x, channels.y, channels.z)),
            Self::OkLch => Color::OkLch(OkLch::new(channels.x, channels.y, channels.z)),
            Self::Rgb => Color::Rgb(Rgb::new(channels.x, channels.y, channels.z)),
            Self::Xyz => Color::Xyz(Xyz::new(channels.x, channels.y, channels.z)),
        }
    }
}

struct PlaneState {
    mesh: GradientMesh,
    texture: Arc<Texture>,
    texture_view: TextureView,
    ranges: (Vec2, Vec2),
    bounds: Bounds<Pixels>,
    primary_channel_override: Option<u8>,
}

struct BarState {
    mesh: GradientMesh,
    texture: Arc<Texture>,
    texture_view: TextureView,
    bounds: Bounds<Pixels>,
    input: Entity<InputState>,
}

pub struct ColorSelectorState {
    color: Color,
    profile: ColorProfile,
    output_profile: ColorProfile,

    presets: Vec<ColorSelectorConfig>,
    selected_preset: usize,

    compute_bounds_pipeline: ComputeBoundsPipeline,
    gradient_pipeline: GradientPipeline,
    ring_pipeline: GradientRingPipeline,
    active_selection: Option<ActiveSelection>,

    planes: Vec<PlaneState>,
    bars: Vec<BarState>,

    widget_bounds: Bounds<Pixels>,
    last_display: DisplayId,

    _subscriptions: Vec<Subscription>,
}

impl ColorSelectorState {
    pub fn new(
        color: Color,
        profile: ColorProfile,
        presets: Vec<ColorSelectorConfig>,
        selected_preset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let device = cx.render_device();
        let selected_preset = if presets.is_empty() {
            0
        } else {
            selected_preset.min(presets.len() - 1)
        };

        let output_profile = cyancia_color::platform::get_window_color_profile(window).unwrap();

        let _subscriptions = vec![cx.observe_window_bounds(window, Self::on_window_bounds_changed)];

        let mut this = Self {
            color,

            presets,
            selected_preset,

            compute_bounds_pipeline: ComputeBoundsPipeline::new(device, &profile),
            gradient_pipeline: GradientPipeline::new(device, &profile, &output_profile),
            ring_pipeline: GradientRingPipeline::new(device, &profile, &output_profile),
            active_selection: None,

            planes: Vec::new(),
            bars: Vec::new(),

            widget_bounds: Bounds::default(),
            last_display: window.display(cx).map(|d| d.id()).unwrap(),

            profile,
            output_profile,

            _subscriptions,
        };
        this.rebuild_bar_states(window, cx);
        this
    }

    fn on_window_bounds_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(display) = window.display(cx).map(|d| d.id()) else {
            return;
        };

        if display == self.last_display {
            return;
        }

        let device = cx.render_device();
        self.output_profile = cyancia_color::platform::get_window_color_profile(window).unwrap();
        self.gradient_pipeline = GradientPipeline::new(device, &self.profile, &self.output_profile);
        self.ring_pipeline = GradientRingPipeline::new(device, &self.profile, &self.output_profile);
    }

    pub fn color(&self) -> Color {
        self.color
    }

    pub fn set_color(&mut self, color: Color, window: &mut Window, cx: &mut Context<Self>) {
        self.color = color;
        self.sync_bar_inputs(window, cx);
        self.redraw_config(cx);
    }

    pub fn configs(&self) -> &[ColorSelectorConfig] {
        &self.presets
    }

    pub fn selected_config(&self) -> Option<usize> {
        (!self.presets.is_empty()).then_some(self.selected_preset)
    }

    pub fn set_configs(
        &mut self,
        configs: Vec<ColorSelectorConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.presets = configs;
        if self.presets.is_empty() {
            self.selected_preset = 0;
            self.planes.clear();
            self.bars.clear();
            self.active_selection = None;
            cx.notify();
            return;
        }

        self.selected_preset = self.selected_preset.min(self.presets.len() - 1);
        self.active_selection = None;
        self.planes.clear();
        self.rebuild_bar_states(window, cx);
        self.update_targets(cx);
        self.redraw_config(cx);
    }

    fn rebuild_bar_states(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bars.clear();
        let Some(preset) = self.presets.get(self.selected_preset) else {
            return;
        };
        let bars = preset.bars.clone();

        for config in bars {
            let value = self.bar_display_value(config.model, config.channel);
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .mask_pattern(MaskPattern::Number {
                        separator: None,
                        fraction: Some(2),
                    })
                    .default_value(format!("{value:.2}"))
            });

            let model = config.model;
            let channel = config.channel;
            cx.subscribe_in(&input, window, move |this, input, event, window, cx| {
                if !matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    return;
                }
                let Ok(value) = input.read(cx).value().parse::<f32>() else {
                    this.sync_bar_inputs(window, cx);
                    return;
                };
                this.set_bar_display_value(model, channel, value, window, cx);
            })
            .detach();

            let width = self.widget_bounds.size.width.as_f32().round().max(1.0) as u32;
            let height = config.bar_height.clamp(10.0, 40.0).round() as u32;
            let device = cx.render_device();
            let (texture, texture_view) =
                Self::create_gradient_texture("bar_gradient", width, height, device);
            self.bars.push(BarState {
                mesh: GradientMesh::new_bar(device),
                texture,
                texture_view,
                bounds: Bounds::default(),
                input,
            });
        }
    }

    fn bar_display_value(&self, model: ColorModel, channel: u8) -> f32 {
        model.channels(self.color, &self.profile)[channel as usize]
            * model.display_scale()[channel as usize]
    }

    fn set_bar_display_value(
        &mut self,
        model: ColorModel,
        channel: u8,
        value: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let channel = channel as usize;
        let scale = model.display_scale()[channel];
        let range = model.channel_ranges()[channel];
        let mut channels = model.channels(self.color, &self.profile);
        channels[channel] = (value / scale).clamp(range.x, range.y);
        self.color = model.color_from_channels(channels);
        self.sync_bar_inputs(window, cx);
        self.redraw_config(cx);
    }

    fn sync_bar_inputs(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preset) = self.presets.get(self.selected_preset) else {
            return;
        };
        for (config, bar) in preset.bars.iter().zip(&self.bars) {
            let value = self.bar_display_value(config.model, config.channel);
            bar.input.update(cx, |input, cx| {
                input.set_value(format!("{value:.2}"), window, cx);
            });
        }
    }

    fn plane_primary_channel(&self, index: usize, config: &GradientPlaneConfig) -> u8 {
        self.planes
            .get(index)
            .and_then(|plane| plane.primary_channel_override)
            .unwrap_or_else(|| {
                (0..3)
                    .find(|channel| config.variable_channels & (1 << channel) == 0)
                    .unwrap_or(0)
            })
    }

    fn bar_uses_saturated_primary_channel(&self, config: &GradientBarConfig) -> bool {
        config.model.hue_channel() == Some(config.channel)
            && self.presets[self.selected_preset]
                .planes
                .iter()
                .any(|plane| plane.model == config.model && plane.ring_bar_saturated_hue_channel)
    }

    fn bar_primary_channel_locked(&self, model: ColorModel, channel: u8) -> bool {
        self.presets[self.selected_preset]
            .planes
            .iter()
            .zip(&self.planes)
            .any(|(config, plane)| {
                config.model == model && plane.primary_channel_override == Some(channel)
            })
    }

    fn toggle_primary_channel_override(
        &mut self,
        model: ColorModel,
        channel: u8,
        cx: &mut Context<Self>,
    ) {
        let enabled = self.bar_primary_channel_locked(model, channel);
        let mut changed = false;
        for (config, plane) in self.presets[self.selected_preset]
            .planes
            .iter()
            .zip(&mut self.planes)
        {
            if config.model == model {
                plane.primary_channel_override = (!enabled).then_some(channel);
                changed = true;
            }
        }
        if changed {
            self.redraw_config(cx);
        }
    }

    fn plane_variable_channels(&self, index: usize, config: &GradientPlaneConfig) -> u8 {
        self.planes
            .get(index)
            .and_then(|plane| plane.primary_channel_override)
            .map_or(config.variable_channels, |channel| 0b111 & !(1 << channel))
    }

    fn plane_normalized_ranges(&self, index: usize) -> (Vec2, Vec2) {
        if !self.presets[self.selected_preset].clip_to_gamut {
            return (Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0));
        }

        self.planes
            .get(index)
            .map(|plane| plane.ranges)
            .unwrap_or((Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0)))
    }

    fn switch_preset(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.presets.len() || index == self.selected_preset {
            return;
        }

        self.selected_preset = index;
        self.active_selection = None;
        self.planes.clear();
        self.rebuild_bar_states(window, cx);
        self.update_targets(cx);
        self.redraw_config(cx);
    }
}

impl EventEmitter<ColorSelectorEvent> for ColorSelectorState {}

impl Render for ColorSelectorState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.presets.is_empty() {
            return div().into_any_element();
        }

        let indicator_color = self.indicator_color();
        let selector_id = cx.entity_id();
        let max_planes_per_row = self.presets[self.selected_preset]
            .max_planes_per_row
            .clamp(1, 5);
        let plane_columns = self.presets[self.selected_preset]
            .planes
            .len()
            .min(max_planes_per_row)
            .max(1);
        let plane_cell_width = relative(1.0 / plane_columns as f32);
        let max_plane_cell_size = self.presets[self.selected_preset]
            .max_plane_size
            .clamp(128, 512) as f32
            + 5.0;

        let planes = self
            .planes
            .chunks(max_planes_per_row)
            .enumerate()
            .map(|(row_index, row)| {
                h_flex()
                    .justify_evenly()
                    .children(row.iter().enumerate().map(|(column_index, plane)| {
                        let index = row_index * max_planes_per_row + column_index;
                        let target = SurfaceTarget::Plane(index);
                        let drag = SurfaceDrag {
                            selector: selector_id,
                            target,
                        };
                        let plane_indicator = self.plane_indicator_position(index);
                        let ring_indicator = self.ring_indicator_position(index);
                        div()
                            .w(plane_cell_width)
                            .max_w(px(max_plane_cell_size))
                            .px(px(2.5))
                            .child(
                                div()
                                    .id(("color-plane-surface", index))
                                    .relative()
                                    .w_full()
                                    .aspect_square()
                                    .on_prepaint({
                                        let state = cx.entity().downgrade();
                                        move |bounds, _, cx| {
                                            state
                                                .update(cx, |state, cx| {
                                                    state.update_plane_bounds(index, bounds, cx);
                                                })
                                                .ok();
                                        }
                                    })
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, event: &MouseDownEvent, window, cx| {
                                                this.start_plane_selection(
                                                    index,
                                                    event.position,
                                                    window,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            },
                                        ),
                                    )
                                    .on_drag(drag, |_, _, _, cx| {
                                        cx.stop_propagation();
                                        cx.new(|_| SurfaceDragPreview)
                                    })
                                    .on_drag_move(cx.listener(
                                        move |this,
                                              event: &DragMoveEvent<SurfaceDrag>,
                                              window,
                                              cx| {
                                            let drag = event.drag(cx);
                                            if drag.selector != cx.entity_id()
                                                || drag.target != target
                                            {
                                                return;
                                            }
                                            this.update_active_selection(
                                                event.event.position,
                                                window,
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, event: &MouseUpEvent, window, cx| {
                                                this.finish_active_selection(
                                                    target,
                                                    event.position,
                                                    window,
                                                    cx,
                                                );
                                            },
                                        ),
                                    )
                                    .on_mouse_up_out(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |this, event: &MouseUpEvent, window, cx| {
                                                this.finish_active_selection(
                                                    target,
                                                    event.position,
                                                    window,
                                                    cx,
                                                );
                                            },
                                        ),
                                    )
                                    .child(
                                        surface(SurfaceSource::Texture {
                                            texture: plane.texture.clone(),
                                            size: Size::new(
                                                plane.texture.width().into(),
                                                plane.texture.height().into(),
                                            ),
                                        })
                                        .size_full(),
                                    )
                                    .children(plane_indicator.map(|position| {
                                        div()
                                            .absolute()
                                            .left(relative(position.x))
                                            .top(relative(position.y))
                                            .ml(-px(3.0))
                                            .mt(-px(3.0))
                                            .size(px(6.0))
                                            .rounded_full()
                                            .border_1()
                                            .border_color(indicator_color)
                                    }))
                                    .children(ring_indicator.map(|position| {
                                        div()
                                            .absolute()
                                            .left(relative(position.x))
                                            .top(relative(position.y))
                                            .ml(-px(3.0))
                                            .mt(-px(3.0))
                                            .size(px(6.0))
                                            .rounded_full()
                                            .border_1()
                                            .border_color(indicator_color)
                                    })),
                            )
                    }))
            });

        let bars = self.presets[self.selected_preset]
            .bars
            .iter()
            .zip(&self.bars)
            .enumerate()
            .map(|(index, (config, bar))| {
                let channel = config.channel as usize;
                let label = config.model.channel_labels()[channel];
                let locked = self.bar_primary_channel_locked(config.model, config.channel);
                let lock = config.show_primary_channel_lock.then(|| {
                    Radio::new(format!("bar-primary-lock-{index}"))
                        .small()
                        .checked(locked)
                        .on_click(cx.listener({
                            let model = config.model;
                            let channel = config.channel;
                            move |this, _, _, cx| {
                                this.toggle_primary_channel_override(model, channel, cx)
                            }
                        }))
                });

                h_flex()
                    .gap_1()
                    .children(
                        config
                            .show_channel_label
                            .then(|| div().w(px(12.0)).flex_shrink_0().child(label)),
                    )
                    .child({
                        let target = SurfaceTarget::Bar(index);
                        let drag = SurfaceDrag {
                            selector: selector_id,
                            target,
                        };
                        let indicator_position = self.bar_indicator_position(index).unwrap_or(0.0);
                        div()
                            .id(("color-bar-surface", index))
                            .relative()
                            .flex_1()
                            .h(px(config.bar_height.clamp(10.0, 40.0)))
                            .on_prepaint({
                                let state = cx.entity().downgrade();
                                move |bounds, _, cx| {
                                    state
                                        .update(cx, |state, cx| {
                                            state.update_bar_bounds(index, bounds, cx);
                                        })
                                        .ok();
                                }
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                    this.start_bar_selection(index, event.position, window, cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .on_drag(drag, |_, _, _, cx| {
                                cx.stop_propagation();
                                cx.new(|_| SurfaceDragPreview)
                            })
                            .on_drag_move(cx.listener(
                                move |this, event: &DragMoveEvent<SurfaceDrag>, window, cx| {
                                    let drag = event.drag(cx);
                                    if drag.selector != cx.entity_id() || drag.target != target {
                                        return;
                                    }
                                    this.update_active_selection(event.event.position, window, cx);
                                    cx.stop_propagation();
                                },
                            ))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                                    this.finish_active_selection(
                                        target,
                                        event.position,
                                        window,
                                        cx,
                                    );
                                }),
                            )
                            .on_mouse_up_out(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                                    this.finish_active_selection(
                                        target,
                                        event.position,
                                        window,
                                        cx,
                                    );
                                }),
                            )
                            .child(
                                surface(SurfaceSource::Texture {
                                    texture: bar.texture.clone(),
                                    size: Size::new(
                                        bar.texture.width().into(),
                                        bar.texture.height().into(),
                                    ),
                                })
                                .object_fit(ObjectFit::Fill)
                                .size_full(),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .left(relative(indicator_position))
                                    .ml(-px(2.0))
                                    .w(px(4.0))
                                    .border_1()
                                    .border_color(indicator_color),
                            )
                    })
                    .children(config.show_precise_spin_box.then(|| {
                        div()
                            .w(px(60.0))
                            .child(Input::new(&bar.input).small().text_center().w_full())
                    }))
                    .children(lock)
            })
            .collect::<Vec<_>>();

        v_flex()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .on_prepaint({
                let state = cx.entity().downgrade();
                move |bounds, _, cx| {
                    state
                        .update(cx, |state, cx| {
                            state.update_widget_bounds(bounds, cx);
                        })
                        .ok();
                }
            })
            .child(v_flex().flex_shrink_0().gap(px(5.0)).children(planes))
            .child(v_flex().flex_shrink_0().gap_2().p_2().children(bars))
            .child(
                div().flex_shrink_0().child(
                    RadioGroup::horizontal("preset-radios")
                        .children(self.presets.iter().map(|p| p.name.clone()))
                        .selected_index(Some(self.selected_preset))
                        .on_click(cx.listener(move |state, index, window, cx| {
                            state.switch_preset(*index, window, cx);
                        })),
                ),
            )
            .into_any_element()
    }
}
