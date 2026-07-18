use std::rc::Rc;

use chrono::{DateTime, Utc};
use cyancia_canvas::{CCanvas, CanvasAppExt, CanvasUndoStackAppExt};
use cyancia_shader_graph::graph::slot::GraphInlineLiteralRenderContext;
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::wrapper;
use glam::Vec2;
use gpui::{
    AnyElement, BorrowAppContext, Context, Global, IntoElement, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Styled, WeakEntity, Window,
};
use gpui_component::{scroll::ScrollableElement, v_flex};

use crate::{
    input_processing::RawPenInput,
    render::{BrushPresetOperator, Time},
};

const TIMESTAMP_MOD: i64 = 1_000_000;

struct BrushToolState {
    canvas_entity: WeakEntity<CCanvas>,
    stroke_begin: DateTime<Utc>,
}

#[derive(Default)]
pub struct BrushTool {
    state: Option<BrushToolState>,
}

impl ToolFunction for BrushTool {
    fn new(_: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("brush_tool".into())
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
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
        let active_layer = canvas.active_layer_id();
        let selection_layer = canvas.image.selection_layer();
        if !canvas.active_layer_node().instance().can_contain_pixels() {
            log::warn!("Unable to paint to the active layer which cannot contain pixels.");
            return;
        }

        let now = Utc::now();
        self.state = Some(BrushToolState {
            canvas_entity: canvas_entity.clone(),
            stroke_begin: now,
        });

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

            let Ok(queued_cmd) = cx.queue_undo_command_to_current() else {
                return;
            };
            brush.begin_stroke(
                params,
                active_layer,
                selection_layer,
                canvas_entity,
                queued_cmd,
                cx,
            );
        });
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(BrushToolState {
            canvas_entity,
            stroke_begin,
        }) = &self.state
        else {
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

        let params = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (stroke_begin.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        cx.update_global::<CurrentBrushPresetOperator, _>(|brush, _cx| {
            let Some(brush) = brush.as_mut() else {
                return;
            };
            let now = std::time::Instant::now();
            brush.update_stroke(params);
            log::debug!("Brush stroke update took {:?}", now.elapsed());
        });
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(BrushToolState {
            canvas_entity,
            stroke_begin,
        }) = self.state.take()
        else {
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

        let final_input = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (stroke_begin.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        cx.update_global::<CurrentBrushPresetOperator, _>(|brush, _cx| {
            if let Some(brush) = brush.as_mut() {
                brush.end_stroke(final_input);
            }
        });
    }

    fn tool_option_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        cx.update_global::<CurrentBrushPresetOperator, _>(|brush, cx| {
            let Some(brush) = brush.as_ref() else {
                return "No brush selected".into_any_element();
            };

            let ext_vars = brush.instance().iter_external_vars().map(|(id, var)| {
                v_flex()
                    .gap_1()
                    .child(var.name.clone())
                    .child(var.value.ty().render_inline(
                        var.value.value(),
                        GraphInlineLiteralRenderContext {
                            slot_id: (*id).into(),
                            window,
                            cx,
                            on_update: Rc::new(move |value, cx| {
                                let Some(op) =
                                    cx.global_mut::<CurrentBrushPresetOperator>().as_mut()
                                else {
                                    return;
                                };
                                op.instance_mut().update_external_var(&id, value);
                            }),
                        },
                    ))
            });

            v_flex()
                .p_2()
                .size_full()
                .overflow_y_scrollbar()
                .gap_2()
                .child("Variables")
                .child(v_flex().gap_1().children(ext_vars))
                .into_any_element()
        })
    }
}

wrapper! {
    pub mut CurrentBrushPresetOperator : Option<BrushPresetOperator>
}

impl Global for CurrentBrushPresetOperator {}
