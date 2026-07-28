use std::sync::Arc;

use cyancia_render::render_context::RenderContextAppExt;
use glam::{Mat2, Vec2};
use gpui::{Bounds, Context, Pixels, Point, rgb};
use wgpu::{
    Device, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages,
    TextureView,
};

use crate::{
    ColorModel, ColorSelectorState, GRADIENT_RING_GAP, GradientPlaneShape, PlaneState,
    config::{GradientPlaneConfig, GradientPlaneFlipAxis},
    pipeline::{GradientMesh, GradientSettings},
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
        let bounds = self.planes.get(index)?.bounds;
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
        let texture_size = self.planes.get(index)?.texture.width() as f32;
        let channels = config.model.channels(self.color, &self.profile);
        let ranges = config.model.channel_ranges();
        let variable_channels = self.plane_variable_channels(index, config);
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
        let texture_size = self.planes.get(index)?.texture.width() as f32;
        let channel = self.plane_primary_channel(index, config) as usize;
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

        for (index, (config, plane)) in preset.planes.iter().zip(&self.planes).enumerate() {
            let ring_settings = GradientSettings::new_plane(
                preset.out_of_gamut_color,
                preset.use_out_of_gamut_color,
                config.model.channels(self.color, &self.profile),
                config,
                plane.primary_channel_override,
                plane.texture.width() as f32,
            );
            let plane_settings = GradientSettings {
                saturate_primary_channel: 0,
                ..ring_settings.clone()
            };

            if preset.clip_to_gamut {
                let readback =
                    self.compute_bounds_pipeline
                        .compute(&device, &queue, &plane_settings);
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
                            let Some(plane) = this.planes.get_mut(index) else {
                                return;
                            };
                            plane.ranges = (x_range, y_range);
                            let plane_settings = GradientSettings {
                                x_range,
                                y_range,
                                ..plane_settings
                            };
                            if preserve_output {
                                let ring_settings = GradientSettings {
                                    x_range,
                                    y_range,
                                    ..ring_settings
                                };
                                this.ring_pipeline.draw(
                                    cx.render_device(),
                                    cx.render_queue(),
                                    &ring_settings,
                                    &plane.texture_view,
                                );
                            }
                            this.gradient_pipeline.draw(
                                cx.render_device(),
                                cx.render_queue(),
                                &plane.mesh,
                                &plane_settings,
                                &plane.texture_view,
                                preserve_output,
                            );
                            cx.notify();
                        })
                        .ok();
                })
                .detach();
            } else {
                if config.show_primary_channel_ring {
                    self.ring_pipeline
                        .draw(&device, &queue, &ring_settings, &plane.texture_view);
                }
                self.gradient_pipeline.draw(
                    &device,
                    &queue,
                    &plane.mesh,
                    &plane_settings,
                    &plane.texture_view,
                    config.show_primary_channel_ring,
                );
            }
        }

        for (config, bar) in preset.bars.iter().zip(&self.bars) {
            let settings = GradientSettings::new_bar(
                preset.out_of_gamut_color,
                preset.use_out_of_gamut_color,
                config.model.channels(self.color, &self.profile),
                config,
                self.bar_uses_saturated_primary_channel(config),
            );
            self.gradient_pipeline.draw(
                &device,
                &queue,
                &bar.mesh,
                &settings,
                &bar.texture_view,
                false,
            );
        }

        cx.notify();
    }

    pub(crate) fn update_targets(&mut self, cx: &mut Context<Self>) {
        let Some(preset) = self.presets.get(self.selected_preset) else {
            self.planes.clear();
            self.bars.clear();
            return;
        };

        let width = self.widget_bounds.size.width.as_f32();
        if width <= 0.0 {
            return;
        }
        let device = cx.render_device();

        let columns = preset
            .planes
            .len()
            .min(preset.max_planes_per_row.clamp(1, 5));
        let available_width = (width - 5.0 * columns.saturating_sub(1) as f32).max(columns as f32);
        let per_size = (available_width / columns as f32)
            .floor()
            .max(1.0)
            .min(preset.max_plane_size.clamp(128, 512) as f32) as u32;
        let old_planes = std::mem::take(&mut self.planes);
        self.planes = preset
            .planes
            .iter()
            .enumerate()
            .map(|(index, config)| {
                let (texture, texture_view) =
                    Self::create_gradient_texture("plane_gradient", per_size, per_size, device);
                PlaneState {
                    mesh: GradientMesh::new_plane(
                        device,
                        config.shape,
                        plane_scale(config, per_size as f32),
                    ),
                    texture,
                    texture_view,
                    ranges: (Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0)),
                    bounds: old_planes
                        .get(index)
                        .map_or_else(Bounds::default, |plane| plane.bounds),
                    primary_channel_override: old_planes
                        .get(index)
                        .and_then(|plane| plane.primary_channel_override),
                }
            })
            .collect();

        let bar_width = width.round().max(1.0) as u32;
        for (config, bar) in preset.bars.iter().zip(&mut self.bars) {
            let bar_height = config.bar_height.clamp(10.0, 40.0).round() as u32;
            let (texture, texture_view) =
                Self::create_gradient_texture("bar_gradient", bar_width, bar_height, device);
            bar.mesh = GradientMesh::new_bar(device);
            bar.texture = texture;
            bar.texture_view = texture_view;
        }
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
        let Some(bar) = self.bars.get(index) else {
            return;
        };
        let width = width.as_f32().round().max(1.0) as u32;
        let height = config.bar_height.clamp(10.0, 40.0).round() as u32;
        if bar.texture.width() == width && bar.texture.height() == height {
            return;
        }

        let device = cx.render_device();
        let (texture, texture_view) =
            Self::create_gradient_texture("bar_gradient", width, height, device);
        let bar = &mut self.bars[index];
        bar.mesh = GradientMesh::new_bar(device);
        bar.texture = texture;
        bar.texture_view = texture_view;
        self.redraw_config(cx);
    }

    pub(crate) fn update_plane_bounds(
        &mut self,
        index: usize,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(plane) = self.planes.get_mut(index) else {
            return;
        };
        plane.bounds = bounds;

        let size = bounds
            .size
            .width
            .as_f32()
            .min(bounds.size.height.as_f32())
            .round()
            .max(1.0) as u32;
        if plane.texture.width() == size && plane.texture.height() == size {
            return;
        }

        let config = &self.presets[self.selected_preset].planes[index];
        let device = cx.render_device();
        let (texture, texture_view) =
            Self::create_gradient_texture("plane_gradient", size, size, device);
        let plane = &mut self.planes[index];
        plane.mesh =
            GradientMesh::new_plane(device, config.shape, plane_scale(config, size as f32));
        plane.texture = texture;
        plane.texture_view = texture_view;
        plane.ranges = (Vec2::new(0.0, 1.0), Vec2::new(0.0, 1.0));
        self.redraw_config(cx);
    }

    pub(crate) fn update_bar_bounds(
        &mut self,
        index: usize,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(bar) = self.bars.get_mut(index) else {
            return;
        };
        bar.bounds = bounds;
        self.update_bar_target_width(index, bounds.size.width, cx);
    }

    pub(crate) fn create_gradient_texture(
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
