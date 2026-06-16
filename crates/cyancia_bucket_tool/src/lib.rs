use bevy_math::IRect;
use cyancia_canvas::{CanvasAppExt, event::CanvasUpdated};
use cyancia_image::tile::{GpuTileStorage, GpuTileStorageInner};
use cyancia_render::render_context::RenderContext;
use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use glam::{IVec2, Vec2, Vec4};
use gpui::{
    AnyElement, App, AppContext, Context, IntoElement, MouseUpEvent, ParentElement, Styled, Window,
    prelude::FluentBuilder,
};
use gpui_component::{
    Selectable, Sizable,
    button::{Button, ButtonGroup},
    form::{field, v_form},
    input::{InputEvent, InputState, MaskPattern, NumberInput, NumberInputEvent, StepAction},
    v_flex,
};

use crate::bucket::{Bucket, BucketAntialiasApproach, BucketParams};

pub mod bucket;

pub fn init(cx: &mut App) {
    cx.add_tool_function::<BucketTool>();
}

pub struct BucketTool {
    threshold: f32,
    alpha_threshold: f32,
    grow: i32,
    close_gap: u32,
    cached_feather: u32,
    aa_approach: BucketAntialiasApproach,
}

impl Default for BucketTool {
    fn default() -> Self {
        Self {
            threshold: 0.08,
            alpha_threshold: 0.02,
            grow: 0,
            close_gap: 0,
            cached_feather: 0,
            aa_approach: BucketAntialiasApproach::Fxaa,
        }
    }
}

impl ToolFunction for BucketTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("bucket_tool")
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(canvas_entity) = cx.current_canvas().and_then(|e| e.upgrade()) else {
            return;
        };
        let canvas = canvas_entity.read(cx);

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
        // TODO Reference other layers
        let ref_layer_id = canvas.image.active_layer;
        let ref_layer_info = tiles.get_layer_tiles(ref_layer_id).unwrap();
        let ref_layer_info_buffer = tiles.get_layer_info(ref_layer_id).unwrap();
        let ref_layer = tiles.get_layer_binding_or_empty(ref_layer_id).unwrap();

        let output_layer_id = canvas.image.active_layer;
        let mut output_layer = tiles.get_layer_mut(output_layer_id).unwrap();

        let image_size = canvas.image.size();
        let params = BucketParams {
            seed: position_ps.as_uvec2(),
            // TODO Connect this to foreground color.
            fill_color: Vec4::new(0.5, 0.5, 0.0, 1.0),
            threshold: self.threshold,
            alpha_threshold: self.alpha_threshold,
            close_gap: self.close_gap,
            grow: self.grow,
            aa_approach: match self.aa_approach {
                BucketAntialiasApproach::Feather(_) => {
                    BucketAntialiasApproach::Feather(self.cached_feather)
                }
                _ => self.aa_approach,
            },
            image_size,
        };

        let bucket = Bucket::new(
            &render_context.device,
            ref_layer_info_buffer.texel_type,
            output_layer.layer_info().texel_type,
        );
        let dirty_tiles = bucket.dispatch(
            &render_context.device,
            &render_context.queue,
            &params,
            &ref_layer,
            ref_layer_info.into_iter().collect(),
            &mut output_layer,
        );
        drop(output_layer);

        if !dirty_tiles.is_empty() {
            let mut min = IVec2::MAX;
            let mut max = IVec2::MIN;
            for dirty_tile in dirty_tiles {
                min = min.min(dirty_tile);
                max = max.max(dirty_tile);
            }
            let dirty_tile_rect = IRect { min, max: max + 1 };

            canvas_entity.update(cx, |_, cx| {
                cx.emit(CanvasUpdated {
                    dirty_tiles: dirty_tile_rect,
                });
            });
        }
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

        let grow_state = window.use_keyed_state(
            format!("{}-{}", *Self::id(), "grow-input"),
            cx,
            |window, cx| {
                let state = cx.new(|cx| {
                    InputState::new(window, cx)
                        .mask_pattern(MaskPattern::Number {
                            separator: None,
                            fraction: Some(2),
                        })
                        .default_value(self.grow.to_string())
                });

                cx.subscribe_in(&state, window, {
                    let entity = bucket_entity.clone();

                    move |_, state, event: &InputEvent, window, cx| match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            entity
                                .update(cx, |bucket, cx| {
                                    state.update(cx, |state, cx| {
                                        let value =
                                            state.value().parse::<i32>().unwrap_or(bucket.grow);
                                        bucket.grow = value.clamp(-64, 64);
                                        state.set_value(bucket.grow.to_string(), window, cx);
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
                            NumberInputEvent::Step(StepAction::Increment) => 1,
                            NumberInputEvent::Step(StepAction::Decrement) => -1,
                        };
                        entity
                            .update(cx, |bucket, cx| {
                                state.update(cx, |state, cx| {
                                    bucket.grow = (bucket.grow + step).clamp(-64, 64);
                                    state.set_value(bucket.grow.to_string(), window, cx);
                                });
                            })
                            .ok();
                    }
                })
                .detach();

                state
            },
        );

        let close_gap_state = window.use_keyed_state(
            format!("{}-{}", *Self::id(), "close-gap-input"),
            cx,
            |window, cx| {
                let state = cx.new(|cx| {
                    InputState::new(window, cx)
                        .mask_pattern(MaskPattern::Number {
                            separator: None,
                            fraction: Some(2),
                        })
                        .default_value(self.close_gap.to_string())
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
                                            .parse::<u32>()
                                            .unwrap_or(bucket.close_gap);
                                        bucket.close_gap = value.clamp(0, 64);
                                        state.set_value(bucket.close_gap.to_string(), window, cx);
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
                            NumberInputEvent::Step(StepAction::Increment) => 1,
                            NumberInputEvent::Step(StepAction::Decrement) => -1,
                        };
                        entity
                            .update(cx, |bucket, cx| {
                                state.update(cx, |state, cx| {
                                    bucket.close_gap = bucket
                                        .close_gap
                                        .checked_add_signed(step)
                                        .unwrap_or(bucket.close_gap)
                                        .clamp(0, 64);
                                    state.set_value(bucket.close_gap.to_string(), window, cx);
                                });
                            })
                            .ok();
                    }
                })
                .detach();

                state
            },
        );

        let aa_approach_buttons = ButtonGroup::new("aa-approach-buttons")
            .child(
                Button::new("none")
                    .selected(matches!(self.aa_approach, BucketAntialiasApproach::None))
                    .label("None")
                    .on_click(cx.listener(|bucket, _, window, cx| {
                        bucket.aa_approach = BucketAntialiasApproach::None;
                    })),
            )
            .child(
                Button::new("fxaa")
                    .selected(matches!(self.aa_approach, BucketAntialiasApproach::Fxaa))
                    .label("FXAA")
                    .on_click(cx.listener(|bucket, _, window, cx| {
                        bucket.aa_approach = BucketAntialiasApproach::Fxaa;
                    })),
            )
            .child(
                Button::new("feather")
                    .selected(matches!(
                        self.aa_approach,
                        BucketAntialiasApproach::Feather(_)
                    ))
                    .label("Feather")
                    .on_click(cx.listener(|bucket, _, window, cx| {
                        bucket.aa_approach =
                            BucketAntialiasApproach::Feather(bucket.cached_feather);
                    })),
            );

        let feather_state = window.use_keyed_state(
            format!("{}-{}", *Self::id(), "feather-input"),
            cx,
            |window, cx| {
                let state = cx.new(|cx| {
                    InputState::new(window, cx)
                        .mask_pattern(MaskPattern::Number {
                            separator: None,
                            fraction: Some(2),
                        })
                        .default_value(self.cached_feather.to_string())
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
                                            .parse::<u32>()
                                            .unwrap_or(bucket.cached_feather);
                                        bucket.cached_feather = value.clamp(0, 64);
                                        state.set_value(
                                            bucket.cached_feather.to_string(),
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
                            NumberInputEvent::Step(StepAction::Increment) => 1,
                            NumberInputEvent::Step(StepAction::Decrement) => -1,
                        };
                        entity
                            .update(cx, |bucket, cx| {
                                state.update(cx, |state, cx| {
                                    bucket.cached_feather =
                                        (bucket.cached_feather + step as u32).clamp(0, 64);
                                    state.set_value(bucket.cached_feather.to_string(), window, cx);
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
        let grow_state = grow_state.read(cx);
        let close_gap_state = close_gap_state.read(cx);
        let feather_state = feather_state.read(cx);

        v_flex()
            .size_full()
            .p_2()
            .child(
                v_form()
                    .size_full()
                    .text_sm()
                    .small()
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
                    .child(
                        field()
                            .label("Grow")
                            .child(NumberInput::new(grow_state).small()),
                    )
                    .child(
                        field()
                            .label("Close Gap")
                            .child(NumberInput::new(close_gap_state).small()),
                    )
                    .child(
                        field()
                            .label("Antialiasing Approach")
                            .child(aa_approach_buttons),
                    )
                    .when(
                        matches!(self.aa_approach, BucketAntialiasApproach::Feather(_)),
                        |f| {
                            f.child(
                                field()
                                    .label("Feather")
                                    .child(NumberInput::new(feather_state).small()),
                            )
                        },
                    ),
            )
            .into_any_element()
    }
}
