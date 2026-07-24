use std::{rc::Rc, sync::Arc};

use chrono::{DateTime, Utc};
use cyancia_assets::{
    AssetAppExt,
    asset::{AssetHandle, AssetId},
};
use cyancia_canvas::{CCanvas, CanvasAppExt, CanvasUndoStackAppExt};
use cyancia_image::layer::properties::LayerTexelTypeProp;
use cyancia_render::{render_context::RenderContextAppExt, texture::Image};
use cyancia_shader_graph::{
    graph::{
        function::GraphFunctionStorage, slot::GraphInlineLiteralRenderContext,
        texture::GraphTextureStorage,
    },
    save::SerializableGraphFunction,
};
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::{log_err::LogErr, wrapper};
use glam::Vec2;
use gpui::{
    AnyElement, BorrowAppContext, Context, Global, IntoElement, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{scroll::ScrollableElement, v_flex};
use log::error;

use crate::{
    asset::BrushPreset,
    editor::{FUNCTION_GRAPH_NODE_REGISTRY, FUNCTION_GRAPH_TYPE_REGISTRY},
    input_processing::{BasicStabilizer, InputProcessor, RawPenInput},
    instance::BrushPresetInstance,
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
    _subscriptions: Vec<Subscription>,
}

impl BrushTool {
    fn reload_preset(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = cx.try_global::<CurrentBrushPresetHandle>().cloned() else {
            return;
        };

        let function_assets = cx
            .assets()
            .all_handles_of::<SerializableGraphFunction>()
            .unwrap();
        let functions = function_assets
            .iter()
            .map(|handle| {
                let func = handle.get().unwrap();
                // TODO err handling
                (
                    func.id,
                    func.deserialize_func(
                        Some(handle.id()),
                        FUNCTION_GRAPH_TYPE_REGISTRY.clone(),
                        FUNCTION_GRAPH_NODE_REGISTRY.as_ref(),
                        cx,
                    )
                    .0
                    .unwrap(),
                )
            })
            .collect();
        let function_storage = Arc::new(GraphFunctionStorage::new(functions));

        // TODO: Update this storage when asset changes.
        let textures = cx.assets().all_handles_of::<Image>().unwrap();
        let texture_storage = Arc::new(GraphTextureStorage::new(textures));
        let (instance, err) = BrushPresetInstance::from_asset(
            &handle.0,
            texture_storage,
            function_storage,
            Arc::new(Default::default()), // TODO
            cx,
        );

        for err in err {
            error!("{}", err);
        }

        let Some(instance) = instance else {
            return;
        };

        let op = BrushPresetOperator::new(
            instance,
            cx.render_device().clone(),
            cx.render_queue().clone(),
            InputProcessor::new(256, Box::new(BasicStabilizer)),
        );
        log::info!(
            "Loaded brush preset {} {:?}",
            op.instance().metadata().name,
            op.instance().asset_id()
        );

        cx.set_global(CurrentBrushPreset::new(op));
    }
}

impl ToolFunction for BrushTool {
    fn new(cx: &mut Context<Self>) -> Self {
        let _subscriptions =
            vec![cx.observe_global::<CurrentBrushPresetHandle>(Self::reload_preset)];
        Self {
            state: None,
            _subscriptions,
        }
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
        if !canvas
            .active_layer_node()
            .properties()
            .contains::<LayerTexelTypeProp>()
        {
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

        if !cx.has_global::<CurrentBrushPreset>() {
            return;
        }

        cx.update_global::<CurrentBrushPreset, _>(|brush, cx| {
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

        if !cx.has_global::<CurrentBrushPreset>() {
            return;
        }

        cx.update_global::<CurrentBrushPreset, _>(|brush, _cx| {
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

        if !cx.has_global::<CurrentBrushPreset>() {
            return;
        }

        cx.update_global::<CurrentBrushPreset, _>(|brush, _cx| {
            brush.end_stroke(final_input);
        });
    }

    fn tool_option_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if !cx.has_global::<CurrentBrushPreset>() {
            return "No brush selected".into_any_element();
        }

        cx.update_global::<CurrentBrushPreset, _>(|brush, cx| {
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
                                if !cx.has_global::<CurrentBrushPreset>() {
                                    return;
                                }
                                let op = cx.global_mut::<CurrentBrushPreset>();
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

// This needs to share between canvases. So should be a global.
wrapper! {
    mut CurrentBrushPreset : BrushPresetOperator
}

impl Global for CurrentBrushPreset {}

wrapper! {
    #[derive(Clone)]
    pub CurrentBrushPresetHandle : AssetHandle<BrushPreset>
}

impl Global for CurrentBrushPresetHandle {}
