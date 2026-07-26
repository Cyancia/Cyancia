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
    App, AppContext, Bounds, Context, Entity, IntoElement, ObjectFit, ParentElement, Pixels,
    Render, RenderOnce, Size, Styled, SurfaceSource, Window, div, px, surface,
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
    config::{ColorSelectorConfig, GradientBarConfig},
    render::{GradientMesh, GradientPipeline, GradientRingPipeline, GradientSettings},
};

pub mod config;
pub mod render;

const MAX_PLANES_PER_ROW: usize = 2;
const MAX_PLANE_SIZE: u32 = 256;
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
    #[display("Okhsl")]
    OkHsl,
    #[display("Okhsv")]
    OkHsv,
    #[display("Oklab")]
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

    pub fn get_reference_color(&self, color: Color) -> Vec3 {
        match color {
            Color::Gray(gray) => Vec3::new(gray.v, 0.0, 0.0),
            Color::Hsl(hsl) => Vec3::new(hsl.h, hsl.s, hsl.l),
            Color::Hsv(hsv) => Vec3::new(hsv.h, hsv.s, hsv.v),
            Color::Lab(lab) => Vec3::new(lab.l, lab.a, lab.b),
            Color::Lch(lch) => Vec3::new(lch.l, lch.c, lch.h),
            Color::OkHsl(ok_hsl) => Vec3::new(ok_hsl.h, ok_hsl.s, ok_hsl.l),
            Color::OkHsv(ok_hsv) => Vec3::new(ok_hsv.h, ok_hsv.s, ok_hsv.v),
            Color::OkLab(ok_lab) => Vec3::new(ok_lab.l, ok_lab.a, ok_lab.b),
            Color::OkLch(ok_lch) => Vec3::new(ok_lch.l, ok_lch.c, ok_lch.h),
            Color::Rgb(rgb) => Vec3::new(rgb.r, rgb.g, rgb.b),
            Color::Xyz(xyz) => Vec3::new(xyz.x, xyz.y, xyz.z),
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

    pub fn from_reference_color(self, color: Vec3) -> Color {
        match self {
            Self::Gray => Color::Gray(Gray::new(color.x)),
            Self::Hsl => Color::Hsl(Hsl::new(color.x, color.y, color.z)),
            Self::Hsv => Color::Hsv(Hsv::new(color.x, color.y, color.z)),
            Self::Lab => Color::Lab(Lab::new(color.x, color.y, color.z)),
            Self::Lch => Color::Lch(Lch::new(color.x, color.y, color.z)),
            Self::OkHsl => Color::OkHsl(OkHsl::new(color.x, color.y, color.z)),
            Self::OkHsv => Color::OkHsv(OkHsv::new(color.x, color.y, color.z)),
            Self::OkLab => Color::OkLab(OkLab::new(color.x, color.y, color.z)),
            Self::OkLch => Color::OkLch(OkLch::new(color.x, color.y, color.z)),
            Self::Rgb => Color::Rgb(Rgb::new(color.x, color.y, color.z)),
            Self::Xyz => Color::Xyz(Xyz::new(color.x, color.y, color.z)),
        }
    }

    pub fn convert_to_self(&self, color: Color, profile: &ColorProfile) -> Color {
        let xyz = match color {
            Color::Gray(gray) => gray.into_xyz(),
            Color::Hsl(hsl) => hsl
                .into_rgb()
                .into_xyz(profile.rgb_to_xyz_matrix().to_f32()),
            Color::Hsv(hsv) => hsv
                .into_rgb()
                .into_xyz(profile.rgb_to_xyz_matrix().to_f32()),
            Color::Lab(lab) => lab.into_xyz(),
            Color::Lch(lch) => lch.into_xyz(),
            Color::OkHsl(ok_hsl) => ok_hsl.into_xyz(),
            Color::OkHsv(ok_hsv) => ok_hsv.into_xyz(),
            Color::OkLab(ok_lab) => ok_lab.into_xyz(),
            Color::OkLch(ok_lch) => ok_lch.into_xyz(),
            Color::Rgb(rgb) => rgb.into_xyz(profile.rgb_to_xyz_matrix().to_f32()),
            Color::Xyz(xyz) => xyz,
        };

        match self {
            ColorModel::Gray => Color::Gray(Gray::from_xyz(xyz)),
            ColorModel::Hsl => Color::Hsl(Hsl::from_rgb(Rgb::from_xyz(
                xyz,
                profile.rgb_to_xyz_matrix().to_f32().inverse(),
            ))),
            ColorModel::Hsv => Color::Hsv(Hsv::from_rgb(Rgb::from_xyz(
                xyz,
                profile.rgb_to_xyz_matrix().to_f32().inverse(),
            ))),
            ColorModel::Lab => Color::Lab(Lab::from_xyz(xyz)),
            ColorModel::Lch => Color::Lch(Lch::from_xyz(xyz)),
            ColorModel::OkHsl => Color::OkHsl(OkHsl::from_xyz(xyz)),
            ColorModel::OkHsv => Color::OkHsv(OkHsv::from_xyz(xyz)),
            ColorModel::OkLab => Color::OkLab(OkLab::from_xyz(xyz)),
            ColorModel::OkLch => Color::OkLch(OkLch::from_xyz(xyz)),
            ColorModel::Rgb => Color::Rgb(Rgb::from_xyz(
                xyz,
                profile.rgb_to_xyz_matrix().to_f32().inverse(),
            )),
            ColorModel::Xyz => Color::Xyz(xyz),
        }
    }
}

pub struct ColorSelectorState {
    color: Color,
    profile: ColorProfile,

    presets: Vec<ColorSelectorConfig>,
    selected_preset: usize,

    plane_pipeline: GradientPipeline,
    ring_pipeline: GradientRingPipeline,
    bar_pipeline: GradientPipeline,
    plane_targets: Vec<(GradientMesh, Arc<Texture>, TextureView)>,
    bar_targets: Vec<(GradientMesh, Arc<Texture>, TextureView)>,
    bar_inputs: Vec<Entity<InputState>>,
    primary_channel_overrides: Vec<[Option<u8>; COLOR_MODEL_COUNT]>,

    widget_bounds: Bounds<Pixels>,
    surface_format: TextureFormat,
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

        let preset_count = presets.len();
        let mut this = Self {
            color,
            profile,

            presets,
            selected_preset,

            plane_pipeline: GradientPipeline::new(device),
            ring_pipeline: GradientRingPipeline::new(device),
            bar_pipeline: GradientPipeline::new(device),
            plane_targets: Vec::new(),
            bar_targets: Vec::new(),
            bar_inputs: Vec::new(),
            primary_channel_overrides: vec![[None; COLOR_MODEL_COUNT]; preset_count],

            widget_bounds: Bounds::default(),
            surface_format: TextureFormat::Rgba16Float,
        };
        this.rebuild_bar_inputs(window, cx);
        this
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
            cx.notify();
            return;
        }

        self.selected_preset = self.selected_preset.min(self.presets.len() - 1);
        self.primary_channel_overrides
            .resize(self.presets.len(), [None; COLOR_MODEL_COUNT]);
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
            let value = self.bar_display_value(&config);
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
                let value = this.bar_display_value_for(model, channel) + delta;
                this.set_bar_display_value(model, channel, value, window, cx);
            })
            .detach();

            self.bar_inputs.push(input);
        }
    }

    fn bar_display_value(&self, config: &GradientBarConfig) -> f32 {
        self.bar_display_value_for(config.model, config.channel)
    }

    fn bar_display_value_for(&self, model: ColorModel, channel: u8) -> f32 {
        let color = model.convert_to_self(self.color, &self.profile);
        let reference = model.get_reference_color(color);
        reference[channel as usize] * model.display_scale()[channel as usize]
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
        let mut reference =
            model.get_reference_color(model.convert_to_self(self.color, &self.profile));
        reference[channel] = (value / scale).clamp(range.x, range.y);
        self.color = model.from_reference_color(reference);
        self.sync_bar_inputs(window, cx);
        self.redraw_config(cx);
    }

    fn sync_bar_inputs(&self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(preset) = self.presets.get(self.selected_preset) else {
            return;
        };
        for (config, input) in preset.bars.iter().zip(&self.bar_inputs) {
            let value = self.bar_display_value(config);
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
            let reference_color = config.model.convert_to_self(self.color, &self.profile);
            let reference = config.model.get_reference_color(reference_color);
            let settings = GradientSettings::new_plane(
                &self.profile,
                reference,
                config,
                self.primary_channel_override(config.model),
                config.primary_channel_ring_width,
                texture.width() as f32,
            );

            if config.show_primary_channel_ring {
                self.ring_pipeline.draw(device, queue, &settings, view);
            }
            self.plane_pipeline.draw(
                device,
                queue,
                mesh,
                &settings,
                view,
                config.show_primary_channel_ring,
            );
        }

        for (config, (mesh, texture, view)) in preset.bars.iter().zip(&self.bar_targets) {
            let reference_color = config.model.convert_to_self(self.color, &self.profile);
            let reference = config.model.get_reference_color(reference_color);
            let settings =
                GradientSettings::new_bar(&self.profile, reference, config, texture.width() as f32);
            self.bar_pipeline
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
            let columns = preset.planes.len().min(MAX_PLANES_PER_ROW);
            let per_size = ((width / columns as f32).round().max(1.0) as u32).min(MAX_PLANE_SIZE);
            self.plane_targets = preset
                .planes
                .iter()
                .map(|config| {
                    let (texture, view) =
                        self.create_gradient_texture("plane_gradient", per_size, per_size, device);
                    let scale = if config.show_primary_channel_ring {
                        let texture_size = per_size as f32;
                        let antialias_width = 1.0 / texture_size;
                        let inner_radius = (0.5
                            - antialias_width
                            - (config.primary_channel_ring_width + GRADIENT_RING_GAP)
                                / texture_size)
                            .max(0.0);
                        let circumradius = match config.shape {
                            GradientPlaneShape::Square => std::f32::consts::SQRT_2,
                            GradientPlaneShape::Triangle => 1.0,
                        };
                        2.0 * inner_radius / circumradius
                    } else {
                        1.0
                    };
                    let mesh = GradientMesh::new_scaled(device, config.shape.into(), scale);
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
                    self.create_gradient_texture("bar_gradient", bar_width, bar_height, device);
                let mesh = GradientMesh::new(device, render::GradientShape::Bar);
                (mesh, texture, view)
            })
            .collect();
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
        let (texture, view) = self.create_gradient_texture("bar_gradient", width, height, device);
        let mesh = GradientMesh::new(device, render::GradientShape::Bar);
        self.bar_targets[index] = (mesh, texture, view);
        self.redraw_config(cx);
    }

    fn switch_preset(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.presets.len() || index == self.selected_preset {
            return;
        }

        self.selected_preset = index;
        self.rebuild_bar_inputs(window, cx);
        self.update_targets(cx);
        self.redraw_config(cx);
    }

    fn create_gradient_texture(
        &self,
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
            format: self.surface_format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        (Arc::new(texture), view)
    }
}

impl Render for ColorSelectorState {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.presets.is_empty() {
            return div().into_any_element();
        }

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
                    .w_full()
                    .gap_1()
                    .children(
                        config
                            .show_channel_label
                            .then(|| div().w(px(12.0)).flex_shrink_0().child(label)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .h(px(config.bar_height.clamp(10.0, 40.0)))
                            .on_prepaint({
                                let state = cx.entity().downgrade();
                                move |bounds, _, cx| {
                                    state
                                        .update(cx, |state, cx| {
                                            state.update_bar_target_width(
                                                index,
                                                bounds.size.width,
                                                cx,
                                            );
                                        })
                                        .ok();
                                }
                            })
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
                            ),
                    )
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
            .size_full()
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
            .child(v_flex().w_full().flex_shrink_0().children(
                self.plane_targets.chunks(MAX_PLANES_PER_ROW).map(|row| {
                    h_flex().w_full().justify_evenly().children(row.iter().map(
                        |(_, texture, _)| {
                            surface(SurfaceSource::Texture {
                                texture: texture.clone(),
                                size: Size::new(texture.width().into(), texture.height().into()),
                            })
                            .size(px(texture.width() as f32))
                        },
                    ))
                }),
            ))
            .child(
                v_flex()
                    .w_full()
                    .flex_shrink_0()
                    .gap_2()
                    .p_2()
                    .children(bars),
            )
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

#[derive(IntoElement)]
pub struct ColorSelector {
    state: Entity<ColorSelectorState>,
}

impl ColorSelector {
    pub fn new(state: &Entity<ColorSelectorState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

impl RenderOnce for ColorSelector {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        div().size_full().child(self.state)
    }
}
