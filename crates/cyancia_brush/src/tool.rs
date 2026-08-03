use std::rc::Rc;

use cyancia_assets::asset::AssetHandle;
use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt};
use cyancia_render::render_context::RenderContextAppExt;
use cyancia_shader_graph::graph::{
    function::ASSET_GRAPH_FUNCTION_STORAGE, slot::GraphInlineLiteralRenderContext,
    texture::ASSET_GRAPH_TEXTURE_STORAGE,
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
    input_processing::{BasicStabilizer, InputProcessor},
    instance::BrushPresetInstance,
    render::CanvasBrushPresetOperator,
};

pub(crate) fn init(cx: &mut App) {
    cx.observe_global::<CurrentBrushPresetHandle>(|cx| {
        let Some(handle) = cx.try_global::<CurrentBrushPresetHandle>().cloned() else {
            cx.remove_global::<CurrentBrushPreset>();
            return;
        };

        let (instance, err) = BrushPresetInstance::from_asset(
            &handle.0,
            ASSET_GRAPH_TEXTURE_STORAGE.clone(),
            ASSET_GRAPH_FUNCTION_STORAGE.clone(),
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

    #[tracing::instrument(skip_all, name = "brush_tool_begin")]
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

    #[tracing::instrument(skip_all, name = "brush_tool_update")]
    fn update(&mut self, mouse: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !cx.has_global::<CurrentBrushPreset>() {
            return;
        }

        cx.update_global::<CurrentBrushPreset, _>(|brush, cx| {
            brush.update_stroke(mouse, cx);
        });
    }

    #[tracing::instrument(skip_all, name = "brush_tool_end")]
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
