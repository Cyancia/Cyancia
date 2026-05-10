use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use cyancia_assets::store::AssetRegistry;
use cyancia_canvas::{CCanvas, CanvasManager};
use cyancia_image::tile::GpuTileStorage;
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_math::number::LerpAngle;
use cyancia_runtime::{Services, service::Service};
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::wrapper;
use glam::{FloatExt, Vec2};
use ringbuffer::{AllocRingBuffer, RingBuffer};

use crate::{
    input_processing::RawPenInput,
    instance::BrushPresetInstance,
    render::{BrushPresetOperator, Time},
};

const TIMESTAMP_MOD: i64 = 1_000_000;

#[derive(Default)]
pub struct BrushTool {
    stroke_begin: Option<DateTime<Utc>>,
}

impl ToolFunction for BrushTool {
    fn id(&self) -> ToolId {
        ToolId::new("brush_tool".into())
    }

    fn begin(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return;
        };

        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return;
        };
        let root_layer = canvas.image.root().id();
        let now = Utc::now();
        self.stroke_begin = Some(now);
        let params = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (now.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        services.try_service_scope::<CurrentBrushPresetOperator>(
            |brush, services| {
                let tiles = services.service::<GpuTileStorage>();
                let assets = services.service::<AssetRegistry>();
                brush.begin_stroke(params, &tiles, &assets, root_layer);
            },
            || {
                log::error!("No current brush preset operator found.");
            },
        );
    }

    fn update(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return;
        };
        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
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

        services.try_service_scope::<CurrentBrushPresetOperator>(
            |brush, services| {
                let tiles = services.service::<GpuTileStorage>();
                let now = std::time::Instant::now();
                brush.update_stroke(params, tiles);
                log::info!("Brush update took: {:?}", now.elapsed());
            },
            || {
                log::error!("No current brush preset operator found.");
            },
        );
    }

    fn end(
        &mut self,
        keyboard: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return;
        };
        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return;
        };
        let Some(stroke_begin) = self.stroke_begin else {
            log::error!("Stroke end called without a stroke begin time.");
            return;
        };

        let root_layer = canvas.image.root().id();
        let final_input = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (stroke_begin.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        services.try_service_scope::<CurrentBrushPresetOperator>(
            |brush, services| {
                let tiles = services.service::<GpuTileStorage>();
                brush.end_stroke(final_input, &tiles, root_layer);
            },
            || {
                log::error!("No current brush preset operator found.");
            },
        );
    }
}

wrapper! {
    pub mut CurrentBrushPresetOperator : BrushPresetOperator
}

impl Service for CurrentBrushPresetOperator {}
