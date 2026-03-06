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
use iced_widget::{Column, pick_list, space};
use serde::{Deserialize, Serialize};
use wesl::{VirtualResolver, Wesl};

pub fn brush_graph_storage() -> GraphDynamicInstancesStorage {
    let mut storage = GraphDynamicInstancesStorage::default();
    storage.nodes.register::<PenPosition>();
    storage.nodes.register::<PixelPosition>();
    storage.nodes.register::<OutputPixelColor>();
    storage.nodes.register::<PasteTextureNode>();
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

impl GraphNode for OutputPixelColor {
    type State = OutputPixelColorState;

    type Message = OutputPixelColorMessage;

    fn name(&self) -> &'static str {
        "Output Pixel Color"
    }

    fn default_state(&self) -> Self::State {
        OutputPixelColorState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self) -> Color {
        color!(0x79f2bb)
    }

    fn create_inputs(&self, state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO)]
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = Column::with_children(
            ctx.view_all_inputs(&["Color"], OutputPixelColorMessage::LiteralUpdate),
        );

        column = column.push(pick_list(
            BlendMode::ALL,
            Some(state.blend_mode),
            OutputPixelColorMessage::SetBlendMode,
        ));

        column.into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        space().into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            OutputPixelColorMessage::LiteralUpdate(m) => ctx.update_literal(m),
            OutputPixelColorMessage::SetBlendMode(blend_mode) => state.blend_mode = blend_mode,
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let layer = ctx.ident_generator.next_output();
        let coord = ctx.ident_generator.next_output();
        let old_color = ctx.ident_generator.next_output();

        Ok(format!(
            r#"
            var {layer} = 0u;
            var {coord} = vec2u(0u);
            convert_pixel_to_tile(pixel_pos, &{layer}, &{coord});
            let {old_color} = image::texture_unpack::unpack_rgba8_texel(textureLoad(outputs[{layer}], {coord}));
            textureStore(outputs[{layer}], {coord}, image::texture_unpack::pack_rgba8_texel(image::blend_modes::{}({}, {old_color})));
            "#,
            state.blend_mode.shader_func(),
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
