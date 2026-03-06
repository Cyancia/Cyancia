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
        let layer = ctx.ident_generator.next_output();
        let coord = ctx.ident_generator.next_output();

        Ok(format!(
            r#"
            var {layer} = 0u;
            var {coord} = vec2u(0u);
            convert_pixel_to_tile(pixel_pos, &{layer}, &{coord});
            textureStore(outputs[{layer}], {coord}, image::texture_unpack::pack_rgba8_texel({}));
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
