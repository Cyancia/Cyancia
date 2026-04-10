use std::collections::VecDeque;

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
    input_processing::RawPenInput, instance::BrushPresetInstance, render::BrushPresetOperator,
};

#[derive(Default)]
pub struct BrushTool;

impl ToolFunction for BrushTool {
    fn id(&self) -> ToolId {
        ToolId::new("brush_tool".into())
    }

    fn begin(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, services: &Services) {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return;
        };
        let Some(mut brush) = services.get_service_mut::<CurrentBrushPresetOperator>() else {
            log::error!("No current brush preset operator found.");
            return;
        };

        let Some(position) = canvas
            .transform
            .read()
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return;
        };

        let params = RawPenInput { position };

        let tiles = services.service::<GpuTileStorage>();
        let assets = services.service::<AssetRegistry>();
        brush.begin_stroke(params, &tiles, &assets, canvas.image.root().id);
    }

    fn update(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, services: &Services) {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return;
        };
        let Some(mut brush) = services.get_service_mut::<CurrentBrushPresetOperator>() else {
            log::error!("No current brush preset operator found.");
            return;
        };

        let Some(position) = canvas
            .transform
            .read()
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return;
        };
        let params = RawPenInput { position };

        let now = std::time::Instant::now();
        brush.update_stroke(params);
        log::info!("Brush update took: {:?}", now.elapsed());
    }

    fn end(&mut self, keyboard: &KeyboardState, mouse: &PressedMouseState, services: &Services) {
        let Some(canvas) = services.service::<CanvasManager>().current() else {
            return;
        };
        let Some(mut brush) = services.get_service_mut::<CurrentBrushPresetOperator>() else {
            log::error!("No current brush preset operator found.");
            return;
        };

        let Some(position) = canvas
            .transform
            .read()
            .window_to_pixel(Vec2::new(mouse.position.x, mouse.position.y))
        else {
            return;
        };
        let tiles = services.service::<GpuTileStorage>();
        let final_input = RawPenInput { position };
        brush.end_stroke(final_input, &tiles, canvas.image.root().id);
    }
}

wrapper! {
    pub mut CurrentBrushPresetOperator : BrushPresetOperator
}

impl Service for CurrentBrushPresetOperator {}
