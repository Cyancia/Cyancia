use cyancia_image::blend_modes::BlendMode;
use cyancia_shader_graph::{
    GraphRenderer, GraphTheme,
    graph::{
        Graph, GraphCompileError, GraphDynamicInstancesStorage,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext, StatelessCommonGraphNode,
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

pub fn brush_graph_storage() -> GraphDynamicInstancesStorage {
    let mut storage = GraphDynamicInstancesStorage::default();
    storage.nodes.register::<PenPosition>();
    storage.nodes.register::<PixelPosition>();
    storage.nodes.register::<OutputPixelColor>();
    storage.nodes.register::<PasteTextureNode>();
    storage.nodes.register::<BlendColorNode>();
    storage.nodes.register::<CurrentPixelColorNode>();
    storage
}

pub struct GraphInputParams {
    pub pen_position: Vec2,
}

#[derive(Default, Clone)]
pub struct PenPosition;

impl StatelessCommonGraphNode for PenPosition {
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

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = graph_input.pen_position;",
            ctx.get_output(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct PixelPosition;

impl StatelessCommonGraphNode for PixelPosition {
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

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
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
pub struct OutputPixelColor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputPixelColorState {
    pub blend_mode: BlendMode,
}

#[derive(Clone)]
pub enum OutputPixelColorMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    SetBlendMode(BlendMode),
}

impl StatelessCommonGraphNode for OutputPixelColor {
    fn name(&self) -> &'static str {
        "Output Pixel Color"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn header_color(&self) -> Color {
        color!(0x79f2bb)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO)]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            r#"
            textureStore(outputs[cur_layer], vec2u(cur_coord), image::texture_unpack::pack_rgba8_texel({}));
            "#,
            ctx.get_input(0)?
        ))
    }
}

#[derive(Default, Clone)]
pub struct PasteTextureNode;

impl StatelessCommonGraphNode for PasteTextureNode {
    fn name(&self) -> &'static str {
        "Paste Texture"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        // TODO: sample modes
        &["Texture", "Translation", "Rotation", "Scale", "Anchor"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Result"]
    }

    fn header_color(&self) -> Color {
        color!(0x79d3f2)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>(TextureLocalIndex::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ONE),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::splat(0.5)),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let tex = ctx.get_input(0)?;
        let translation = ctx.get_input(1)?;
        let rotation = ctx.get_input(2)?;
        let scale = ctx.get_input(3)?;
        let anchor = ctx.get_input(4)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = paste_texture({}, pixel_posf, {}, {}, {}, {});\n",
            output, tex, scale, rotation, translation, anchor
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
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn header_color(&self) -> Color {
        color!(0xf279f0)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let output = ctx.get_output(0)?;
        Ok(format!(
            r#"
@if(POSTPROCESSING) let {} = image::texture_unpack::unpack_rgba8_texel(textureLoad(inputs[cur_layer], vec2u(cur_coord)));
@else let {} = image::texture_unpack::unpack_rgba8_texel(textureLoad(outputs[cur_layer], vec2u(cur_coord)));
            "#,
            output, output
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

    fn create_inputs(&self, state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
        ]
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
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
            "let {} = image::blend_modes::{}({}, {});\n",
            output,
            state.blend_mode.shader_func(),
            src,
            dst
        ))
    }
}
