use bevy_math::Rect;
use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt};
use cyancia_render::render_context::RenderContextAppExt;
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::log_err::LogErr;
use glam::Vec2;
use gpui::{
    AnyElement, App, Context, FillRule, IntoElement, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Styled, Window,
};
use gpui_component::{
    Selectable, Sizable,
    button::{Button, ButtonGroup},
    form::{field, v_form},
    v_flex,
};
use tracing::info;
use wgpu::TextureView;

use crate::render::{
    SelectionOperation, SelectionPreviewPipeline, generate_cmd, indices_from_vertices,
};

struct FreehandSelectionState {
    aabb: Rect,
    points_ps: Vec<Vec2>,
    op: SelectionOperation,
}

pub struct FreehandSelectionTool {
    fill_rule: FillRule,
    state: Option<FreehandSelectionState>,
    preview_pipeline: Option<SelectionPreviewPipeline>,
}

impl Default for FreehandSelectionTool {
    fn default() -> Self {
        Self {
            fill_rule: FillRule::EvenOdd,
            state: Default::default(),
            preview_pipeline: Default::default(),
        }
    }
}

impl ToolFunction for FreehandSelectionTool {
    fn new(_: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("freehand_selection_tool".into())
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
            op: SelectionOperation::from_modifiers(mouse.modifiers),
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

    fn end(&mut self, _: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(state) = self.state.take() else {
            return;
        };

        if state.points_ps.len() < 3 {
            return;
        }

        let geometry = indices_from_vertices(&state.points_ps, self.fill_rule);
        let cmd = generate_cmd(
            "Freehand Selection".into(),
            &geometry.vertices,
            &geometry.indices,
            state.aabb.as_irect(),
            state.op,
            cx,
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

    fn tool_option_widget(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .p_2()
            .size_full()
            .child(
                v_form().size_full().small().text_sm().child(
                    field().label("Fill Rule").child(
                        ButtonGroup::new("fill-rule-group")
                            .child(
                                Button::new("even-odd")
                                    .label("Even Odd")
                                    .small()
                                    .selected(self.fill_rule == FillRule::EvenOdd)
                                    .on_click(cx.listener(|tool, _, _, _| {
                                        tool.fill_rule = FillRule::EvenOdd;
                                    })),
                            )
                            .child(
                                Button::new("non-zero")
                                    .label("Non Zero")
                                    .small()
                                    .selected(self.fill_rule == FillRule::NonZero)
                                    .on_click(cx.listener(|tool, _, _, _| {
                                        tool.fill_rule = FillRule::NonZero;
                                    })),
                            ),
                    ),
                ),
            )
            .into_any_element()
    }

    fn canvas_overlay(&mut self, canvas_surface: &TextureView, _: &mut Window, cx: &mut App) {
        let Some(state) = &self.state else {
            return;
        };

        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let device = cx.render_device();
        let queue = cx.render_queue();

        let preview_pipeline = self.preview_pipeline.get_or_insert_with(|| {
            SelectionPreviewPipeline::new(device, canvas_surface.texture().format())
        });

        preview_pipeline.draw(
            device,
            queue,
            &state.points_ps,
            canvas_surface,
            &canvas.transform,
        );
    }
}

const POLYGON_CLOSE_DISTANCE: f32 = 10.0;

pub struct PolygonSelectionTool {
    fill_rule: FillRule,
    state: Option<FreehandSelectionState>,
    preview_pipeline: Option<SelectionPreviewPipeline>,
}

impl Default for PolygonSelectionTool {
    fn default() -> Self {
        Self {
            fill_rule: FillRule::EvenOdd,
            state: Default::default(),
            preview_pipeline: Default::default(),
        }
    }
}

impl ToolFunction for PolygonSelectionTool {
    fn new(_: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("polygon_selection_tool".into())
    }

    fn begin(&mut self, mouse: &MouseDownEvent, _: &mut Context<Self>) {
        if self.state.is_none() {
            self.state = Some(FreehandSelectionState {
                aabb: Rect::EMPTY,
                points_ps: Vec::new(),
                op: SelectionOperation::from_modifiers(mouse.modifiers),
            });
        }
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let point_ss = Vec2::new(mouse.position.x.into(), mouse.position.y.into());
        let Some(point_ps) = canvas.transform.window_to_pixel(point_ss) else {
            return;
        };
        let point_ws = point_ss - canvas.transform.widget_bounds.min;

        let Some(state) = self.state.as_mut() else {
            return;
        };

        if state.points_ps.len() >= 3 {
            let first_ws = canvas
                .transform
                .pixel_to_widget
                .transform_point2(state.points_ps[0]);

            if first_ws.distance_squared(point_ws) < POLYGON_CLOSE_DISTANCE * POLYGON_CLOSE_DISTANCE
            {
                let geometry = indices_from_vertices(&state.points_ps, self.fill_rule);
                let cmd = generate_cmd(
                    "Polygon Selection".into(),
                    &geometry.vertices,
                    &geometry.indices,
                    state.aabb.as_irect(),
                    state.op,
                    cx,
                );

                if let Some(cmd) = cmd {
                    cx.push_undo_command_to_current(cmd).log_err();
                    info!(
                        "Polygon select {} points aabb {:?}",
                        state.points_ps.len(),
                        state.aabb
                    );
                }

                self.state = None;

                return;
            }
        }

        state.points_ps.push(point_ps);
        state.aabb = state.aabb.union_point(point_ps);
    }

    fn tool_option_widget(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .p_2()
            .size_full()
            .child(
                v_form().size_full().small().text_sm().child(
                    field().label("Fill Rule").child(
                        ButtonGroup::new("fill-rule-group")
                            .child(
                                Button::new("even-odd")
                                    .label("Even Odd")
                                    .small()
                                    .selected(self.fill_rule == FillRule::EvenOdd)
                                    .on_click(cx.listener(|tool, _, _, _| {
                                        tool.fill_rule = FillRule::EvenOdd;
                                    })),
                            )
                            .child(
                                Button::new("non-zero")
                                    .label("Non Zero")
                                    .small()
                                    .selected(self.fill_rule == FillRule::NonZero)
                                    .on_click(cx.listener(|tool, _, _, _| {
                                        tool.fill_rule = FillRule::NonZero;
                                    })),
                            ),
                    ),
                ),
            )
            .into_any_element()
    }

    fn canvas_overlay(&mut self, canvas_surface: &TextureView, window: &mut Window, cx: &mut App) {
        let Some(state) = &self.state else {
            return;
        };

        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let mouse = window.mouse_position();
        let point_ss = Vec2::new(mouse.x.into(), mouse.y.into());
        let Some(point_ps) = canvas.transform.window_to_pixel(point_ss) else {
            return;
        };

        let device = cx.render_device();
        let queue = cx.render_queue();

        let preview_pipeline = self.preview_pipeline.get_or_insert_with(|| {
            SelectionPreviewPipeline::new(device, canvas_surface.texture().format())
        });
        let line_vertices_ps = state
            .points_ps
            .iter()
            .copied()
            .chain([point_ps])
            .collect::<Vec<_>>();

        preview_pipeline.draw(
            device,
            queue,
            &line_vertices_ps,
            canvas_surface,
            &canvas.transform,
        );
    }
}
