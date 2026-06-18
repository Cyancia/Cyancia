use bevy_math::IRect;
use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt, command::TileReplaceCommand};
use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use cyancia_utils::log_err::LogErr;
use glam::{IVec2, Vec2};
use gpui::{App, Context, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
use tracing::info;
use wgpu::{Texture, TextureView};

use crate::render::{SelectionOperation, SelectionPipeline, SelectionPreviewPipeline};

struct ShapeSelectionState {
    start_ps: IVec2,
    cur_end_ps: IVec2,
}

#[derive(Default)]
pub struct RectangularSelectionTool {
    state: Option<ShapeSelectionState>,
    preview_pipeline: Option<SelectionPreviewPipeline>,
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
            cur_end_ps: start_pos.as_ivec2(),
        });
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let Some(end_pos) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };

        state.cur_end_ps = end_pos.as_ivec2() + 1;
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

        let selection_layer = tiles.get_layer(selection_layer_id).unwrap();
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
            SelectionOperation::from_modifiers(mouse.modifiers),
            selection_layer_binding,
            selection_layer.iter_tiles().map(|(i, _, _)| i).collect(),
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

    fn canvas_overlay(&mut self, canvas_surface: &TextureView, cx: &mut App) {
        let Some(state) = &self.state else {
            return;
        };

        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let v0 = state.start_ps.as_vec2();
        let v1 = state.cur_end_ps.as_vec2();
        let v2 = Vec2::new(v0.x, v1.y);
        let v3 = Vec2::new(v1.x, v0.y);

        let render_context = cx.global::<RenderContext>();
        let preview_pipeline = self.preview_pipeline.get_or_insert_with(|| {
            SelectionPreviewPipeline::new(&render_context.device, canvas_surface.texture().format())
        });

        preview_pipeline.draw(
            &render_context.device,
            &render_context.queue,
            &[v0, v2, v1, v3, v0],
            canvas_surface,
            &canvas.transform,
        );
    }
}
