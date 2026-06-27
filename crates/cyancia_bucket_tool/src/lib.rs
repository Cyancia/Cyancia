use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt, command::TileReplaceCommand};
use cyancia_image::tile::TileStorageAppExt;
use cyancia_render::render_context::RenderContextAppExt;
use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use cyancia_utils::log_err::LogErr;
use glam::{Vec2, Vec4};
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

// TODO Blending mode and contiguous
//      Hold shift to disable contiguous
pub struct BucketTool {
    pub threshold: f32,
    pub alpha_threshold: f32,
    pub grow: i32,
    pub close_gap: u32,
    pub cached_feather: u32,
    pub aa_approach: BucketAntialiasApproach,
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
    fn new(_: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("bucket_tool".into())
    }

    fn end(&mut self, mouse: &MouseUpEvent, cx: &mut Context<Self>) {
        let Some(canvas_entity) = cx.current_canvas().and_then(|e| e.upgrade()) else {
            return;
        };
        let canvas = canvas_entity.read(cx);
        let canvas_id = canvas.id();

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

        let tiles = cx.tile_storage();
        // TODO Reference other layers
        let ref_layer_id = canvas.active_layer_id();
        let ref_layer_info = tiles.get_layer_tiles(ref_layer_id).unwrap();
        let ref_layer_info_buffer = tiles.get_layer_info(ref_layer_id).unwrap();
        let ref_layer = tiles.get_layer_binding_or_empty(ref_layer_id).unwrap();

        let output_layer_id = canvas.active_layer_id();
        let output_layer_info = tiles.get_layer_info(output_layer_id).unwrap();
        let output_layer = tiles.get_layer_binding_or_empty(output_layer_id).unwrap();

        let selection_layer = canvas.image.selection_layer();
        let selection_layer = tiles.get_layer_binding_or_empty(selection_layer).unwrap();

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

        let device = cx.render_device();
        let queue = cx.render_queue();

        let bucket = Bucket::new(
            device,
            ref_layer_info_buffer.texel_type,
            output_layer_info.texel_type,
        );
        let result = bucket.dispatch_composite(
            device,
            queue,
            &params,
            &ref_layer,
            ref_layer_info.into_iter().collect(),
            &output_layer,
            &selection_layer,
        );

        if let Some(new_tiles) = result {
            let output_layer = tiles.get_layer(output_layer_id).unwrap();
            let cmd = TileReplaceCommand::new(
                "Bucket Fill".into(),
                canvas_id,
                device,
                queue,
                output_layer_id,
                &output_layer,
                new_tiles.iter_tiles().map(|(i, _, _)| i).collect(),
                new_tiles.texture().unwrap().clone(),
            );
            drop(output_layer);
            cx.push_undo_command_to_current(cmd).log_err();
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
                    .on_click(cx.listener(|bucket, _, _, _| {
                        bucket.aa_approach = BucketAntialiasApproach::None;
                    })),
            )
            .child(
                Button::new("fxaa")
                    .selected(matches!(self.aa_approach, BucketAntialiasApproach::Fxaa))
                    .label("FXAA")
                    .on_click(cx.listener(|bucket, _, _, _| {
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
                    .on_click(cx.listener(|bucket, _, _, _| {
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
