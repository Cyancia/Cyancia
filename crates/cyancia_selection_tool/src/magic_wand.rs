use cyancia_bucket_tool::{
    BucketTool,
    bucket::{Bucket, BucketAntialiasApproach, BucketParams},
};
use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt, command::TileReplaceCommand};
use cyancia_image::tile::{GpuTileInfo, GpuTileStorage, GpuTileStorageInner};
use cyancia_render::{buffer::BufferVec, render_context::RenderContext};
use cyancia_tools::{ToolFunction, ToolId};
use cyancia_utils::log_err::LogErr;
use glam::{Vec2, Vec4};
use gpui::{
    AnyElement, AppContext, Context, IntoElement, MouseDownEvent, ParentElement, Styled, Window,
    prelude::FluentBuilder,
};
use gpui_component::{
    Selectable, Sizable,
    button::{Button, ButtonGroup},
    form::{field, v_form},
    input::{InputEvent, InputState, MaskPattern, NumberInput, NumberInputEvent, StepAction},
    v_flex,
};
use wgpu::{BufferUsages, Extent3d, TextureDimension, TextureUsages, wgt::TextureDescriptor};

use crate::render::{SelectionOperation, SelectionPipeline};

pub struct MagicWandSelectionTool {
    threshold: f32,
    alpha_threshold: f32,
    grow: i32,
    close_gap: u32,
    cached_feather: u32,
    aa_approach: BucketAntialiasApproach,
}

impl Default for MagicWandSelectionTool {
    fn default() -> Self {
        let b = BucketTool::default();
        Self {
            threshold: b.threshold,
            alpha_threshold: b.alpha_threshold,
            grow: b.grow,
            close_gap: b.close_gap,
            cached_feather: b.cached_feather,
            aa_approach: b.aa_approach,
        }
    }
}

impl ToolFunction for MagicWandSelectionTool {
    fn new(cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    fn id() -> ToolId {
        ToolId::new("magic_wand_selection_tool")
    }

    fn begin(&mut self, mouse: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(canvas) = cx.read_current_canvas() else {
            return;
        };
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

        let tiles = cx.global::<GpuTileStorage>();
        let render_context = cx.global::<RenderContext>();
        // TODO Reference other layers
        let ref_layer_id = canvas.image.active_layer;
        let ref_layer_info = tiles.get_layer_tiles(ref_layer_id).unwrap();
        let ref_layer_info_buffer = tiles.get_layer_info(ref_layer_id).unwrap();
        let ref_layer = tiles.get_layer_binding_or_empty(ref_layer_id).unwrap();

        let image_size = canvas.image.size();
        let params = BucketParams {
            seed: position_ps.as_uvec2(),
            fill_color: Vec4::ZERO,
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
            // This won't be used
            ref_layer_info_buffer.texel_type,
        );
        let Some((mut output_texture, mut output_tiles)) = bucket.dispatch_mask(
            &render_context.device,
            &render_context.queue,
            &params,
            &ref_layer,
            ref_layer_info.into_iter().collect(),
        ) else {
            return;
        };

        let selection_layer_id = canvas.image.selection_layer();
        let selection_layer_storage = tiles.get_layer(selection_layer_id).unwrap();
        let selection_layer_info = selection_layer_storage.layer_info();

        output_tiles.extend(selection_layer_storage.iter_tiles().map(|(i, _, _)| i));

        let selection_layer = tiles
            .get_layer_binding_or_empty(selection_layer_id)
            .unwrap();

        if output_texture.depth_or_array_layers() != output_tiles.len() as u32 {
            let t = render_context.device.create_texture(&TextureDescriptor {
                label: Some("inout_texture"),
                size: Extent3d {
                    width: GpuTileStorageInner::TILE_SIZE,
                    height: GpuTileStorageInner::TILE_SIZE,
                    depth_or_array_layers: output_tiles.len() as u32,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: output_texture.format(),
                usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_DST | TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let mut ec = render_context
                .device
                .create_command_encoder(&Default::default());
            ec.copy_texture_to_texture(
                output_texture.as_image_copy(),
                t.as_image_copy(),
                output_texture.size(),
            );
            render_context.queue.submit([ec.finish()]);
            output_texture = t;
        }

        let output_tile_info_buffer = {
            let mut b = BufferVec::new(
                Some("inout_tile_info_buffer".to_string()),
                BufferUsages::STORAGE,
            );
            for index in output_tiles.iter() {
                b.push(&GpuTileInfo {
                    index: *index,
                    origin: *index * GpuTileStorageInner::TILE_SIZE as i32,
                });
            }
            b.write_buffer(&render_context.device, &render_context.queue);
            b.into_inner_buffer().unwrap()
        };

        let selection_pipeline =
            SelectionPipeline::new(&render_context.device, selection_layer_info.texel_type);
        let mut ec = render_context
            .device
            .create_command_encoder(&Default::default());
        selection_pipeline.composite(
            &render_context.device,
            &render_context.queue,
            &mut ec,
            SelectionOperation::from_modifiers(mouse.modifiers),
            &output_texture,
            &output_tiles,
            &output_tile_info_buffer,
            &selection_layer,
        );
        render_context.queue.submit([ec.finish()]);

        let cmd = TileReplaceCommand::new(
            "Magic Wand".into(),
            canvas_id,
            &render_context.device,
            &render_context.queue,
            selection_layer_id,
            &selection_layer_storage,
            output_tiles.into_iter().collect(),
            output_texture,
        );
        drop(selection_layer_storage);
        cx.push_undo_command_to_current(cmd).log_err();
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
