use glam::Vec2;
use gpui::{Context, Empty, EntityId, IntoElement, Pixels, Point, Render, Window};

use crate::{ColorSelectorEvent, ColorSelectorState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceTarget {
    Plane(usize),
    Bar(usize),
}

#[derive(Clone)]
pub(crate) struct SurfaceDrag {
    pub(crate) selector: EntityId,
    pub(crate) target: SurfaceTarget,
}

pub(crate) struct SurfaceDragPreview;

impl Render for SurfaceDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveSelection {
    Plane(usize),
    Ring(usize),
    Bar(usize),
}

fn remap_normalized(value: f32, range: Vec2) -> f32 {
    range.x + (range.y - range.x) * value
}

impl ColorSelectorState {
    pub(crate) fn start_plane_selection(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let config = &self.presets[self.selected_preset].planes[index];
        let Some(bounds) = self.planes.get(index).map(|plane| plane.bounds) else {
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

    pub(crate) fn start_bar_selection(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if index >= self.bars.len() {
            return;
        }
        self.active_selection = Some(ActiveSelection::Bar(index));
        self.update_active_selection(position, window, cx);
    }

    pub(crate) fn update_active_selection(
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
                let config = &self.presets[self.selected_preset].planes[index];
                let Some((uv, _)) = self.plane_uv_from_window_position(index, position) else {
                    return;
                };
                let (x_range, y_range) = self.plane_normalized_ranges(index);
                let uv = Vec2::new(
                    remap_normalized(uv.x, x_range),
                    remap_normalized(uv.y, y_range),
                );
                let mut channels = config.model.channels(self.color, &self.profile);
                let ranges = config.model.channel_ranges();
                let mut variable_index = 0;
                let variable_channels = self.plane_variable_channels(index, config);
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
                let config = &self.presets[self.selected_preset].planes[index];
                let Some(bounds) = self.planes.get(index).map(|plane| plane.bounds) else {
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
                let channel = self.plane_primary_channel(index, config) as usize;
                let mut channels = config.model.channels(self.color, &self.profile);
                let range = config.model.channel_ranges()[channel];
                channels[channel] = range.x + (range.y - range.x) * factor;
                self.color = config.model.color_from_channels(channels);
            }
            ActiveSelection::Bar(index) => {
                let config = &self.presets[self.selected_preset].bars[index];
                let Some(bounds) = self.bars.get(index).map(|bar| bar.bounds) else {
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

    pub(crate) fn finish_active_selection(
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
}
