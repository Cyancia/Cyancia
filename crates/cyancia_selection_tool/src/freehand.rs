use bevy_math::Rect;
use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt};
use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::log_err::LogErr;
use glam::{IVec2, Vec2};
use gpui::{App, Context, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
use tracing::info;
use wgpu::TextureView;

use crate::render::{
    SelectionOperation, SelectionPipeline, SelectionPreviewPipeline, generate_cmd,
};

struct FreehandSelectionState {
    aabb: Rect,
    points_ps: Vec<Vec2>,
}

fn indices_from_looped_vertices(vertices: u32) -> Vec<u32> {
    let mut indices = Vec::with_capacity(vertices as usize - 2);
    for i in 1..vertices {
        indices.push(0);
        indices.push(i - 1);
        indices.push(i);
    }
    indices
}

#[derive(Default)]
pub struct FreehandSelectionTool {
    state: Option<FreehandSelectionState>,
    preview_pipeline: Option<SelectionPreviewPipeline>,
}

impl ToolFunction for FreehandSelectionTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("freehand_selection_tool")
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let Some(point_ps) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };

        self.state = Some(FreehandSelectionState {
            aabb: Rect {
                min: point_ps,
                max: point_ps,
            },
            points_ps: vec![point_ps],
        });
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let Some(point_ps) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };

        let Some(state) = self.state.as_mut() else {
            return;
        };

        state.points_ps.push(point_ps);
        state.aabb = state.aabb.union_point(point_ps);
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(state) = self.state.take() else {
            return;
        };

        if state.points_ps.len() < 3 {
            return;
        }

        let cmd = generate_cmd(
            "Freehand Selection".into(),
            &state.points_ps,
            &indices_from_looped_vertices(state.points_ps.len() as u32),
            state.aabb.as_irect(),
            cx,
            mouse.modifiers,
        );

        if let Some(cmd) = cmd {
            cx.push_undo_command_to_current(cmd).log_err();
            info!(
                "Freehand select {} points aabb {:?}",
                state.points_ps.len(),
                state.aabb
            );
        }
    }

    fn canvas_overlay(&mut self, canvas_surface: &TextureView, cx: &mut App) {
        let Some(state) = &self.state else {
            return;
        };

        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let render_context = cx.global::<RenderContext>();
        let preview_pipeline = self.preview_pipeline.get_or_insert_with(|| {
            SelectionPreviewPipeline::new(&render_context.device, canvas_surface.texture().format())
        });

        preview_pipeline.draw(
            &render_context.device,
            &render_context.queue,
            &state.points_ps,
            canvas_surface,
            &canvas.transform,
        );
    }
}
