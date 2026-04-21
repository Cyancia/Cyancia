use std::collections::VecDeque;

use bevy_math::IRect;
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
use futures::channel::oneshot;
use glam::{FloatExt, Vec2};
use iced_runtime::Task;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use wgpu::{Buffer, BufferAsyncError};

use crate::{
    input_processing::RawPenInput,
    instance::BrushPresetInstance,
    render::{BrushPresetOperator, StrokeInfo},
};

#[derive(Default)]
pub struct BrushTool {
    target_layer: Option<(CanvasId, LayerId)>,
}

pub enum BrushToolMessage {
    StrokeInfoReadback(StrokeInfo),
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
        let params = RawPenInput { position };
        self.target_layer = Some((canvas.id(), active_layer));

        let success =
            services.try_service_scope::<CurrentBrushPresetOperator, ()>(|brush, services| {
                let tiles = services.service::<GpuTileStorage>();
                let assets = services.service::<AssetRegistry>();
                let target_layer_info = tiles.get_layer_info(active_layer).unwrap();
                let target_layer_binding = tiles.get_layer_binding_or_empty(active_layer).unwrap();
                brush.begin_stroke(params, &assets, target_layer_binding, target_layer_info);
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
        let params = RawPenInput { position };

        let Some(brush) = services.get_service_mut::<CurrentBrushPresetOperator>() else {
            log::error!("No current brush preset operator found.");
            return Task::none();
        };

        let now = std::time::Instant::now();
        brush.update_stroke(params);
        log::info!("Brush update took: {:?}", now.elapsed());

        let (tx, rx) = oneshot::channel();
        if let Some(staging) = brush.map_stroke_info_async(tx) {
            Task::future(stroke_info_readback(canvas_id, rx, staging))
                .map(BrushToolMessage::StrokeInfoReadback)
        } else {
            Task::none()
        }
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
        let final_input = RawPenInput { position };

        let success =
            services.try_service_scope::<CurrentBrushPresetOperator, ()>(|brush, services| {
                let tiles = services.service::<GpuTileStorage>();
                let stroke_info = brush.end_stroke(final_input, &tiles, active_layer);
                CanvasUpdate::broadcast(CanvasUpdate {
                    id: canvas_id,
                    dirty_tiles: IRect {
                        min: stroke_info.accumulated_bound_min,
                        max: stroke_info.accumulated_bound_max,
                    },
                });
            });

        let overriders = services.service_mut::<LayerPreviewOverriders>();
        overriders.remove_overrider(&active_layer);

        if success.is_none() {
            log::error!("No current brush preset operator found.");
        }

        Task::none()
    }

    fn handle_message(
        &mut self,
        message: Self::Message,
        services: &mut Services,
    ) -> Task<Self::Message> {
        match message {
            BrushToolMessage::StrokeInfoReadback(stroke_info) => {
                let Some((canvas_id, active_layer)) = self.target_layer else {
                    return Task::none();
                };

                let brush = services.service::<CurrentBrushPresetOperator>();
                let Some(buffer) = brush.stroke_buffer() else {
                    return Task::none();
                };
                // TODO Do stroke post process. This is not actually what outputs.
                let overrider = PixelPreviewOverrider {
                    texture: buffer.textures()[1 - stroke_info.total_dabs as usize % 2].clone(),
                    tile_info_buffer: buffer.tile_info_buffer().clone(),
                };

                let overriders = services.service_mut::<LayerPreviewOverriders>();
                overriders.insert_overrider(active_layer, overrider);
                CanvasUpdate::broadcast(CanvasUpdate {
                    id: canvas_id,
                    dirty_tiles: IRect {
                        min: stroke_info.accumulated_bound_min,
                        max: stroke_info.accumulated_bound_max,
                    },
                });
            }
        }

        Task::none()
    }
}

wrapper! {
    pub mut CurrentBrushPresetOperator : BrushPresetOperator
}

impl Service for CurrentBrushPresetOperator {}

async fn stroke_info_readback(
    id: CanvasId,
    rx: oneshot::Receiver<Result<(), BufferAsyncError>>,
    staging_buffer: Buffer,
) -> StrokeInfo {
    rx.await.unwrap().unwrap();
    let stroke_info_data = staging_buffer.slice(..).get_mapped_range();
    let storage = encase::StorageBuffer::new(stroke_info_data.as_ref());
    storage.create::<StrokeInfo>().unwrap()
}
