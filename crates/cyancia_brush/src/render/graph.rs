use cyancia_image::blend_modes::BlendMode;
use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    graph::{
        Graph, GraphCompileError,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeInputsViewContext, GraphNodeOutputsViewContext, GraphNodeUpdateContext,
            StatelessCommonGraphNode,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::{ColorType, F32Type, TextureLocalIndex, TextureType, Vec2FType},
};
use glam::{Vec2, Vec4};
use iced_core::{Color, Element, color};
use iced_widget::{Column, column, pick_list, space};
use serde::{Deserialize, Serialize};
use wesl::{VirtualResolver, Wesl};

#[derive(Default, Clone)]
pub struct PenPositionNode;

impl StatelessCommonGraphNode for PenPositionNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = graph_input.position;",
            ctx.get_output(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct DrawDirectionNode;

impl StatelessCommonGraphNode for DrawDirectionNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = graph_input.draw_direction_angle;\nlet {} = graph_input.draw_direction_vec;\n",
            ctx.get_output(0)?,
            ctx.get_output(1)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct PixelPositionNode;

impl StatelessCommonGraphNode for PixelPositionNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("let {} = pixel_posf;", ctx.get_output(0)?))
    }
}

#[derive(Default, Clone)]
pub struct FilterWithinMaskNode;

impl StatelessCommonGraphNode for FilterWithinMaskNode {
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
        &["Color"]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2bb)
    }

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<TextureType>(TextureLocalIndex::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ONE),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::splat(0.5)),
        ]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = filter_within_mask(pixel_pos, {}, {}, {}, {}, {}, {});\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?,
            ctx.get_input(1)?,
            ctx.get_input(4)?,
            ctx.get_input(3)?,
            ctx.get_input(2)?,
            ctx.get_input(5)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct FilterWithinBoundsNode;

impl StatelessCommonGraphNode for FilterWithinBoundsNode {
    fn name(&self) -> &'static str {
        "Filter Within Bounds"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Color", "Min Bounds", "Max Bounds"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn header_color(&self) -> Color {
        color!(0x79b8f2)
    }

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
        ]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = filter_within_bounds(pixel_pos, {}, {}, {});\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?,
            ctx.get_input(1)?,
            ctx.get_input(2)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct OutputColorNode;

impl StatelessCommonGraphNode for OutputColorNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO)]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "set_output_color(pixel_pos, {});\n",
            ctx.get_input(0)?,
        ))
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

impl GraphNode for PasteTextureNode {
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
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>(TextureLocalIndex::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ONE),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::splat(0.5)),
        ]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Color"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            PasteTextureNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
            PasteTextureNodeMessage::SetMode(mode) => state.mode = mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext,
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

impl StatelessCommonGraphNode for CurrentPixelColorNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO)]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
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

impl StatelessCommonGraphNode for LayerPixelColorNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO)]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
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

impl GraphNode for BlendColorNode {
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
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
        ]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Color"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            BlendColorNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
            BlendColorNodeMessage::SetBlendMode(blend_mode) => state.blend_mode = blend_mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext,
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

impl StatelessCommonGraphNode for StrokeBoundsNode {
    fn name(&self) -> &'static str {
        "Stroke Bounds"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Min Bounds", "Max Bounds"]
    }

    fn header_color(&self) -> Color {
        color!(0xa2f279)
    }

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<Vec2FType>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let bounds = ctx.ident_generator.next_output();
        Ok(format!(
            "
            let {bounds} = get_accumulated_pixel_bounds();
            let {} = vec2f({bounds}.min);
            let {} = vec2f({bounds}.max);
            ",
            ctx.get_output(0)?,
            ctx.get_output(1)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct EllipticalMaskNode;

impl StatelessCommonGraphNode for EllipticalMaskNode {
    fn name(&self) -> &'static str {
        "Elliptical Mask"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Sample Position", "Center", "Radii"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Mask Value", "Min Bounds", "Max Bounds"]
    }

    fn header_color(&self) -> Color {
        color!(0x462bbb)
    }

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
        ]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let mask = ctx.ident_generator.next_output();

        Ok(format!(
            "let {mask} = elliptical_mask({}, {}, {});\nlet {} = {mask}.value;\nlet {} = {mask}.min_bounds;\nlet {} = {mask}.max_bounds;\n",
            ctx.get_input(0)?,
            ctx.get_input(1)?,
            ctx.get_input(2)?,
            ctx.get_output(0)?,
            ctx.get_output(1)?,
            ctx.get_output(2)?,
        ))
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

impl GraphNode for BlendWithInputNode {
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
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Color"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            BlendWithBufferNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
            BlendWithBufferNodeMessage::SetBlendMode(blend_mode) => state.blend_mode = blend_mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext,
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

impl GraphNode for BlendWithLayerNode {
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
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Color"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            BlendWithLayerNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
            BlendWithLayerNodeMessage::SetBlendMode(blend_mode) => state.blend_mode = blend_mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext,
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

impl StatelessCommonGraphNode for OutputSpacingNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("return {};\n", ctx.get_input(0)?))
    }
}

#[derive(Default, Clone)]
pub struct PenPositionsNode;

impl StatelessCommonGraphNode for PenPositionsNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<Vec2FType>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct DrawDirectionsNode;

impl StatelessCommonGraphNode for DrawDirectionsNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct OutputRequiredSpacingNode;

impl StatelessCommonGraphNode for OutputRequiredSpacingNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("return {};\n", ctx.get_input(0)?))
    }
}
