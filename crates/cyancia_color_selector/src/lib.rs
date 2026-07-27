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
    AppContext, Bounds, Context, DisplayId, DragMoveEvent, Empty, Entity, EntityId, EventEmitter,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, ObjectFit,
    ParentElement, Pixels, Point, Render, Size, StatefulInteractiveElement, Styled, SurfaceSource,
    Window, div, px, relative, rgb, surface,
};
use gpui_component::{
    ElementExt, Sizable, h_flex,
    input::{InputEvent, InputState, MaskPattern, NumberInput, NumberInputEvent, StepAction},
    radio::{Radio, RadioGroup},
    v_flex,
};
use moxcms::ColorProfile;
use parse_display::Display;
use wgpu::{
    Device, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureView,
};

use crate::{
    config::{ColorSelectorConfig, GradientBarConfig, GradientPlaneConfig},
    render::{GradientMesh, GradientPipeline, GradientRingPipeline, GradientSettings},
};

pub mod config;
mod render;

#[derive(Debug, Clone, Copy)]
pub enum ColorSelectorEvent {
    Confirmed(Color),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceTarget {
    Plane(usize),
    Bar(usize),
}

#[derive(Clone)]
struct SurfaceDrag {
    selector: EntityId,
    target: SurfaceTarget,
}

struct SurfaceDragPreview;

impl Render for SurfaceDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveSelection {
    Plane(usize),
    Ring(usize),
    Bar(usize),
}

const GRADIENT_RING_GAP: f32 = 5.0;
const COLOR_MODEL_COUNT: usize = ColorModel::ALL.len();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
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

fn plane_scale(config: &GradientPlaneConfig, texture_size: f32) -> f32 {
    if !config.show_primary_channel_ring {
        return 1.0;
    }

    let antialias_width = 1.0 / texture_size;
    let inner_radius = (0.5
        - antialias_width
        - (config.primary_channel_ring_width + GRADIENT_RING_GAP) / texture_size)
        .max(0.0);
    let circumradius = match config.shape {
        GradientPlaneShape::Square => std::f32::consts::SQRT_2,
        GradientPlaneShape::Triangle => 1.0,
    };
    2.0 * inner_radius / circumradius
}

fn clamp_plane_uv(shape: GradientPlaneShape, uv: Vec2) -> Vec2 {
    match shape {
        GradientPlaneShape::Square => uv.clamp(Vec2::ZERO, Vec2::ONE),
        GradientPlaneShape::Triangle => {
            let y = uv.y.clamp(0.0, 1.0);
            let min_x = 0.5 * (1.0 - y);
            let max_x = 0.5 * (1.0 + y);
            Vec2::new(uv.x.clamp(min_x, max_x), y)
        }
    }
}

fn plane_position_to_uv(shape: GradientPlaneShape, position: Vec2) -> Vec2 {
    match shape {
        GradientPlaneShape::Square => (position + Vec2::ONE) * 0.5,
        GradientPlaneShape::Triangle => {
            Vec2::new(0.5 + position.x / 3.0_f32.sqrt(), (1.0 - position.y) / 1.5)
        }
    }
}

fn plane_uv_to_position(shape: GradientPlaneShape, uv: Vec2) -> Vec2 {
    match shape {
        GradientPlaneShape::Square => uv * 2.0 - Vec2::ONE,
        GradientPlaneShape::Triangle => Vec2::new(3.0_f32.sqrt() * (uv.x - 0.5), 1.0 - 1.5 * uv.y),
    }
}

fn rotate_clockwise(position: Vec2, rotation: f32) -> Vec2 {
    let (sin, cos) = rotation.sin_cos();
    Vec2::new(
        cos * position.x + sin * position.y,
        -sin * position.x + cos * position.y,
    )
}

fn rotate_counterclockwise(position: Vec2, rotation: f32) -> Vec2 {
    let (sin, cos) = rotation.sin_cos();
    Vec2::new(
        cos * position.x - sin * position.y,
        sin * position.x + cos * position.y,
    )
}

pub struct ColorSelectorState {
    color: Color,
    profile: ColorProfile,
    output_profile: ColorProfile,

    presets: Vec<ColorSelectorConfig>,
    selected_preset: usize,

    gradient_pipeline: GradientPipeline,
    ring_pipeline: GradientRingPipeline,
    plane_targets: Vec<(GradientMesh, Arc<Texture>, TextureView)>,
    bar_targets: Vec<(GradientMesh, Arc<Texture>, TextureView)>,
    bar_inputs: Vec<Entity<InputState>>,
    primary_channel_overrides: Vec<[Option<u8>; COLOR_MODEL_COUNT]>,
    plane_bounds: Vec<Bounds<Pixels>>,
    bar_bounds: Vec<Bounds<Pixels>>,
    active_selection: Option<ActiveSelection>,

    widget_bounds: Bounds<Pixels>,
    last_display: DisplayId,
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

        let preset_count = presets.len();
        let mut this = Self {
            color,

            presets,
            selected_preset,

            gradient_pipeline: GradientPipeline::new(device, &profile, &output_profile),
            ring_pipeline: GradientRingPipeline::new(device, &profile, &output_profile),
            plane_targets: Vec::new(),
            bar_targets: Vec::new(),
            bar_inputs: Vec::new(),
            primary_channel_overrides: vec![[None; COLOR_MODEL_COUNT]; preset_count],
            plane_bounds: Vec::new(),
            bar_bounds: Vec::new(),
            active_selection: None,

            widget_bounds: Bounds::default(),
            last_display: window.display(cx).map(|d| d.id()).unwrap(),

            profile,
            output_profile,
        };
        this.rebuild_bar_inputs(window, cx);
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
            self.plane_targets.clear();
            self.bar_targets.clear();
            self.bar_inputs.clear();
            self.primary_channel_overrides.clear();
            self.plane_bounds.clear();
            self.bar_bounds.clear();
            self.active_selection = None;
            cx.notify();
            return;
        }

        self.selected_preset = self.selected_preset.min(self.presets.len() - 1);
        self.active_selection = None;
        self.primary_channel_overrides = vec![[None; COLOR_MODEL_COUNT]; self.presets.len()];
        self.rebuild_bar_inputs(window, cx);
        self.update_targets(cx);
        self.redraw_config(cx);
    }

    fn rebuild_bar_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bar_inputs.clear();
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
            cx.subscribe_in(&input, window, move |this, _, event, window, cx| {
                let delta = match event {
                    NumberInputEvent::Step(StepAction::Increment) => 0.1,
                    NumberInputEvent::Step(StepAction::Decrement) => -0.1,
                };
                let value = this.bar_display_value(model, channel) + delta;
                this.set_bar_display_value(model, channel, value, window, cx);
            })
            .detach();

            self.bar_inputs.push(input);
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
        for (config, input) in preset.bars.iter().zip(&self.bar_inputs) {
            let value = self.bar_display_value(config.model, config.channel);
            input.update(cx, |input, cx| {
                input.set_value(format!("{value:.2}"), window, cx);
            });
        }
    }

    fn primary_channel_override(&self, model: ColorModel) -> Option<u8> {
        self.primary_channel_overrides
            .get(self.selected_preset)
            .and_then(|overrides| overrides[model as usize])
    }

    fn plane_primary_channel(&self, config: &GradientPlaneConfig) -> u8 {
        self.primary_channel_override(config.model)
            .unwrap_or_else(|| {
                (0..3)
                    .find(|channel| config.variable_channels & (1 << channel) == 0)
                    .unwrap_or(0)
            })
    }

    fn bar_uses_saturated_primary_channel(&self, config: &GradientBarConfig) -> bool {
        self.presets
            .get(self.selected_preset)
            .is_some_and(|preset| {
                preset.planes.iter().any(|plane| {
                    plane.model == config.model
                        && plane.saturated_primary_channel
                        && self.plane_primary_channel(plane) == config.channel
                })
            })
    }

    fn toggle_primary_channel_override(
        &mut self,
        model: ColorModel,
        channel: u8,
        cx: &mut Context<Self>,
    ) {
        let Some(overrides) = self.primary_channel_overrides.get_mut(self.selected_preset) else {
            return;
        };
        let value = &mut overrides[model as usize];
        *value = if *value == Some(channel) {
            None
        } else {
            Some(channel)
        };
        self.redraw_config(cx);
    }

    fn plane_variable_channels(&self, config: &GradientPlaneConfig) -> u8 {
        self.primary_channel_override(config.model)
            .map_or(config.variable_channels, |channel| 0b111 & !(1 << channel))
    }

    fn update_plane_bounds(
        &mut self,
        index: usize,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.plane_bounds.len() <= index {
            self.plane_bounds.resize(index + 1, Bounds::default());
        }
        self.plane_bounds[index] = bounds;

        let size = bounds
            .size
            .width
            .as_f32()
            .min(bounds.size.height.as_f32())
            .round()
            .max(1.0) as u32;
        if self
            .plane_targets
            .get(index)
            .is_some_and(|(_, texture, _)| texture.width() == size && texture.height() == size)
            || index >= self.plane_targets.len()
        {
            return;
        }
        let Some(config) = self
            .presets
            .get(self.selected_preset)
            .and_then(|preset| preset.planes.get(index))
        else {
            return;
        };

        let device = cx.render_device();
        let (texture, view) = Self::create_gradient_texture("plane_gradient", size, size, device);
        let mesh = GradientMesh::new_plane(device, config.shape, plane_scale(config, size as f32));
        self.plane_targets[index] = (mesh, texture, view);
        self.redraw_config(cx);
    }

    fn update_bar_bounds(&mut self, index: usize, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        if self.bar_bounds.len() <= index {
            self.bar_bounds.resize(index + 1, Bounds::default());
        }
        self.bar_bounds[index] = bounds;
        self.update_bar_target_width(index, bounds.size.width, cx);
    }

    fn plane_uv_from_window_position(
        &self,
        index: usize,
        position: Point<Pixels>,
    ) -> Option<(Vec2, bool)> {
        let config = self.presets.get(self.selected_preset)?.planes.get(index)?;
        let bounds = *self.plane_bounds.get(index)?;
        let width = bounds.size.width.as_f32();
        let height = bounds.size.height.as_f32();
        if width <= 0.0 || height <= 0.0 {
            return None;
        }

        let output_position = Vec2::new(
            2.0 * (position.x - bounds.origin.x).as_f32() / width - 1.0,
            1.0 - 2.0 * (position.y - bounds.origin.y).as_f32() / height,
        );
        let mut input_position = rotate_counterclockwise(output_position, config.rotation);
        if config.flip_axis.contains(config::GradientPlaneFlipAxis::X) {
            input_position.x = -input_position.x;
        }
        if config.flip_axis.contains(config::GradientPlaneFlipAxis::Y) {
            input_position.y = -input_position.y;
        }

        let scale = plane_scale(config, width);
        if scale <= f32::EPSILON {
            return None;
        }
        input_position /= scale;
        let uv = plane_position_to_uv(config.shape, input_position);
        let clamped = clamp_plane_uv(config.shape, uv);
        Some((clamped, (uv - clamped).length_squared() <= 1e-6))
    }

    fn start_plane_selection(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(config) = self
            .presets
            .get(self.selected_preset)
            .and_then(|preset| preset.planes.get(index))
            .cloned()
        else {
            return;
        };
        let Some(bounds) = self.plane_bounds.get(index).copied() else {
            return;
        };
        let size = bounds.size.width.as_f32();
        if size <= 0.0 {
            return;
        }

        let texture_uv = Vec2::new(
            (position.x - bounds.origin.x).as_f32() / size,
            1.0 - (position.y - bounds.origin.y).as_f32() / size,
        );
        let radius = (texture_uv - Vec2::splat(0.5)).length();
        let antialias_width = 1.0 / size;
        let outer_radius = 0.5 - antialias_width;
        let inner_radius = (outer_radius - config.primary_channel_ring_width / size).max(0.0);

        self.active_selection = if config.show_primary_channel_ring
            && radius >= inner_radius - antialias_width
            && radius <= outer_radius + antialias_width
        {
            Some(ActiveSelection::Ring(index))
        } else if self
            .plane_uv_from_window_position(index, position)
            .is_some_and(|(_, inside)| inside)
        {
            Some(ActiveSelection::Plane(index))
        } else {
            None
        };

        self.update_active_selection(position, window, cx);
    }

    fn start_bar_selection(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.bar_bounds.len() {
            return;
        }
        self.active_selection = Some(ActiveSelection::Bar(index));
        self.update_active_selection(position, window, cx);
    }

    fn update_active_selection(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selection) = self.active_selection else {
            return;
        };

        match selection {
            ActiveSelection::Plane(index) => {
                let Some(config) = self
                    .presets
                    .get(self.selected_preset)
                    .and_then(|preset| preset.planes.get(index))
                    .cloned()
                else {
                    return;
                };
                let Some((uv, _)) = self.plane_uv_from_window_position(index, position) else {
                    return;
                };
                let mut channels = config.model.channels(self.color, &self.profile);
                let ranges = config.model.channel_ranges();
                let mut variable_index = 0;
                let variable_channels = self.plane_variable_channels(&config);
                for channel in 0..3 {
                    if variable_channels & (1 << channel) != 0 {
                        channels[channel] = ranges[channel].x
                            + (ranges[channel].y - ranges[channel].x) * uv[variable_index];
                        variable_index += 1;
                    }
                }
                self.color = config.model.color_from_channels(channels);
            }
            ActiveSelection::Ring(index) => {
                let Some(config) = self
                    .presets
                    .get(self.selected_preset)
                    .and_then(|preset| preset.planes.get(index))
                    .cloned()
                else {
                    return;
                };
                let Some(bounds) = self.plane_bounds.get(index).copied() else {
                    return;
                };
                let size = bounds.size.width.as_f32();
                let centered = Vec2::new(
                    (position.x - bounds.origin.x).as_f32() / size - 0.5,
                    0.5 - (position.y - bounds.origin.y).as_f32() / size,
                );
                if centered.length_squared() <= f32::EPSILON {
                    return;
                }
                let mut angle = centered.y.atan2(centered.x) + config.ring_rotation;
                if config.reversed_ring {
                    angle = -angle;
                }
                let factor = (angle / std::f32::consts::TAU).rem_euclid(1.0);
                let channel = self.plane_primary_channel(&config) as usize;
                let mut channels = config.model.channels(self.color, &self.profile);
                let range = config.model.channel_ranges()[channel];
                channels[channel] = range.x + (range.y - range.x) * factor;
                self.color = config.model.color_from_channels(channels);
            }
            ActiveSelection::Bar(index) => {
                let Some(config) = self
                    .presets
                    .get(self.selected_preset)
                    .and_then(|preset| preset.bars.get(index))
                    .cloned()
                else {
                    return;
                };
                let Some(bounds) = self.bar_bounds.get(index).copied() else {
                    return;
                };
                let factor = ((position.x - bounds.origin.x).as_f32() / bounds.size.width.as_f32())
                    .clamp(0.0, 1.0);
                let channel = config.channel as usize;
                let mut channels = config.model.channels(self.color, &self.profile);
                let range = config.model.channel_ranges()[channel];
                channels[channel] = range.x + (range.y - range.x) * factor;
                self.color = config.model.color_from_channels(channels);
            }
        }

        self.sync_bar_inputs(window, cx);
        self.redraw_config(cx);
    }

    fn finish_active_selection(
        &mut self,
        target: SurfaceTarget,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let matches_target = matches!(
            (self.active_selection, target),
            (Some(ActiveSelection::Plane(active)), SurfaceTarget::Plane(target))
                | (Some(ActiveSelection::Ring(active)), SurfaceTarget::Plane(target))
                | (Some(ActiveSelection::Bar(active)), SurfaceTarget::Bar(target))
                if active == target
        );
        if !matches_target {
            return;
        }
        self.update_active_selection(position, window, cx);
        self.active_selection = None;
        cx.emit(ColorSelectorEvent::Confirmed(self.color));
    }

    fn plane_indicator_position(&self, index: usize) -> Option<Vec2> {
        let config = self.presets.get(self.selected_preset)?.planes.get(index)?;
        let texture_size = self.plane_targets.get(index)?.1.width() as f32;
        let channels = config.model.channels(self.color, &self.profile);
        let ranges = config.model.channel_ranges();
        let variable_channels = self.plane_variable_channels(config);
        let mut uv = Vec2::ZERO;
        let mut variable_index = 0;
        for channel in 0..3 {
            if variable_channels & (1 << channel) != 0 {
                uv[variable_index] = ((channels[channel] - ranges[channel].x)
                    / (ranges[channel].y - ranges[channel].x))
                    .clamp(0.0, 1.0);
                variable_index += 1;
            }
        }
        uv = clamp_plane_uv(config.shape, uv);

        let mut position =
            plane_uv_to_position(config.shape, uv) * plane_scale(config, texture_size);
        if config.flip_axis.contains(config::GradientPlaneFlipAxis::X) {
            position.x = -position.x;
        }
        if config.flip_axis.contains(config::GradientPlaneFlipAxis::Y) {
            position.y = -position.y;
        }
        position = rotate_clockwise(position, config.rotation);
        Some(Vec2::new(
            (position.x + 1.0) * 0.5,
            (1.0 - position.y) * 0.5,
        ))
    }

    fn ring_indicator_position(&self, index: usize) -> Option<Vec2> {
        let config = self.presets.get(self.selected_preset)?.planes.get(index)?;
        if !config.show_primary_channel_ring {
            return None;
        }
        let texture_size = self.plane_targets.get(index)?.1.width() as f32;
        let channel = self.plane_primary_channel(config) as usize;
        let channels = config.model.channels(self.color, &self.profile);
        let range = config.model.channel_ranges()[channel];
        let factor = ((channels[channel] - range.x) / (range.y - range.x)).clamp(0.0, 1.0);
        let angle = if config.reversed_ring {
            -factor * std::f32::consts::TAU - config.ring_rotation
        } else {
            factor * std::f32::consts::TAU - config.ring_rotation
        };
        let antialias_width = 1.0 / texture_size;
        let outer_radius = 0.5 - antialias_width;
        let inner_radius =
            (outer_radius - config.primary_channel_ring_width / texture_size).max(0.0);
        let radius = (inner_radius + outer_radius) * 0.5;
        Some(Vec2::new(
            0.5 + angle.cos() * radius,
            0.5 - angle.sin() * radius,
        ))
    }

    fn bar_indicator_position(&self, index: usize) -> Option<f32> {
        let config = self.presets.get(self.selected_preset)?.bars.get(index)?;
        let channels = config.model.channels(self.color, &self.profile);
        let range = config.model.channel_ranges()[config.channel as usize];
        Some(((channels[config.channel as usize] - range.x) / (range.y - range.x)).clamp(0.0, 1.0))
    }

    fn indicator_color(&self) -> gpui::Rgba {
        let value = ColorModel::Gray.channels(self.color, &self.profile).x;
        if value > 0.5 {
            rgb(0x000000)
        } else {
            rgb(0xffffff)
        }
    }

    fn update_widget_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let width_changed = self.widget_bounds.size.width != bounds.size.width;
        self.widget_bounds = bounds;

        if width_changed && !self.presets.is_empty() {
            self.update_targets(cx);
            self.redraw_config(cx);
        }
    }

    fn redraw_config(&self, cx: &mut Context<Self>) {
        let Some(preset) = self.presets.get(self.selected_preset) else {
            return;
        };

        let device = cx.render_device();
        let queue = cx.render_queue();

        for (config, (mesh, texture, view)) in preset.planes.iter().zip(&self.plane_targets) {
            let settings = GradientSettings::new_plane(
                config.model.channels(self.color, &self.profile),
                config,
                self.primary_channel_override(config.model),
                texture.width() as f32,
            );

            if config.show_primary_channel_ring {
                self.ring_pipeline.draw(device, queue, &settings, view);
            }
            self.gradient_pipeline.draw(
                device,
                queue,
                mesh,
                &settings,
                view,
                config.show_primary_channel_ring,
            );
        }

        for (config, (mesh, _, view)) in preset.bars.iter().zip(&self.bar_targets) {
            let settings = GradientSettings::new_bar(
                config.model.channels(self.color, &self.profile),
                config,
                self.bar_uses_saturated_primary_channel(config),
            );
            self.gradient_pipeline
                .draw(device, queue, mesh, &settings, view, false);
        }

        cx.notify();
    }

    fn update_targets(&mut self, cx: &mut Context<Self>) {
        let Some(preset) = self.presets.get(self.selected_preset) else {
            self.plane_targets.clear();
            self.bar_targets.clear();
            return;
        };
        let device = cx.render_device();

        let width = self.widget_bounds.size.width.as_f32();
        if width <= 0.0 {
            self.plane_targets.clear();
            self.bar_targets.clear();
            return;
        }

        if preset.planes.is_empty() {
            self.plane_targets.clear();
        } else {
            let columns = preset
                .planes
                .len()
                .min(preset.max_planes_per_row.clamp(1, 5));
            let available_width =
                (width - 5.0 * columns.saturating_sub(1) as f32).max(columns as f32);
            let per_size = (available_width / columns as f32)
                .floor()
                .max(1.0)
                .min(preset.max_plane_size.clamp(128, 512) as f32)
                as u32;
            self.plane_targets = preset
                .planes
                .iter()
                .map(|config| {
                    let (texture, view) =
                        Self::create_gradient_texture("plane_gradient", per_size, per_size, device);
                    let scale = plane_scale(config, per_size as f32);
                    let mesh = GradientMesh::new_plane(device, config.shape, scale);
                    (mesh, texture, view)
                })
                .collect();
        }

        let bar_width = width.round().max(1.0) as u32;
        self.bar_targets = preset
            .bars
            .iter()
            .map(|config| {
                let bar_height = config.bar_height.clamp(10.0, 40.0).round() as u32;
                let (texture, view) =
                    Self::create_gradient_texture("bar_gradient", bar_width, bar_height, device);
                let mesh = GradientMesh::new_bar(device);
                (mesh, texture, view)
            })
            .collect();
        self.plane_bounds
            .resize(preset.planes.len(), Bounds::default());
        self.bar_bounds.resize(preset.bars.len(), Bounds::default());
    }

    fn update_bar_target_width(&mut self, index: usize, width: Pixels, cx: &mut Context<Self>) {
        let Some(config) = self
            .presets
            .get(self.selected_preset)
            .and_then(|preset| preset.bars.get(index))
        else {
            return;
        };
        let width = width.as_f32().round().max(1.0) as u32;
        let height = config.bar_height.clamp(10.0, 40.0).round() as u32;
        if self
            .bar_targets
            .get(index)
            .is_some_and(|(_, texture, _)| texture.width() == width && texture.height() == height)
            || index >= self.bar_targets.len()
        {
            return;
        }

        let device = cx.render_device();
        let (texture, view) = Self::create_gradient_texture("bar_gradient", width, height, device);
        let mesh = GradientMesh::new_bar(device);
        self.bar_targets[index] = (mesh, texture, view);
        self.redraw_config(cx);
    }

    fn switch_preset(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.presets.len() || index == self.selected_preset {
            return;
        }

        self.selected_preset = index;
        self.active_selection = None;
        self.rebuild_bar_inputs(window, cx);
        self.update_targets(cx);
        self.redraw_config(cx);
    }

    fn create_gradient_texture(
        label: &'static str,
        width: u32,
        height: u32,
        device: &Device,
    ) -> (Arc<Texture>, TextureView) {
        let texture = device.create_texture(&TextureDescriptor {
            label: Some(label),
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba16Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        (Arc::new(texture), view)
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
        let bars = self.presets[self.selected_preset]
            .bars
            .iter()
            .zip(&self.bar_targets)
            .zip(&self.bar_inputs)
            .enumerate()
            .map(|(index, ((config, (_, texture, _)), input))| {
                let channel = config.channel as usize;
                let label = config.model.channel_labels()[channel];
                let locked = self.primary_channel_override(config.model) == Some(config.channel);
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
                                    texture: texture.clone(),
                                    size: Size::new(
                                        texture.width().into(),
                                        texture.height().into(),
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
                            .w(px(96.0))
                            .flex_none()
                            .child(NumberInput::new(input).small().w_full())
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
            .child(
                v_flex().flex_shrink_0().gap(px(5.0)).children(
                    self.plane_targets
                        .chunks(max_planes_per_row)
                        .enumerate()
                        .map(|(row_index, row)| {
                            h_flex()
                                .justify_evenly()
                                .children(row.iter().enumerate().map(
                                    |(column_index, (_, texture, _))| {
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
                                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                        this.start_plane_selection(
                                            index,
                                            event.position,
                                            window,
                                            cx,
                                        );
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
                                        if drag.selector != cx.entity_id() || drag.target != target
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
                                        texture: texture.clone(),
                                        size: Size::new(
                                            texture.width().into(),
                                            texture.height().into(),
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
                                    },
                                ))
                        }),
                ),
            )
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
