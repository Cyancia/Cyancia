use cyancia_canvas::CanvasAppExt;
use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use glam::Vec2;
use gpui::{
    AnyElement, App, AppContext, Context, IntoElement, MouseUpEvent, ParentElement, Styled, Window,
};
use gpui_component::{
    Sizable,
    form::{field, v_form},
    input::{InputEvent, InputState, MaskPattern, NumberInput, NumberInputEvent, StepAction},
    v_flex,
};

use crate::bucket::{Bucket, BucketParams};

pub mod bucket;

pub fn init(cx: &mut App) {
    cx.add_tool_function::<BucketTool>();
}

const _: () = {
    if GpuTileStorageInner::TILE_SIZE % 32 != 0 {
        panic!(
            "Tile size must be divisible by 32, otherwise computations in shaders will be incorrect"
        );
    }
};

#[derive(Default)]
pub struct BucketTool {
    threshold: f32,
    alpha_threshold: f32,
}

impl ToolFunction for BucketTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("bucket_tool")
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };

        let position_ws = Vec2::new(mouse.position.x.into(), mouse.position.y.into());
        let Some(position_ps) = canvas.transform.window_to_pixel(position_ws) else {
            return;
        };
        if position_ps.x < 0.0
            || position_ps.y < 0.0
            || position_ps.x > canvas.image.size().x as f32
            || position_ps.y > canvas.image.size().y as f32
        {
            return;
        }

        let tiles = cx.global::<GpuTileStorage>();
        let render_context = cx.global::<RenderContext>();
        let ref_layer_id = canvas.image.active_layer;
        let ref_layer = tiles.get_layer_binding_or_empty(ref_layer_id).unwrap();
        let ref_layer_info = tiles.get_layer_info(ref_layer_id).unwrap();

        let params = BucketParams {
            seed: position_ps.as_uvec2(),
            threshold: self.threshold,
            alpha_threshold: self.alpha_threshold,
        };

        let bucket = Bucket::new(&render_context.device, ref_layer_info.texel_type);
        let prepared = bucket.prepare(
            &render_context.device,
            &render_context.queue,
            &params,
            &ref_layer,
        );
        bucket.dispatch(&render_context.device, &render_context.queue, prepared);
    }

    fn tool_option_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let bucket_entity = cx.entity().downgrade();
        let threshold_state = window.use_keyed_state(
            format!("{}-{}", *Self::id(), "threshold-input"),
            cx,
            |window, cx| {
                let state = cx.new(|cx| {
                    InputState::new(window, cx)
                        .mask_pattern(MaskPattern::Number {
                            separator: None,
                            fraction: Some(2),
                        })
                        .default_value(format!("{:.4}", self.threshold))
                });

                cx.subscribe_in(&state, window, {
                    let entity = bucket_entity.clone();
                    move |_, state, event: &InputEvent, window, cx| match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            entity
                                .update(cx, |bucket, cx| {
                                    state.update(cx, |state, cx| {
                                        let value = state
                                            .value()
                                            .parse::<f32>()
                                            .unwrap_or(bucket.threshold);
                                        bucket.threshold = value.clamp(0.0, 1.0);
                                        state.set_value(
                                            format!("{:.4}", bucket.threshold),
                                            window,
                                            cx,
                                        );
                                    });
                                })
                                .ok();
                        }
                        InputEvent::Change | InputEvent::Focus => {}
                    }
                })
                .detach();

                cx.subscribe_in(&state, window, {
                    let entity = bucket_entity.clone();
                    move |_, state, event: &NumberInputEvent, window, cx| {
                        let step = match event {
                            NumberInputEvent::Step(StepAction::Increment) => 0.1,
                            NumberInputEvent::Step(StepAction::Decrement) => -0.1,
                        };
                        entity
                            .update(cx, |bucket, cx| {
                                state.update(cx, |state, cx| {
                                    bucket.threshold = (bucket.threshold + step).clamp(0.0, 1.0);
                                    state.set_value(format!("{:.4}", bucket.threshold), window, cx);
                                });
                            })
                            .ok();
                    }
                })
                .detach();

                state
            },
        );

        let alpha_threshold_state = window.use_keyed_state(
            format!("{}-{}", *Self::id(), "alpha-threshold-input"),
            cx,
            |window, cx| {
                let state = cx.new(|cx| {
                    InputState::new(window, cx)
                        .mask_pattern(MaskPattern::Number {
                            separator: None,
                            fraction: Some(2),
                        })
                        .default_value(format!("{:.4}", self.alpha_threshold))
                });

                cx.subscribe_in(&state, window, {
                    let entity = bucket_entity.clone();

                    move |_, state, event: &InputEvent, window, cx| match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            entity
                                .update(cx, |bucket, cx| {
                                    state.update(cx, |state, cx| {
                                        let value = state
                                            .value()
                                            .parse::<f32>()
                                            .unwrap_or(bucket.alpha_threshold);
                                        bucket.alpha_threshold = value.clamp(0.0, 1.0);
                                        state.set_value(
                                            format!("{:.4}", bucket.alpha_threshold),
                                            window,
                                            cx,
                                        );
                                    });
                                })
                                .ok();
                        }
                        InputEvent::Change | InputEvent::Focus => {}
                    }
                })
                .detach();

                cx.subscribe_in(&state, window, {
                    let entity = bucket_entity.clone();
                    move |_, state, event: &NumberInputEvent, window, cx| {
                        let step = match event {
                            NumberInputEvent::Step(StepAction::Increment) => 0.1,
                            NumberInputEvent::Step(StepAction::Decrement) => -0.1,
                        };
                        entity
                            .update(cx, |bucket, cx| {
                                state.update(cx, |state, cx| {
                                    bucket.alpha_threshold =
                                        (bucket.alpha_threshold + step).clamp(0.0, 1.0);
                                    state.set_value(
                                        format!("{:.4}", bucket.alpha_threshold),
                                        window,
                                        cx,
                                    );
                                });
                            })
                            .ok();
                    }
                })
                .detach();

                state
            },
        );

        let threshold_state = threshold_state.read(cx);
        let alpha_threshold_state = alpha_threshold_state.read(cx);

        v_flex()
            .size_full()
            .p_2()
            .child(
                v_form()
                    .size_full()
                    .text_sm()
                    .child(
                        field()
                            .label("Threshold")
                            .child(NumberInput::new(threshold_state).small()),
                    )
                    .child(
                        field()
                            .label("Alpha Threshold")
                            .child(NumberInput::new(alpha_threshold_state).small()),
                    )
                    .small(),
            )
            .into_any_element()
    }
}
