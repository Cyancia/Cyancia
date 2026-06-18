use bevy_math::IRect;
use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt, command::TileReplaceCommand};
use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use cyancia_utils::log_err::LogErr;
use glam::{IVec2, Vec2};
use gpui::{App, Context, MouseDownEvent, MouseUpEvent};
use tracing::info;
use wgpu::Texture;

use crate::render::{SelectionOperation, SelectionPipeline};

struct ShapeSelectionState {
    start_ps: IVec2,
}

#[derive(Default)]
pub struct RectangularSelectionTool {
    state: Option<ShapeSelectionState>,
}

impl ToolFunction for RectangularSelectionTool {
    fn new(_: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("rectangular_selection_tool")
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let Some(start_pos) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };

        self.state = Some(ShapeSelectionState {
            start_ps: start_pos.as_ivec2(),
        });
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };
        let canvas_id = canvas.id();

        let Some(end_pos) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };
        let end_pos = end_pos.as_ivec2() + 1;

        let Some(state) = self.state.take() else {
            return;
        };

        let tiles = cx.global::<GpuTileStorage>();
        let render_context = cx.global::<RenderContext>();
        let selection_layer_id = canvas.image.selection_layer();

        let selection_pixels = IRect {
            min: state.start_ps.min(end_pos),
            max: state.start_ps.max(end_pos),
        };
        let affected_tiles = GpuTileStorageInner::pixel_rect_to_tile(selection_pixels);

        let mut selection_layer = tiles.get_layer_mut(selection_layer_id).unwrap();
        selection_layer.clear();
        let selection_layer_format = selection_layer.layer_info().texel_type;
        let selection_layer_binding = selection_layer
            .binding_data()
            .unwrap_or_else(|| tiles.empty_layer_binding(selection_layer_format));

        let mut pipeline = SelectionPipeline::new(&render_context.device, selection_layer_format);
        let result = pipeline.draw(
            &render_context.device,
            &render_context.queue,
            affected_tiles,
            &[
                selection_pixels.min.as_vec2(),
                Vec2::new(selection_pixels.max.x as f32, selection_pixels.min.y as f32),
                selection_pixels.max.as_vec2(),
                Vec2::new(selection_pixels.min.x as f32, selection_pixels.max.y as f32),
            ],
            &[2, 1, 0, 3, 2, 0],
            SelectionOperation::Or,
            selection_layer_binding,
        );

        if let Some((output_buffer, output_tiles)) = result {
            let cmd = TileReplaceCommand::new(
                "Rectangular Selection".into(),
                canvas_id,
                &render_context.device,
                &render_context.queue,
                selection_layer_id,
                &selection_layer,
                output_tiles,
                output_buffer,
            );
            drop(selection_layer);
            info!(
                "Selected rectangle {} to {}",
                selection_pixels.min, selection_pixels.max
            );

            cx.push_undo_command_to_current(cmd).log_err();
        }
    }
}
