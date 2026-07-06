use std::f32::consts::{PI, TAU};

use bevy_math::IRect;
use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::log_err::LogErr;
use glam::{IVec2, Vec2};
use gpui::{App, Context, FillRule, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window};
use tracing::info;
use wgpu::TextureView;

use crate::render::{
    SelectionOperation, SelectionPreviewPipeline, generate_cmd, indices_from_vertices,
};

fn common_begin(
    state: &mut Option<ShapeSelectionState>,
    mouse: &MouseDownEvent,
    cx: &mut Context<impl ToolFunction>,
) {
    let Some(canvas) = cx.read_current_canvas() else {
        return;
    };

    let Some(start_pos) = canvas
        .transform
        .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
    else {
        return;
    };

    *state = Some(ShapeSelectionState {
        start_ps: start_pos.as_ivec2(),
        cur_end_ps: start_pos.as_ivec2(),
        op: SelectionOperation::from_modifiers(mouse.modifiers),
    });
}

fn common_update(
    state: &mut Option<ShapeSelectionState>,
    mouse: &MouseMoveEvent,
    cx: &mut Context<impl ToolFunction>,
) {
    let Some(state) = state.as_mut() else {
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
    let end_pos = end_pos.as_ivec2();

    let cur_end_pos = if mouse.modifiers.shift {
        // Square
        let length = (end_pos - state.start_ps).abs().min_element();
        let dir = (end_pos - state.start_ps).signum();
        state.start_ps + dir * length
    } else {
        end_pos
    };
    state.cur_end_ps = cur_end_pos + 1;
}
struct ShapeSelectionState {
    start_ps: IVec2,
    cur_end_ps: IVec2,
    op: SelectionOperation,
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
        ToolId::new("rectangular_selection_tool".into())
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        common_begin(&mut self.state, mouse, cx);
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        common_update(&mut self.state, mouse, cx);
    }

    fn end(&mut self, _: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(state) = self.state.take() else {
            return;
        };

        let selection_pixels = IRect {
            min: state.start_ps.min(state.cur_end_ps),
            max: state.start_ps.max(state.cur_end_ps),
        };

        let cmd = generate_cmd(
            "Rectangular Selection".into(),
            &[
                selection_pixels.min.as_vec2(),
                Vec2::new(selection_pixels.max.x as f32, selection_pixels.min.y as f32),
                selection_pixels.max.as_vec2(),
                Vec2::new(selection_pixels.min.x as f32, selection_pixels.max.y as f32),
            ],
            &[2, 1, 0, 3, 2, 0],
            selection_pixels,
            state.op,
            cx,
        );

        if let Some(cmd) = cmd {
            cx.push_undo_command_to_current(cmd).log_err();
            info!(
                "Selected rectangle {} to {}",
                selection_pixels.min, selection_pixels.max
            );
        }
    }

    fn canvas_overlay(&mut self, canvas_surface: &TextureView, _: &mut Window, cx: &mut App) {
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

const MIN_SEGMENTS: u32 = 8;
const MAX_SEGMENTS: u32 = 1024;
const PIXEL_SPACING: f32 = 2.0;

fn ellipse_perimeter(a: f32, b: f32) -> f32 {
    let h = ((a - b) / (a + b)).powi(2);
    PI * (a + b) * (1.0 + 3.0 * h / (10.0 + (4.0 - 3.0 * h).sqrt()))
}

fn generate_ellipse(center: Vec2, radii: Vec2) -> Vec<Vec2> {
    let perimeter = ellipse_perimeter(radii.x, radii.y);
    let segments = ((perimeter / PIXEL_SPACING).ceil() as u32).clamp(MIN_SEGMENTS, MAX_SEGMENTS);

    let aa = radii.x * radii.x;
    let bb = radii.y * radii.y;

    let mut points = Vec::with_capacity(segments as usize);
    for t in 0..segments {
        let theta = (t as f32 / segments as f32) * TAU;
        let (sin, cos) = theta.sin_cos();
        let r = radii.x * radii.y / (aa * sin * sin + bb * cos * cos).sqrt();
        points.push(Vec2::new(r * cos, r * sin) + center);
    }
    points
}

#[derive(Default)]
pub struct EllipticalSelectionTool {
    state: Option<ShapeSelectionState>,
    preview_pipeline: Option<SelectionPreviewPipeline>,
}

impl ToolFunction for EllipticalSelectionTool {
    fn new(_: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("elliptical_selection_tool".into())
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        common_begin(&mut self.state, mouse, cx);
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        common_update(&mut self.state, mouse, cx);
    }

    fn end(&mut self, _: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(state) = self.state.take() else {
            return;
        };

        let selection_pixels = IRect {
            min: state.start_ps.min(state.cur_end_ps),
            max: state.start_ps.max(state.cur_end_ps),
        };
        let selection_pixelsf = selection_pixels.as_rect();
        let vertices = generate_ellipse(selection_pixelsf.center(), selection_pixelsf.size() * 0.5);

        let geometry = indices_from_vertices(&vertices, FillRule::EvenOdd);

        let cmd = generate_cmd(
            "Elliptical Selection".into(),
            &geometry.vertices,
            &geometry.indices,
            selection_pixels,
            state.op,
            cx,
        );

        if let Some(cmd) = cmd {
            cx.push_undo_command_to_current(cmd).log_err();
            info!(
                "Selected rectangle {} to {}",
                selection_pixels.min, selection_pixels.max
            );
        }
    }

    fn canvas_overlay(&mut self, canvas_surface: &TextureView, _: &mut Window, cx: &mut App) {
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

        let selection_pixels = IRect {
            min: state.start_ps.min(state.cur_end_ps),
            max: state.start_ps.max(state.cur_end_ps),
        };
        let selection_pixelsf = selection_pixels.as_rect();
        let mut vertices =
            generate_ellipse(selection_pixelsf.center(), selection_pixelsf.size() * 0.5);
        vertices.push(vertices[0]);

        preview_pipeline.draw(
            &render_context.device,
            &render_context.queue,
            &vertices,
            canvas_surface,
            &canvas.transform,
        );
    }
}
