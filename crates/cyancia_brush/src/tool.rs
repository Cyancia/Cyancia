use std::collections::VecDeque;

use bevy_math::IRect;
use chrono::{DateTime, Utc};
use cyancia_assets::{AssetAppExt, store::AssetRegistry};
use cyancia_canvas::{CCanvas, CanvasAppExt, CanvasId, CanvasManager, event::CanvasUpdated};
use cyancia_image::{
    composite::{LayerPreviewOverriders, PixelPreviewOverrider},
    layer::LayerId,
    tile::{GpuTileStorage, GpuTileStorageInner},
};
use cyancia_math::number::LerpAngle;
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::wrapper;
use glam::{FloatExt, Vec2};
use gpui::{
    App, BorrowAppContext, Global, MouseDownEvent, MouseMoveEvent, MouseUpEvent, WeakEntity,
};
use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::{
    input_processing::RawPenInput,
    instance::BrushPresetInstance,
    render::{BrushPresetOperator, Time},
};

const TIMESTAMP_MOD: i64 = 1_000_000;

#[derive(Default)]
pub struct BrushTool {
    target_layer: Option<(WeakEntity<CCanvas>, LayerId)>,
    stroke_begin: Option<DateTime<Utc>>,
}

impl ToolFunction for BrushTool {
    fn id() -> ToolId {
        ToolId::new("brush_tool")
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut App) {
        let Some(canvas_entity) = cx.current_canvas() else {
            return;
        };
        let Some(canvas) = canvas_entity.upgrade().map(|c| c.read(cx)) else {
            return;
        };

        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };
        let active_layer = canvas.image.active_layer;
        if !canvas.image.active_layer_data().can_contain_pixels() {
            log::warn!("Unable to paint to the active layer which cannot contain pixels.");
            return;
        }

        let now = Utc::now();
        self.stroke_begin = Some(now);
        self.target_layer = Some((canvas_entity, active_layer));

        let params = RawPenInput {
            position,
            time: Time {
                now: (now.timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (now.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        if !cx.has_global::<CurrentBrushPresetOperator>() {
            log::error!("No current brush preset operator found.");
            return;
        }

        cx.update_global::<CurrentBrushPresetOperator, _>(|brush, cx| {
            let Some(brush) = brush.as_mut() else {
                return;
            };
            brush.begin_stroke(params, active_layer, cx);
        });
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut App) {
        let Some((canvas_entity, active_layer)) = &self.target_layer else {
            return;
        };

        let Some(canvas_entity) = canvas_entity.upgrade() else {
            return;
        };
        let canvas = canvas_entity.read(cx);

        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };

        let Some(stroke_begin) = self.stroke_begin else {
            log::error!("Stroke update called without a stroke begin time.");
            return;
        };

        let params = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (stroke_begin.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        let maybe_preview = cx.update_global::<CurrentBrushPresetOperator, _>(|brush, cx| {
            let Some(brush) = brush.as_mut() else {
                return None;
            };
            let now = std::time::Instant::now();
            brush.update_stroke(params, cx);
            let preview = brush.generate_preview(cx);
            log::debug!("Brush stroke update took {:?}", now.elapsed());

            preview
        });

        if let Some((dirty_pixels, preview)) = maybe_preview {
            let dirty_tiles = GpuTileStorageInner::pixel_rect_to_tile(dirty_pixels);
            let overriders = cx.global_mut::<LayerPreviewOverriders>();
            overriders.insert_overrider(
                *active_layer,
                PixelPreviewOverrider {
                    texture: preview.texture().unwrap().as_ref().clone(),
                    tile_info_buffer: preview.tile_info_buffer().unwrap().clone(),
                },
            );

            canvas_entity.update(cx, |canvas, cx| {
                cx.emit(CanvasUpdated { dirty_tiles });
            });
        }
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut App) {
        let Some((canvas_entity, active_layer)) = self.target_layer.take() else {
            return;
        };

        let Some(canvas_entity) = canvas_entity.upgrade() else {
            return;
        };
        let canvas = canvas_entity.read(cx);

        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };

        let Some(stroke_begin) = self.stroke_begin.take() else {
            log::error!("Stroke end called without a stroke begin time.");
            return;
        };

        let final_input = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (stroke_begin.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        let dirty_pixels = cx.update_global::<CurrentBrushPresetOperator, _>(|brush, cx| {
            let Some(brush) = brush.as_mut() else {
                return None;
            };
            brush.end_stroke(final_input, active_layer, cx)
        });

        let overriders = cx.global_mut::<LayerPreviewOverriders>();
        overriders.remove_overrider(&active_layer);

        if let Some(dirty_pixels) = dirty_pixels {
            let dirty_tiles = GpuTileStorageInner::pixel_rect_to_tile(dirty_pixels);
            canvas_entity.update(cx, |canvas, cx| {
                cx.emit(CanvasUpdated { dirty_tiles });
            });
        }
    }
}

wrapper! {
    pub mut CurrentBrushPresetOperator : Option<BrushPresetOperator>
}

impl Global for CurrentBrushPresetOperator {}
