use std::sync::Arc;

use cyancia_color::{
    Color,
    model::{
        gray::Gray, hsl::Hsl, hsv::Hsv, lab::Lab, lch::Lch, okhsl::OkHsl, okhsv::OkHsv,
        oklab::OkLab, oklch::OkLch, rgb::Rgb, xyz::Xyz,
    },
};
use cyancia_render::render_context::RenderContextAppExt;
use glam::{Mat2, Vec2, Vec3};
use gpui::{
    AppContext, Bounds, Context, DisplayId, DragMoveEvent, Empty, Entity, EntityId, EventEmitter,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, ObjectFit,
    ParentElement, Pixels, Point, Render, Size, StatefulInteractiveElement, Styled, Subscription,
    SurfaceSource, Window, div, px, relative, rgb, surface,
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
    ColorModel, ColorSelectorState, GRADIENT_RING_GAP, GradientPlaneShape,
    config::{ColorSelectorConfig, GradientBarConfig, GradientPlaneConfig, GradientPlaneFlipAxis},
    control::ActiveSelection,
    pipeline::{
        ComputeBoundsPipeline, GradientMesh, GradientPipeline, GradientRingPipeline,
        GradientSettings,
    },
};

fn unmap_normalized(value: f32, range: Vec2) -> f32 {
    let width = range.y - range.x;
    if width.abs() <= f32::EPSILON {
        0.5
    } else {
        (value - range.x) / width
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

fn plane_position_to_uv(shape: GradientPlaneShape, position: Vec2) -> Vec2 {
    match shape {
        GradientPlaneShape::Square => (position + Vec2::ONE) * 0.5,
        GradientPlaneShape::Triangle => {
            let y = (1.0 - position.y) / 1.5;
            let x = 0.5 + position.x / (3.0_f32.sqrt() * y.max(f32::EPSILON));
            Vec2::new(x, y)
        }
    }
}

fn plane_uv_to_position(shape: GradientPlaneShape, uv: Vec2) -> Vec2 {
    match shape {
        GradientPlaneShape::Square => uv * 2.0 - Vec2::ONE,
        GradientPlaneShape::Triangle => {
            Vec2::new(3.0_f32.sqrt() * uv.y * (uv.x - 0.5), 1.0 - 1.5 * uv.y)
        }
    }
}

impl ColorSelectorState {
    pub(crate) fn plane_uv_from_window_position(
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
        let mut input_position = Mat2::from_angle(config.rotation) * output_position;
        if config.flip_axis.contains(GradientPlaneFlipAxis::X) {
            input_position.x = -input_position.x;
        }
        if config.flip_axis.contains(GradientPlaneFlipAxis::Y) {
            input_position.y = -input_position.y;
        }

        let scale = plane_scale(config, width);
        if scale <= f32::EPSILON {
            return None;
        }
        input_position /= scale;
        let uv = plane_position_to_uv(config.shape, input_position);
        let clamped = uv.clamp(Vec2::ZERO, Vec2::ONE);
        Some((clamped, (uv - clamped).length_squared() <= 1e-6))
    }

    pub(crate) fn plane_indicator_position(&self, index: usize) -> Option<Vec2> {
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
        let (x_range, y_range) = self.plane_normalized_ranges(index);
        uv.x = unmap_normalized(uv.x, x_range);
        uv.y = unmap_normalized(uv.y, y_range);
        uv = uv.clamp(Vec2::ZERO, Vec2::ONE);

        let mut position =
            plane_uv_to_position(config.shape, uv) * plane_scale(config, texture_size);
        if config.flip_axis.contains(GradientPlaneFlipAxis::X) {
            position.x = -position.x;
        }
        if config.flip_axis.contains(GradientPlaneFlipAxis::Y) {
            position.y = -position.y;
        }
        position = Mat2::from_angle(-config.rotation) * position;
        Some(Vec2::new(
            (position.x + 1.0) * 0.5,
            (1.0 - position.y) * 0.5,
        ))
    }

    pub(crate) fn ring_indicator_position(&self, index: usize) -> Option<Vec2> {
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

    pub(crate) fn bar_indicator_position(&self, index: usize) -> Option<f32> {
        let config = self.presets.get(self.selected_preset)?.bars.get(index)?;
        let channels = config.model.channels(self.color, &self.profile);
        let range = config.model.channel_ranges()[config.channel as usize];
        Some(((channels[config.channel as usize] - range.x) / (range.y - range.x)).clamp(0.0, 1.0))
    }

    pub(crate) fn indicator_color(&self) -> gpui::Rgba {
        let value = ColorModel::Gray.channels(self.color, &self.profile).x;
        if value > 0.5 {
            rgb(0x000000)
        } else {
            rgb(0xffffff)
        }
    }

    pub(crate) fn update_widget_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let width_changed = self.widget_bounds.size.width != bounds.size.width;
        self.widget_bounds = bounds;

        if width_changed && !self.presets.is_empty() {
            self.update_targets(cx);
            self.redraw_config(cx);
        }
    }

    pub(crate) fn redraw_config(&self, cx: &mut Context<Self>) {
        let preset = &self.presets[self.selected_preset];

        let device = cx.render_device().clone();
        let queue = cx.render_queue().clone();

        for (index, (config, (mesh, texture, view))) in
            preset.planes.iter().zip(&self.plane_targets).enumerate()
        {
            let settings = GradientSettings::new_plane(
                preset.out_of_gamut_color,
                preset.use_out_of_gamut_color,
                config.model.channels(self.color, &self.profile),
                config,
                self.primary_channel_override(config.model),
                texture.width() as f32,
            );

            if preset.clip_to_gamut {
                let readback = self
                    .compute_bounds_pipeline
                    .compute(&device, &queue, &settings);
                let preserve_output = config.show_primary_channel_ring;
                cx.spawn(async move |state, cx| {
                    let Ok(Ok(bounds)) = readback.into_inner().await else {
                        return;
                    };
                    let (x_range, y_range) = bounds
                        .normalized_ranges()
                        .unwrap_or((Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0)));
                    state
                        .update(cx, move |this, cx| {
                            let (mesh, _, view) = &this.plane_targets[index];
                            this.plane_ranges[index] = (x_range, y_range);
                            let settings = GradientSettings {
                                x_range,
                                y_range,
                                ..settings
                            };
                            if preserve_output {
                                this.ring_pipeline.draw(
                                    cx.render_device(),
                                    cx.render_queue(),
                                    &settings,
                                    view,
                                );
                            }
                            this.gradient_pipeline.draw(
                                cx.render_device(),
                                cx.render_queue(),
                                mesh,
                                &settings,
                                view,
                                preserve_output,
                            );
                            cx.notify();
                        })
                        .ok();
                })
                .detach();
            } else {
                if config.show_primary_channel_ring {
                    self.ring_pipeline.draw(&device, &queue, &settings, view);
                }
                self.gradient_pipeline.draw(
                    &device,
                    &queue,
                    mesh,
                    &settings,
                    view,
                    config.show_primary_channel_ring,
                );
            }
        }

        for (config, (mesh, _, view)) in preset.bars.iter().zip(&self.bar_targets) {
            let settings = GradientSettings::new_bar(
                preset.out_of_gamut_color,
                preset.use_out_of_gamut_color,
                config.model.channels(self.color, &self.profile),
                config,
                self.bar_uses_saturated_primary_channel(config),
            );
            self.gradient_pipeline
                .draw(&device, &queue, mesh, &settings, view, false);
        }

        cx.notify();
    }

    pub(crate) fn update_targets(&mut self, cx: &mut Context<Self>) {
        let Some(preset) = self.presets.get(self.selected_preset) else {
            self.plane_targets.clear();
            self.plane_ranges.clear();
            self.bar_targets.clear();
            return;
        };
        let device = cx.render_device();

        let width = self.widget_bounds.size.width.as_f32();
        if width <= 0.0 {
            self.plane_targets.clear();
            self.plane_ranges.clear();
            self.bar_targets.clear();
            return;
        }

        // plane targets

        let columns = preset
            .planes
            .len()
            .min(preset.max_planes_per_row.clamp(1, 5));
        let available_width = (width - 5.0 * columns.saturating_sub(1) as f32).max(columns as f32);
        let per_size = (available_width / columns as f32)
            .floor()
            .max(1.0)
            .min(preset.max_plane_size.clamp(128, 512) as f32) as u32;
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
        self.plane_bounds
            .resize(preset.planes.len(), Bounds::default());
        self.plane_ranges = vec![(Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0)); preset.planes.len()];

        // bar targets

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
        self.bar_bounds.resize(preset.bars.len(), Bounds::default());
    }

    pub(crate) fn update_bar_target_width(
        &mut self,
        index: usize,
        width: Pixels,
        cx: &mut Context<Self>,
    ) {
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

    pub(crate) fn update_plane_bounds(
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

        let config = &self.presets[self.selected_preset].planes[index];
        let device = cx.render_device();
        let (texture, view) = Self::create_gradient_texture("plane_gradient", size, size, device);
        let mesh = GradientMesh::new_plane(device, config.shape, plane_scale(config, size as f32));
        self.plane_targets[index] = (mesh, texture, view);
        if let Some(ranges) = self.plane_ranges.get_mut(index) {
            *ranges = (Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0));
        }
        self.redraw_config(cx);
    }

    pub(crate) fn update_bar_bounds(
        &mut self,
        index: usize,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if self.bar_bounds.len() <= index {
            self.bar_bounds.resize(index + 1, Bounds::default());
        }
        self.bar_bounds[index] = bounds;
        self.update_bar_target_width(index, bounds.size.width, cx);
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
