use bevy_math::{IRect, Rect, VectorSpace};
use cyancia_image::blend_modes::BlendMode;
use cyancia_math::{mat3::Mat3ScaleRotattionTranslationWithAnchor, rect_transform::RectTransform};
use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    graph::{
        GraphData,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeInputsViewContext, GraphNodeOutputsViewContext, GraphNodeRunContext,
            GraphNodeRunError, GraphNodeUpdateContext, GraphNodeUpdateSignatureContext,
            StatelessCommonGraphNode,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::{
        nodes::{GraphDataWithTime, GraphTimes},
        types::{ColorType, F32Type, RectType, TextureReference, TextureType, Vec2FType},
    },
};
use cyancia_shader_graph_derive::stateless;
use glam::{Mat2, Mat3, Vec2, Vec4};
use iced_core::{Color, Element, color};
use iced_wgpu::graphics::damage;
use iced_widget::{Column, column, pick_list};
use serde::{Deserialize, Serialize};

use crate::render::{ComputedPenInput, Time};

#[derive(Default, Clone)]
pub struct BrushGraphPostprocessData {
    pub accumulated_pixel_bounds: IRect,
    pub time: Time,
}

impl GraphData for BrushGraphPostprocessData {}

impl GraphDataWithTime for BrushGraphPostprocessData {
    fn time(&self) -> GraphTimes {
        GraphTimes {
            now: self.time.now,
            stroke_begin: self.time.stroke_begin,
        }
    }

    fn wgsl_variable() -> String {
        "sample.time".into()
    }
}

#[derive(Default, Clone)]
pub struct BrushGraphData {
    pub pen_input: ComputedPenInput,
}

impl GraphData for BrushGraphData {}

impl GraphDataWithTime for BrushGraphData {
    fn time(&self) -> GraphTimes {
        GraphTimes {
            now: self.pen_input.time.now,
            stroke_begin: self.pen_input.time.stroke_begin,
        }
    }

    fn wgsl_variable() -> String {
        "sample.time".into()
    }
}

#[derive(Default, Clone)]
pub struct BrushGraphDataTuple {
    pub lhs: BrushGraphData,
    pub rhs: BrushGraphData,
}

impl GraphData for BrushGraphDataTuple {}

#[derive(Default, Clone)]
pub struct PenPositionNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphData> for PenPositionNode {
    fn name(&self) -> &'static str {
        "Pen Position"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Position"]
    }

    fn header_color(&self) -> Color {
        color!(0x79b5f2)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphData>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphData>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphData>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = graph_input.position;",
            ctx.get_output(0)?
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphData>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<Vec2FType>(0, ctx.data.pen_input.position)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct DrawDirectionNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphData> for DrawDirectionNode {
    fn name(&self) -> &'static str {
        "Draw Direction"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Angle", "Direction"]
    }

    fn header_color(&self) -> Color {
        color!(0xc1c073)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphData>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphData>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphData>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = graph_input.draw_direction_angle;\nlet {} = graph_input.draw_direction_vec;\n",
            ctx.get_output(0)?,
            ctx.get_output(1)?
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphData>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<F32Type>(0, ctx.data.pen_input.draw_direction_angle)?;
        ctx.set_output_value::<Vec2FType>(1, ctx.data.pen_input.draw_direction_vec)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct PixelPositionNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for PixelPositionNode {
    fn name(&self) -> &'static str {
        "Pixel Position"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Position"]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2a0)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("let {} = pixel_posf;", ctx.get_output(0)?))
    }
}

#[derive(Default, Clone)]
pub struct FilterWithinMaskNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for FilterWithinMaskNode {
    fn name(&self) -> &'static str {
        "Filter Within Mask"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[
            "Color",
            "Mask",
            "Translation",
            "Rotation",
            "Scale",
            "Anchor",
        ]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Color", "Bounds"]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2bb)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<TextureType>(TextureReference::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ONE),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::splat(0.5)),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<ColorType>(),
            GraphDefaultOutputSlot::new::<RectType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let mask = ctx.get_input(1)?;
        let translation = ctx.get_input(2)?;
        let rotation = ctx.get_input(3)?;
        let scale = ctx.get_input(4)?;
        let anchor = ctx.get_input(5)?;

        Ok(format!(
            "let {} = filter_within_mask(pixel_pos, {}, {}, {}, {}, {}, {});\nlet {} = filter_within_mask_bounds({}, {}, {}, {}, {});\n",
            ctx.get_output(0)?,
            color,
            mask,
            scale,
            rotation,
            translation,
            anchor,
            ctx.get_output(1)?,
            mask,
            scale,
            rotation,
            translation,
            anchor,
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let mask = ctx.get_input_value::<TextureType>(1)?;
        let translation = ctx.get_input_value::<Vec2FType>(2)?;
        let rotation = ctx.get_input_value::<F32Type>(3)?;
        let scale = ctx.get_input_value::<Vec2FType>(4)?;
        let anchor = ctx.get_input_value::<Vec2FType>(5)?;
        let texture = ctx
            .resources
            .textures
            .get(&mask.external_id)
            .expect("Texture not found");
        let img = &texture.handle.get().unwrap().image;
        let size = Vec2::new(img.width() as f32, img.height() as f32);
        let bounds = Rect {
            min: Vec2::ZERO,
            max: size,
        };

        let mat = Mat3::from_scale_angle_translation_with_anchor(
            scale,
            rotation,
            translation,
            anchor * size,
        );
        let bounds = bounds.transformed(&mat);

        ctx.set_output_value::<RectType>(1, bounds)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct FilterWithinBoundsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for FilterWithinBoundsNode {
    fn name(&self) -> &'static str {
        "Filter Within Bounds"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Color", "Bounds"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Color", "Bounds"]
    }

    fn header_color(&self) -> Color {
        color!(0x79b8f2)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<RectType>(Rect::default()),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<ColorType>(),
            GraphDefaultOutputSlot::new::<RectType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let bounds = ctx.get_input(1)?;

        Ok(format!(
            "let {} = filter_within_bounds(pixel_pos, {}, {});\nlet {} = {};\n",
            ctx.get_output(0)?,
            color,
            bounds,
            ctx.get_output(1)?,
            bounds
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let bounds = ctx.get_input_value::<RectType>(1)?;
        // TODO We are unable to determine if the current pixel is filtered out or not.
        ctx.set_output_value::<ColorType>(0, Vec4::ZERO)?;
        ctx.set_output_value::<RectType>(1, bounds)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct OutputColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputColorNode {
    fn name(&self) -> &'static str {
        "Output Color"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn header_color(&self) -> Color {
        color!(0xf50687)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO)]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "set_output_color(pixel_pos, {});\n",
            ctx.get_input(0)?
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct OutputBoundsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputBoundsNode {
    fn name(&self) -> &'static str {
        "Output Bounds"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Bounds"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn header_color(&self) -> Color {
        color!(0x6db477)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<RectType>(Rect::default())]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn update_signature(&self, mut ctx: GraphNodeUpdateSignatureContext<'_, Data>) {
        ctx.require_input_slot_as_graph_output(0, "Bounds".to_string());
    }

    fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(Default::default())
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct PasteTextureNode;

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasteTextureMode {
    Clamp,
    #[default]
    Wrap,
}

impl ToString for PasteTextureMode {
    fn to_string(&self) -> String {
        match self {
            PasteTextureMode::Clamp => "Clamp".to_string(),
            PasteTextureMode::Wrap => "Wrap".to_string(),
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct PasteTextureNodeState {
    pub mode: PasteTextureMode,
}

#[derive(Clone)]
pub enum PasteTextureNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    SetMode(PasteTextureMode),
}

impl<Data: GraphData> GraphNode<Data> for PasteTextureNode {
    type State = PasteTextureNodeState;
    type Message = PasteTextureNodeMessage;

    fn name(&self) -> &'static str {
        "Paste Texture"
    }

    fn default_state(&self) -> Self::State {
        Default::default()
    }

    fn header_color(&self) -> Color {
        color!(0x79d3f2)
    }

    fn create_inputs(
        &self,
        _state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>(TextureReference::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ONE),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::splat(0.5)),
        ]
    }

    fn create_outputs(
        &self,
        _state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_inputs(
            &["Texture", "Translation", "Rotation", "Scale", "Anchor"],
            PasteTextureNodeMessage::LiteralUpdate,
        ))
        .push(pick_list(
            [PasteTextureMode::Clamp, PasteTextureMode::Wrap],
            Some(state.mode),
            PasteTextureNodeMessage::SetMode,
        ))
        .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Color"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            PasteTextureNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
            PasteTextureNodeMessage::SetMode(mode) => state.mode = mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let tex = ctx.get_input(0)?;
        let translation = ctx.get_input(1)?;
        let rotation = ctx.get_input(2)?;
        let scale = ctx.get_input(3)?;
        let anchor = ctx.get_input(4)?;
        let output = ctx.get_output(0)?;
        let fn_name = match state.mode {
            PasteTextureMode::Clamp => "sample_transformed_local_texture_clamp",
            PasteTextureMode::Wrap => "sample_transformed_local_texture_wrap",
        };
        Ok(format!(
            "let {} = {}({}, pixel_posf, {}, {}, {}, {});\n",
            output, fn_name, tex, scale, rotation, translation, anchor
        ))
    }
}

#[derive(Default, Clone)]
pub struct CurrentPixelColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for CurrentPixelColorNode {
    fn name(&self) -> &'static str {
        "Current Pixel Color"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Position"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn header_color(&self) -> Color {
        color!(0xf279f0)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO)]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = current_input_color(vec2i({}));\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct LayerPixelColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for LayerPixelColorNode {
    fn name(&self) -> &'static str {
        "Layer Pixel Color"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Position"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2d4)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO)]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = target_layer_color(vec2i({}));\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct BlendColorNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlendColorNodeState {
    pub blend_mode: BlendMode,
}

#[derive(Clone)]
pub enum BlendColorNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    SetBlendMode(BlendMode),
}

impl<Data: GraphData> GraphNode<Data> for BlendColorNode {
    type State = BlendColorNodeState;
    type Message = BlendColorNodeMessage;

    fn name(&self) -> &'static str {
        "Blend Color"
    }

    fn default_state(&self) -> Self::State {
        BlendColorNodeState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self) -> Color {
        color!(0x79ccf2)
    }

    fn create_inputs(
        &self,
        _state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
        ]
    }

    fn create_outputs(
        &self,
        _state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let pick = pick_list(
            BlendMode::ALL,
            Some(state.blend_mode),
            BlendColorNodeMessage::SetBlendMode,
        );
        column![pick]
            .extend(ctx.view_all_inputs(
                &["Src Color", "Dst Color"],
                BlendColorNodeMessage::LiteralUpdate,
            ))
            .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Color"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            BlendColorNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
            BlendColorNodeMessage::SetBlendMode(blend_mode) => state.blend_mode = blend_mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let src = ctx.get_input(0)?;
        let dst = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;
        Ok(format!(
            "let {} = package::image::blend_modes::{}({}, {});\n",
            output,
            state.blend_mode.shader_func(),
            src,
            dst
        ))
    }
}

#[derive(Default, Clone)]
pub struct StrokeBoundsNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphPostprocessData> for StrokeBoundsNode {
    fn name(&self) -> &'static str {
        "Stroke Bounds"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Bounds"]
    }

    fn header_color(&self) -> Color {
        color!(0xa2f279)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphPostprocessData>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphPostprocessData>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<RectType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphPostprocessData>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = Rect(vec2f(sample.accumulated_bound.min), vec2f(sample.accumulated_bound.max));",
            ctx.get_output(0)?
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphPostprocessData>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<RectType>(
            0,
            Rect {
                min: ctx.data.accumulated_pixel_bounds.min.as_vec2(),
                max: ctx.data.accumulated_pixel_bounds.max.as_vec2(),
            },
        )?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct EllipticalMaskNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for EllipticalMaskNode {
    fn name(&self) -> &'static str {
        "Elliptical Mask"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Sample Position", "Center", "Radii"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Mask Value", "Bounds"]
    }

    fn header_color(&self) -> Color {
        color!(0x462bbb)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<RectType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let mask = ctx.ident_generator.next_output();
        Ok(format!(
            "let {mask} = elliptical_mask({}, {}, {});\nlet {} = {mask}.value;\nlet {} = {mask}.bounds;\n",
            ctx.get_input(0)?,
            ctx.get_input(1)?,
            ctx.get_input(2)?,
            ctx.get_output(0)?,
            ctx.get_output(1)?
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let center = ctx.get_input_value::<Vec2FType>(1)?;
        let radii = ctx.get_input_value::<Vec2FType>(2)?;
        ctx.set_output_value::<RectType>(
            1,
            Rect {
                min: center - radii,
                max: center + radii,
            },
        )?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct BlendWithInputNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlendWithBufferNodeState {
    pub blend_mode: BlendMode,
}

#[derive(Clone)]
pub enum BlendWithBufferNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    SetBlendMode(BlendMode),
}

impl<Data: GraphData> GraphNode<Data> for BlendWithInputNode {
    type State = BlendWithBufferNodeState;
    type Message = BlendWithBufferNodeMessage;

    fn name(&self) -> &'static str {
        "Blend With Input"
    }

    fn default_state(&self) -> Self::State {
        BlendWithBufferNodeState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self) -> Color {
        color!(0x93f0fd)
    }

    fn create_inputs(
        &self,
        _state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(
        &self,
        _state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_inputs(
            &["Color", "Opacity"],
            BlendWithBufferNodeMessage::LiteralUpdate,
        ))
        .push(pick_list(
            BlendMode::ALL,
            Some(state.blend_mode),
            BlendWithBufferNodeMessage::SetBlendMode,
        ))
        .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Color"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            BlendWithBufferNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
            BlendWithBufferNodeMessage::SetBlendMode(blend_mode) => state.blend_mode = blend_mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let opacity = ctx.get_input(1)?;
        Ok(format!(
            "let {} = package::image::blend_modes::{}(vec4f({color}.rgb, {color}.a * {opacity}), current_input_color(pixel_pos));\n",
            ctx.get_output(0)?,
            state.blend_mode.shader_func()
        ))
    }
}

#[derive(Default, Clone)]
pub struct BlendWithLayerNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlendWithLayerNodeState {
    pub blend_mode: BlendMode,
}

#[derive(Clone)]
pub enum BlendWithLayerNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    SetBlendMode(BlendMode),
}

impl<Data: GraphData> GraphNode<Data> for BlendWithLayerNode {
    type State = BlendWithLayerNodeState;
    type Message = BlendWithLayerNodeMessage;

    fn name(&self) -> &'static str {
        "Blend With Layer"
    }

    fn default_state(&self) -> Self::State {
        BlendWithLayerNodeState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self) -> Color {
        color!(0xc0f83a)
    }

    fn create_inputs(
        &self,
        _state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(
        &self,
        _state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_inputs(
            &["Color", "Opacity"],
            BlendWithLayerNodeMessage::LiteralUpdate,
        ))
        .push(pick_list(
            BlendMode::ALL,
            Some(state.blend_mode),
            BlendWithLayerNodeMessage::SetBlendMode,
        ))
        .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Color"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            BlendWithLayerNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
            BlendWithLayerNodeMessage::SetBlendMode(blend_mode) => state.blend_mode = blend_mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let opacity = ctx.get_input(1)?;
        Ok(format!(
            "let {} = package::image::blend_modes::{}(vec4f({color}.rgb, {color}.a * {opacity}), target_layer_color(pixel_pos));\n",
            ctx.get_output(0)?,
            state.blend_mode.shader_func()
        ))
    }
}

#[derive(Default, Clone)]
pub struct OutputSpacingNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputSpacingNode {
    fn name(&self) -> &'static str {
        "Output Spacing"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Spacing"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn header_color(&self) -> Color {
        color!(0x23948d)
    }

    fn update_signature(&self, mut ctx: GraphNodeUpdateSignatureContext<'_, Data>) {
        ctx.require_input_slot_as_graph_output(0, "Spacing".to_string());
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("return {};\n", ctx.get_input(0)?))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct PenPositionsNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphDataTuple> for PenPositionsNode {
    fn name(&self) -> &'static str {
        "Pen Positions"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Src Position", "Dst Position"]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2c9)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<Vec2FType>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphDataTuple>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "
            let {} = src.position;
            let {} = dst.position;
            ",
            ctx.get_output(0)?,
            ctx.get_output(1)?,
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphDataTuple>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<Vec2FType>(0, ctx.data.lhs.pen_input.position)?;
        ctx.set_output_value::<Vec2FType>(1, ctx.data.rhs.pen_input.position)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct DrawDirectionsNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphDataTuple> for DrawDirectionsNode {
    fn name(&self) -> &'static str {
        "Draw Directions"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Src Angle", "Dst Angle", "Src Direction", "Dst Direction"]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2c0)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphDataTuple>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "
            let {} = src.draw_direction_angle;
            let {} = dst.draw_direction_angle;
            let {} = src.draw_direction_vec;
            let {} = dst.draw_direction_vec;
            ",
            ctx.get_output(0)?,
            ctx.get_output(1)?,
            ctx.get_output(2)?,
            ctx.get_output(3)?,
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphDataTuple>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<F32Type>(0, ctx.data.lhs.pen_input.draw_direction_angle)?;
        ctx.set_output_value::<F32Type>(1, ctx.data.rhs.pen_input.draw_direction_angle)?;
        ctx.set_output_value::<Vec2FType>(2, ctx.data.lhs.pen_input.draw_direction_vec)?;
        ctx.set_output_value::<Vec2FType>(3, ctx.data.rhs.pen_input.draw_direction_vec)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct TimesNode;

#[stateless]
impl StatelessCommonGraphNode<BrushGraphDataTuple> for TimesNode {
    fn name(&self) -> &'static str {
        "Times"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Src Now", "Src Stroke Begin", "Dst Now", "Dst Stroke Begin"]
    }

    fn header_color(&self) -> Color {
        color!(0xb88e9d)
    }

    fn create_inputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, BrushGraphDataTuple>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, BrushGraphDataTuple>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "
            let {} = src.time.now;
            let {} = src.time.stroke_begin;
            let {} = dst.time.now;
            let {} = dst.time.stroke_begin;
            ",
            ctx.get_output(0)?,
            ctx.get_output(1)?,
            ctx.get_output(2)?,
            ctx.get_output(3)?,
        ))
    }

    fn run(
        &self,
        mut ctx: GraphNodeRunContext<'_, BrushGraphDataTuple>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<F32Type>(0, ctx.data.lhs.pen_input.time.now)?;
        ctx.set_output_value::<F32Type>(1, ctx.data.lhs.pen_input.time.stroke_begin)?;
        ctx.set_output_value::<F32Type>(2, ctx.data.rhs.pen_input.time.now)?;
        ctx.set_output_value::<F32Type>(3, ctx.data.rhs.pen_input.time.stroke_begin)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct OutputRequiredSpacingNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputRequiredSpacingNode {
    fn name(&self) -> &'static str {
        "Output Required Spacing"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Required Spacing"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn header_color(&self) -> Color {
        color!(0x3f463c)
    }

    fn update_signature(&self, mut ctx: GraphNodeUpdateSignatureContext<'_, Data>) {
        ctx.require_input_slot_as_graph_output(0, "Required Spacing".to_string());
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("return {};\n", ctx.get_input(0)?))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        Ok(())
    }
}
