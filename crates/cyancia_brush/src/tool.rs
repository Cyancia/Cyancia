use std::collections::VecDeque;

use bevy_math::IRect;
use chrono::{DateTime, Utc};
use cyancia_assets::store::AssetRegistry;
use cyancia_canvas::{CCanvas, CanvasId, CanvasManager, event::CanvasUpdate};
use cyancia_image::{
    composite::{LayerPreviewOverriders, PixelPreviewOverrider},
    layer::LayerId,
    tile::GpuTileStorage,
};
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_math::number::LerpAngle;
use cyancia_runtime::{Services, event::Event, service::Service};
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::wrapper;
use glam::{FloatExt, Vec2};
use iced_runtime::Task;
use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::{
    input_processing::RawPenInput,
    instance::BrushPresetInstance,
    render::{BrushPresetOperator, Time},
};

const TIMESTAMP_MOD: i64 = 1_000_000;

#[derive(Default)]
pub struct BrushTool {
    target_layer: Option<(CanvasId, LayerId)>,
    stroke_begin: Option<DateTime<Utc>>,
}

pub enum BrushToolMessage {
    CanvasUpdateDuringStroke(CanvasId, IRect),
}

impl ToolFunction for BrushTool {
    type Message = BrushToolMessage;

    fn id() -> ToolId {
        ToolId::new("brush_tool")
    }

    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return Task::none();
        };

        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return Task::none();
        };
        let active_layer = canvas.image.active_layer;
        if !canvas.image.active_layer_data().can_contain_pixels() {
            log::warn!("Unable to paint to the active layer which cannot contain pixels.");
            return Task::none();
        }

        let now = Utc::now();
        self.stroke_begin = Some(now);
        self.target_layer = Some((canvas.id(), active_layer));

        let params = RawPenInput {
            position,
            time: Time {
                now: (now.timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (now.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        let success =
            services.try_service_scope::<CurrentBrushPresetOperator, ()>(|brush, services| {
                let tiles = services.service::<GpuTileStorage>();
                let assets = services.service::<AssetRegistry>();
                brush.begin_stroke(params, tiles, assets, active_layer);
            });

        if success.is_none() {
            log::error!("No current brush preset operator found.");
        }

        Task::none()
    }

    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some((canvas_id, active_layer)) = self.target_layer else {
            return Task::none();
        };

        let Some(canvas) = services.service::<CanvasManager>().get(&canvas_id) else {
            return Task::none();
        };
        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return Task::none();
        };

        let Some(stroke_begin) = self.stroke_begin else {
            log::error!("Stroke update called without a stroke begin time.");
            return Task::none();
        };

        let params = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (stroke_begin.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        let Some(brush) = services.get_service_mut::<CurrentBrushPresetOperator>() else {
            log::error!("No current brush preset operator found.");
            return Task::none();
        };

        let now = std::time::Instant::now();
        // update_stroke needs GpuTileStorage — split borrow via a workaround:
        // We take the tiles reference separately. Since services holds both, we call
        // update_stroke through try_service_scope instead.
        // NOTE: We drop brush borrow here and re-scope below.
        drop(brush);

        let maybe_preview = services
            .try_service_scope::<CurrentBrushPresetOperator, _>(|brush, services| {
                let tiles = services.service::<GpuTileStorage>();
                brush.update_stroke(params, tiles);
                brush.generate_preview(tiles)
            })
            .flatten();

        if let Some((bounds, preview)) = maybe_preview {
            let overriders = services.service_mut::<LayerPreviewOverriders>();
            overriders.insert_overrider(
                active_layer,
                PixelPreviewOverrider {
                    texture: preview.texture().unwrap().as_ref().clone(),
                    tile_info_buffer: preview.tile_info_buffer().unwrap().clone(),
                },
            );

            CanvasUpdate::broadcast(CanvasUpdate {
                id: canvas_id,
                dirty_tiles: bounds,
            });
        }

        Task::none()
    }

    fn end(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some((canvas_id, active_layer)) = self.target_layer.take() else {
            return Task::none();
        };

        let Some(canvas) = services.service::<CanvasManager>().get(&canvas_id) else {
            return Task::none();
        };
        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return Task::none();
        };

        let Some(stroke_begin) = self.stroke_begin.take() else {
            log::error!("Stroke end called without a stroke begin time.");
            return Task::none();
        };

        let final_input = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (stroke_begin.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        let dirty_tiles = services
            .try_service_scope::<CurrentBrushPresetOperator, _>(|brush, services| {
                let tiles = services.service::<GpuTileStorage>();
                brush.end_stroke(final_input, tiles, active_layer)
            })
            .flatten();

        let overriders = services.service_mut::<LayerPreviewOverriders>();
        overriders.remove_overrider(&active_layer);

        if let Some(dirty_tiles) = dirty_tiles {
            CanvasUpdate::broadcast(CanvasUpdate {
                id: canvas_id,
                dirty_tiles,
            });
        }

        Task::none()
    }
}

wrapper! {
    pub mut CurrentBrushPresetOperator : BrushPresetOperator
}

impl Service for CurrentBrushPresetOperator {}
