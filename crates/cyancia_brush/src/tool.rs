use std::rc::Rc;

use chrono::{DateTime, Utc};
use cyancia_canvas::{
    CCanvas, CanvasAppExt, CanvasUndoStackAppExt, command::TileReplaceCommand, event::CanvasUpdated,
};
use cyancia_image::{
    composite::{LayerPreviewOverriders, PixelPreviewOverrider},
    layer::LayerId,
    tile::{GpuTileStorage, GpuTileStorageInner},
};
use cyancia_render::render_context::RenderContext;
use cyancia_shader_graph::graph::slot::GraphInlineLiteralRenderContext;
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::{log_err::LogErr, wrapper};
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

#[derive(Default)]
pub struct BrushTool {
    target_layer: Option<(WeakEntity<CCanvas>, LayerId)>,
    stroke_begin: Option<DateTime<Utc>>,
}

impl ToolFunction for BrushTool {
    fn new(_: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("brush_tool")
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
        let active_layer = canvas.image.active_layer;
        if !canvas.image.active_layer_data().can_contain_pixels() {
            log::warn!("Unable to paint to the active layer which cannot contain pixels.");
            return;
        }

        let now = Utc::now();
        self.stroke_begin = Some(now);
        self.target_layer = Some((canvas_entity, active_layer));

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
            brush.begin_stroke(params, active_layer, cx);
        });
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some((canvas_entity, active_layer)) = &self.target_layer else {
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

        let maybe_preview = cx.update_global::<CurrentBrushPresetOperator, _>(|brush, cx| {
            let brush = brush.as_mut()?;
            let now = std::time::Instant::now();
            brush.update_stroke(params, cx);
            let preview = brush.generate_preview(cx);
            log::debug!("Brush stroke update took {:?}", now.elapsed());

            preview
        });

        if let Some((dirty_pixels, preview)) = maybe_preview {
            let dirty_tiles = GpuTileStorageInner::pixel_rect_to_tile(dirty_pixels);
            let overriders = cx.global_mut::<LayerPreviewOverriders>();
            overriders.insert_overrider(
                *active_layer,
                PixelPreviewOverrider {
                    texture: preview.texture().unwrap().as_ref().clone(),
                    tile_info_buffer: preview.tile_info_buffer().unwrap().clone(),
                },
            );

            canvas_entity.update(cx, |_, cx| {
                cx.emit(CanvasUpdated { dirty_tiles });
            });
        }
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some((canvas_entity, active_layer_id)) = self.target_layer.take() else {
            return;
        };

        let Some(canvas_entity) = canvas_entity.upgrade() else {
            return;
        };
        let canvas = canvas_entity.read(cx);
        let canvas_id = canvas.id();

        let Some(position) = canvas
            .transform
            .window_to_pixel(Vec2::new(mouse.position.x.into(), mouse.position.y.into()))
        else {
            return;
        };

        let Some(stroke_begin) = self.stroke_begin.take() else {
            log::error!("Stroke end called without a stroke begin time.");
            return;
        };

        let final_input = RawPenInput {
            position,
            time: Time {
                now: (Utc::now().timestamp_micros() % TIMESTAMP_MOD) as f32,
                stroke_begin: (stroke_begin.timestamp_micros() % TIMESTAMP_MOD) as f32,
            },
        };

        let result = cx.update_global::<CurrentBrushPresetOperator, _>(|brush, cx| {
            let brush = brush.as_mut()?;
            brush.end_stroke(final_input, cx)
        });

        let overriders = cx.global_mut::<LayerPreviewOverriders>();
        overriders.remove_overrider(&active_layer_id);

        if let Some((new_tiles, new_tile_indices)) = result {
            let render_context = cx.global::<RenderContext>();
            let active_layer = cx
                .global::<GpuTileStorage>()
                .get_layer(active_layer_id)
                .unwrap();
            let cmd = TileReplaceCommand::new(
                "Brush Stroke".into(),
                canvas_id,
                &render_context.device,
                &render_context.queue,
                active_layer_id,
                &active_layer,
                new_tile_indices,
                new_tiles,
            );
            drop(active_layer);
            cx.push_undo_command_to_current(cmd).log_err();
        }
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
