use std::sync::Arc;

use cyancia_color::{
    Color,
    model::{
        gray::Gray, lab::Lab, lch::Lch, okhsl::OkHsl, okhsv::OkHsv, oklab::OkLab, oklch::OkLch,
        rgb::Rgb,
    },
};
use cyancia_render::render_context::RenderContextAppExt;
use glam::{Vec2, Vec3};
use gpui::{
    App, Bounds, Context, Entity, IntoElement, ParentElement, Pixels, Render, RenderOnce, Size,
    Styled, SurfaceSource, Window, div, px, surface,
};
use gpui_component::{ElementExt, h_flex, radio::RadioGroup, v_flex};
use moxcms::ColorProfile;
use parse_display::Display;
use wgpu::{
    Device, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureView,
};

use crate::render::{GradientMesh, GradientPipeline, GradientSettings};

pub mod render;

const MAX_PLANES_PER_ROW: usize = 2;
const MAX_PLANE_SIZE: u32 = 256;

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
    pub fn get_reference_color(&self, color: Color) -> Vec3 {
        match color {
            Color::Gray(gray) => Vec3::new(gray.v, 0.0, 0.0),
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
            ColorModel::OkHsl | ColorModel::OkHsv => [
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

    pub fn convert_to_self(&self, color: Color, profile: &ColorProfile) -> Color {
        let xyz = match color {
            Color::Gray(gray) => gray.into_xyz(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GradientPlaneConfig {
    pub model: ColorModel,
    pub shape: GradientPlaneShape,
    pub variable_channels: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorModePreset {
    pub name: String,
    pub planes: Vec<GradientPlaneConfig>,
}

pub struct ColorSelectorState {
    color: Color,
    profile: ColorProfile,

    presets: Vec<SelectorModePreset>,
    selected_preset: usize,

    pipeline: GradientPipeline,
    plane_targets: Vec<(GradientMesh, Arc<Texture>, TextureView)>,

    widget_bounds: Bounds<Pixels>,
    surface_format: TextureFormat,
}

impl ColorSelectorState {
    pub fn new(
        color: Color,
        profile: ColorProfile,
        presets: Vec<SelectorModePreset>,
        selected_preset: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        assert!(
            !presets.is_empty(),
            "at least one selector preset is required"
        );
        assert!(
            selected_preset < presets.len(),
            "selected preset index is out of bounds"
        );
        for preset in &presets {
            assert!(
                !preset.planes.is_empty(),
                "selector presets must contain at least one plane"
            );
            for plane in &preset.planes {
                assert_eq!(
                    plane.variable_channels & !0b111,
                    0,
                    "variable channel mask contains an invalid channel"
                );
                assert!(
                    plane.variable_channels.count_ones() <= 2,
                    "a two-dimensional gradient plane supports at most two variable channels"
                );
            }
        }

        let device = cx.render_device();

        Self {
            color,
            profile,

            presets,
            selected_preset,

            pipeline: GradientPipeline::new(device),
            plane_targets: Vec::new(),

            widget_bounds: Bounds::default(),
            surface_format: TextureFormat::Rgba16Float,
        }
    }

    fn update_widget_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let width_changed = self.widget_bounds.size.width != bounds.size.width;
        self.widget_bounds = bounds;

        if width_changed {
            self.update_targets(cx);
            self.redraw_config(cx);
        }
    }

    fn redraw_config(&self, cx: &mut Context<Self>) {
        let device = cx.render_device();
        let queue = cx.render_queue();
        let preset = &self.presets[self.selected_preset];

        for (config, (mesh, _, view)) in preset.planes.iter().zip(&self.plane_targets) {
            let reference_color = config.model.convert_to_self(self.color, &self.profile);
            let reference = config.model.get_reference_color(reference_color);
            let settings = GradientSettings::new(
                &self.profile,
                reference,
                config.model,
                u32::from(config.variable_channels),
            );

            self.pipeline.draw(device, queue, mesh, &settings, view);
        }

        cx.notify();
    }

    fn update_targets(&mut self, cx: &mut Context<Self>) {
        let preset = &self.presets[self.selected_preset];
        let device = cx.render_device();

        let width = self.widget_bounds.size.width.as_f32();
        if width <= 0.0 {
            self.plane_targets.clear();
            return;
        }

        let columns = preset.planes.len().min(MAX_PLANES_PER_ROW);
        let per_size = ((width / columns as f32).round().max(1.0) as u32).min(MAX_PLANE_SIZE);
        self.plane_targets = preset
            .planes
            .iter()
            .map(|p| {
                let (t, v) =
                    self.create_gradient_texture("plane_gradient", per_size, per_size, device);
                let mesh = GradientMesh::new(device, p.shape.into());
                (mesh, t, v)
            })
            .collect();
    }

    fn switch_preset(&mut self, index: usize, cx: &mut Context<Self>) {
        if index == self.selected_preset {
            return;
        }

        self.selected_preset = index;
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
                    h_flex()
                        .w_full()
                        .children(row.iter().map(|(_, texture, _)| {
                            surface(SurfaceSource::Texture {
                                texture: texture.clone(),
                                size: Size::new(texture.width().into(), texture.height().into()),
                            })
                            .size(px(texture.width() as f32))
                        }))
                }),
            ))
            .child(
                div().flex_shrink_0().child(
                    RadioGroup::horizontal("preset-radios")
                        .children(self.presets.iter().map(|p| p.name.clone()))
                        .selected_index(Some(self.selected_preset))
                        .on_click(cx.listener(move |state, index, _, cx| {
                            state.switch_preset(*index, cx);
                        })),
                ),
            )
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
