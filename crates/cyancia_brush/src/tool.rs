use std::{rc::Rc, sync::Arc};

use cyancia_assets::{AssetAppExt, asset::AssetHandle};
use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt};
use cyancia_render::{render_context::RenderContextAppExt, texture::Image};
use cyancia_shader_graph::{
    graph::{
        function::GraphFunctionStorage, slot::GraphInlineLiteralRenderContext,
        texture::GraphTextureStorage,
    },
    save::SerializableGraphFunction,
};
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::wrapper;
use gpui::{
    AnyElement, App, BorrowAppContext, Context, Global, IntoElement, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Styled, Window,
};
use gpui_component::{scroll::ScrollableElement, v_flex};
use log::error;

use crate::{
    asset::BrushPreset,
    editor::{FUNCTION_GRAPH_NODE_REGISTRY, FUNCTION_GRAPH_TYPE_REGISTRY},
    input_processing::{BasicStabilizer, InputProcessor},
    instance::BrushPresetInstance,
    render::CanvasBrushPresetOperator,
};

pub(crate) fn init(cx: &mut App) {
    cx.observe_global::<CurrentBrushPresetHandle>(|cx| {
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

        let op = CanvasBrushPresetOperator::new(
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
    })
    .detach();
}

// TODO We should derive more tools based on the brush tool.
//      For example, eraser tool, airbrush tool etc.
//      They are fundamentally the same tool, but with different default tags.
#[derive(Default)]
pub struct BrushTool;

impl ToolFunction for BrushTool {
    fn new(_cx: &mut Context<Self>) -> Self {
        Self
    }

    fn id() -> ToolId {
        ToolId::new("brush_tool".into())
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(canvas_entity) = cx.current_canvas() else {
            return;
        };

        if !cx.has_global::<CurrentBrushPreset>() {
            return;
        }

        cx.update_global::<CurrentBrushPreset, _>(|brush, cx| {
            let Ok(queued_cmd) = cx.queue_undo_command_to_current() else {
                return;
            };
            brush.begin_stroke(mouse, canvas_entity.upgrade().unwrap(), queued_cmd, cx);
        });
    }

    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !cx.has_global::<CurrentBrushPreset>() {
            return;
        }

        cx.update_global::<CurrentBrushPreset, _>(|brush, cx| {
            let now = std::time::Instant::now();
            brush.update_stroke(mouse, cx);
            log::debug!("Brush stroke update took {:?}", now.elapsed());
        });
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        if !cx.has_global::<CurrentBrushPreset>() {
            return;
        }

        cx.update_global::<CurrentBrushPreset, _>(|brush, cx| {
            brush.end_stroke(mouse, cx);
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
    mut CurrentBrushPreset : CanvasBrushPresetOperator
}

impl Global for CurrentBrushPreset {}

wrapper! {
    #[derive(Clone)]
    pub CurrentBrushPresetHandle : AssetHandle<BrushPreset>
}

impl Global for CurrentBrushPresetHandle {}
