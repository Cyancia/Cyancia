use cyancia_canvas::{CanvasAppExt, CanvasUndoStackAppExt, command::TileReplaceCommand};
use cyancia_color::ForegroundBackgroundColorExt;
use cyancia_image::tile::TileStorageAppExt;
use cyancia_input::{key::KeyboardState, mouse::PressedMouseState};
use cyancia_render::render_context::RenderContextAppExt;
use cyancia_runtime::{Application, Services, plugin::Plugin};
use cyancia_tools::{ToolFunction, ToolId, ToolsAppExt};
use cyancia_utils::log_err::LogErr;
use cyancia_widgets::{
    fluent_builder::When, form::Form, spin_slider::SpinSlider, style::ButtonStyle,
};
use glam::{Vec2, Vec4};
use iced_core::{Element, Length, Theme};
use iced_runtime::Task;
use iced_wgpu::Renderer;
use iced_widget::{button, container, row};

use crate::bucket::{Bucket, BucketAntialiasApproach, BucketParams};

pub mod bucket;

pub struct BucketPlugin;

impl Plugin for BucketPlugin {
    fn build(&self, app: &mut Application) {
        app.runtime_mut()
            .services_mut()
            .add_tool_function::<BucketTool>();
    }
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

#[derive(Debug, Clone)]
pub enum BucketToolMessage {
    ThresholdChanged(f32),
    AlphaThresholdChanged(f32),
    GrowChanged(i32),
    CloseGapChanged(u32),
    FeatherChanged(u32),
    AaApproachSelected(BucketAntialiasApproach),
}

impl ToolFunction for BucketTool {
    type Message = BucketToolMessage;

    fn id() -> ToolId {
        ToolId::new("bucket_tool".into())
    }

    fn end(
        &mut self,
        _: &KeyboardState,
        mouse: &PressedMouseState,
        services: &mut Services,
    ) -> Task<Self::Message> {
        let Some(canvas_id) = services.current_canvas_id() else {
            return Task::none();
        };
        let Some(canvas) = services.current_canvas() else {
            return Task::none();
        };

        let position_ws = Vec2::new(mouse.position.x, mouse.position.y);
        let position_ps = canvas.transform.window_to_pixel(position_ws);
        if position_ps.x < 0.0
            || position_ps.y < 0.0
            || position_ps.x > canvas.image.size().x as f32
            || position_ps.y > canvas.image.size().y as f32
        {
            return Task::none();
        }

        let profile = canvas.image.profile();
        let fg_color = services
            .foreground_color()
            .get()
            .into_rgb(profile.rgb_to_xyz_matrix().to_f32().inverse());

        let tiles = services.tile_storage();
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
            fill_color: Vec4::new(fg_color.r, fg_color.g, fg_color.b, 1.0),
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

        let device = services.render_device();
        let queue = services.render_queue();

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
                new_tiles.iter_tile_indices().collect(),
                new_tiles.texture().unwrap().clone(),
            );
            drop(output_layer);
            services.push_undo_command_to_current(cmd).log_err();
        }

        Task::none()
    }

    fn handle_message(&mut self, message: Self::Message, _: &mut Services) -> Task<Self::Message> {
        match message {
            BucketToolMessage::ThresholdChanged(value) => self.threshold = value,
            BucketToolMessage::AlphaThresholdChanged(value) => self.alpha_threshold = value,
            BucketToolMessage::GrowChanged(value) => self.grow = value,
            BucketToolMessage::CloseGapChanged(value) => self.close_gap = value,
            BucketToolMessage::FeatherChanged(value) => {
                self.cached_feather = value;
                if matches!(self.aa_approach, BucketAntialiasApproach::Feather(_)) {
                    self.aa_approach = BucketAntialiasApproach::Feather(value);
                }
            }
            BucketToolMessage::AaApproachSelected(approach) => {
                self.aa_approach = match approach {
                    BucketAntialiasApproach::Feather(_) => {
                        BucketAntialiasApproach::Feather(self.cached_feather)
                    }
                    other => other,
                };
            }
        }

        Task::none()
    }

    fn tool_option_widget<'a>(
        &'a self,
        _: &'a Services,
    ) -> Option<Element<'a, Self::Message, Theme, Renderer>> {
        let fields = Form::new()
            .push(
                "Threshold",
                SpinSlider::new_01(self.threshold).on_confirm(BucketToolMessage::ThresholdChanged),
            )
            .push(
                "Alpha Threshold",
                SpinSlider::new_01(self.alpha_threshold)
                    .on_confirm(BucketToolMessage::AlphaThresholdChanged),
            )
            .push(
                "Grow",
                SpinSlider::new(-64..=64, self.grow).on_confirm(BucketToolMessage::GrowChanged),
            )
            .push(
                "Close Gap",
                SpinSlider::new(0..=64, self.close_gap)
                    .on_confirm(BucketToolMessage::CloseGapChanged),
            )
            .push(
                "Antialiasing Approach",
                row![
                    button("None")
                        .on_press(BucketToolMessage::AaApproachSelected(
                            BucketAntialiasApproach::None
                        ))
                        .style_pressed(matches!(self.aa_approach, BucketAntialiasApproach::None)),
                    button("FXAA")
                        .on_press(BucketToolMessage::AaApproachSelected(
                            BucketAntialiasApproach::Fxaa
                        ))
                        .style_pressed(matches!(self.aa_approach, BucketAntialiasApproach::Fxaa)),
                    button("Feather")
                        .on_press(BucketToolMessage::AaApproachSelected(
                            BucketAntialiasApproach::Feather(self.cached_feather)
                        ))
                        .style_pressed(matches!(
                            self.aa_approach,
                            BucketAntialiasApproach::Feather(_)
                        )),
                ],
            )
            .when(
                matches!(self.aa_approach, BucketAntialiasApproach::Feather(_)),
                |form| {
                    form.push(
                        "Feather",
                        SpinSlider::new(0..=64, self.cached_feather)
                            .on_confirm(BucketToolMessage::FeatherChanged)
                            .precision(0),
                    )
                },
            );

        Some(container(fields).padding(8).width(Length::Fill).into())
    }
}
