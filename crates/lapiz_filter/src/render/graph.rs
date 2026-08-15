use std::sync::{Arc, LazyLock};

use iced_core::Color;
use iced_widget::pick_list;
use lapiz_image::blend_modes::BlendMode;
use lapiz_shader_graph::{
    graph::{
        GraphData, GraphResources,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeDefaultStateContext, GraphNodeRegistry, GraphNodeUpdateContext,
            GraphNodeUpdateSignatureContext, GraphNodeViewContext, StatelessCommonGraphNode,
            stateless,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
        variable::GraphTypeRegistry,
    },
    wgsl_std::{
        builtin_nodes, builtin_types,
        types::{ColorType, F32Type, RectType, Vec2FType},
    },
};
use lapiz_utils::random_oklch;
use serde::{Deserialize, Serialize};

/// Graph data for a single filter shader group. Mirrors the plan's structure:
/// the shared graph resources (type / node registries, textures, functions,
/// external variables) are carried directly on the data.
#[derive(Default, Clone)]
pub struct FilterGraphData {
    pub resources: GraphResources<FilterGraphData>,
}

impl GraphData for FilterGraphData {}

/// Current pixel position in floating-point pixel coordinates.
#[derive(Default, Clone)]
pub struct PixelPositionNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for PixelPositionNode {
    fn name(&self) -> &'static str {
        "Pixel Position"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(PixelPositionNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>("Position".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("let {} = pixel_posf;", ctx.get_output(0)?))
    }
}

/// Color at the current pixel's position in the input (layer or previous group buffer).
#[derive(Default, Clone)]
pub struct InputColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for InputColorNode {
    fn name(&self) -> &'static str {
        "Input Color"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(InputColorNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = load_input_color(pixel_pos);",
            ctx.get_output(0)?
        ))
    }
}

/// Sample the input color at an arbitrary position.
#[derive(Default, Clone)]
pub struct SampleInputColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for SampleInputColorNode {
    fn name(&self) -> &'static str {
        "Sample Input Color"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(SampleInputColorNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>("Position".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let position = ctx.get_input(0)?;
        Ok(format!(
            "let {} = load_input_color(vec2i({}));",
            ctx.get_output(0)?,
            position
        ))
    }
}

/// The pixel rectangle of the current input, used by bounds evaluation.
#[derive(Default, Clone)]
pub struct InputBoundsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for InputBoundsNode {
    fn name(&self) -> &'static str {
        "Input Bounds"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(InputBoundsNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<RectType>("Rectangle".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!("let {} = input_bounds;", ctx.get_output(0)?))
    }
}

/// Writes the computed color to the output buffer for the current pixel.
#[derive(Default, Clone)]
pub struct OutputColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputColorNode {
    fn name(&self) -> &'static str {
        "Output Color"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(OutputColorNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>("Color".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        // set_output_color is a no-op during bounds evaluation (see template).
        Ok(format!(
            "set_output_color(pixel_pos, {});\n",
            ctx.get_input(0)?
        ))
    }
}

/// Sets the output pixel bounds (only takes effect during bounds evaluation).
#[derive(Default, Clone)]
pub struct OutputBoundsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for OutputBoundsNode {
    fn name(&self) -> &'static str {
        "Output Bounds"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(OutputBoundsNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<RectType>("Bounds".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
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
        // set_output_bounds is a no-op outside bounds evaluation (see template).
        Ok(format!("set_output_bounds({});", ctx.get_input(0)?))
    }
}

/// Value of the canvas selection mask at an arbitrary pixel position.
/// Mirrors `lapiz_brush::render::graph::SelectionMaskNode`.
#[derive(Default, Clone)]
pub struct SelectionMaskNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for SelectionMaskNode {
    fn name(&self) -> &'static str {
        "Selection Mask"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(SelectionMaskNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>("Position".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Value".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input = ctx.get_input(0)?;
        let output = ctx.get_output(0)?;
        Ok(format!(
            "let {output} = load_selection_mask_value(vec2i({input}));\n"
        ))
    }
}

/// Color of the original layer at an arbitrary pixel position.
/// Mirrors `lapiz_brush::render::graph::LayerPixelColorNode`.
#[derive(Default, Clone)]
pub struct LayerPixelColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for LayerPixelColorNode {
    fn name(&self) -> &'static str {
        "Layer Pixel Color"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(LayerPixelColorNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>("Position".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input = ctx.get_input(0)?;
        let output = ctx.get_output(0)?;
        Ok(format!(
            "let {output} = target_layer_color(vec2i({input}));\n"
        ))
    }
}

/// Blends a color over the original layer at the current pixel with the given
/// opacity. Mirrors `lapiz_brush::render::graph::BlendWithLayerNode`; this is
/// the replacement for the removed fixed `filter_blend.wesl` pass.
#[derive(Default, Clone)]
pub struct BlendWithLayerNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlendWithLayerNodeState {
    pub blend_mode: BlendMode,
}

#[derive(Clone)]
pub enum BlendWithLayerNodeMessage {
    ModeChanged(BlendMode),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for BlendWithLayerNode {
    type State = BlendWithLayerNodeState;
    type Message = BlendWithLayerNodeMessage;

    fn name(&self) -> &'static str {
        "Blend With Layer"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        BlendWithLayerNodeState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(BlendWithLayerNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Color".into()),
            GraphDefaultInputSlot::new::<F32Type>("Opacity".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> lapiz_shader_graph::GraphElement<'static, Self::Message> {
        ctx.view_all_slots_with_header(
            pick_list(
                BlendMode::ALL,
                Some(state.blend_mode),
                BlendWithLayerNodeMessage::ModeChanged,
            )
            .width(iced_core::Length::Fill),
            BlendWithLayerNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            BlendWithLayerNodeMessage::ModeChanged(mode) => state.blend_mode = mode,
            BlendWithLayerNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let opacity = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;
        Ok(format!(
            "let {output} = package::image::blend_modes::{}(vec4f({color}.rgb, {color}.a * {opacity}), target_layer_color(pixel_pos));\n",
            state.blend_mode.shader_func()
        ))
    }
}

/// Blends a color over the current group input at the current pixel with the
/// given opacity. Mirrors `lapiz_brush::render::graph::BlendWithInputNode`.
#[derive(Default, Clone)]
pub struct BlendWithInputNode;

#[derive(Clone, Serialize, Deserialize)]
pub struct BlendWithInputNodeState {
    pub blend_mode: BlendMode,
}

#[derive(Clone)]
pub enum BlendWithInputNodeMessage {
    ModeChanged(BlendMode),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for BlendWithInputNode {
    type State = BlendWithInputNodeState;
    type Message = BlendWithInputNodeMessage;

    fn name(&self) -> &'static str {
        "Blend With Input"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        BlendWithInputNodeState {
            blend_mode: BlendMode::Normal,
        }
    }

    fn header_color(&self, is_dark: bool) -> Color {
        random_oklch!(BlendWithInputNode, is_dark)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Color".into()),
            GraphDefaultInputSlot::new::<F32Type>("Opacity".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Color".into())]
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> lapiz_shader_graph::GraphElement<'static, Self::Message> {
        ctx.view_all_slots_with_header(
            pick_list(
                BlendMode::ALL,
                Some(state.blend_mode),
                BlendWithInputNodeMessage::ModeChanged,
            )
            .width(iced_core::Length::Fill),
            BlendWithInputNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            BlendWithInputNodeMessage::ModeChanged(mode) => state.blend_mode = mode,
            BlendWithInputNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let color = ctx.get_input(0)?;
        let opacity = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;
        Ok(format!(
            "let {output} = package::image::blend_modes::{}(vec4f({color}.rgb, {color}.a * {opacity}), current_input_color(pixel_pos));\n",
            state.blend_mode.shader_func()
        ))
    }
}

pub static FILTER_GRAPH_TYPES: LazyLock<Arc<GraphTypeRegistry>> =
    LazyLock::new(|| Arc::new(filter_graph_types()));
pub static FILTER_GRAPH_NODES: LazyLock<Arc<GraphNodeRegistry<FilterGraphData>>> =
    LazyLock::new(|| Arc::new(filter_graph_nodes()));

fn filter_graph_types() -> GraphTypeRegistry {
    let mut types = GraphTypeRegistry::default();
    types.merge(builtin_types());
    types
}

fn filter_graph_nodes() -> GraphNodeRegistry<FilterGraphData> {
    let mut nodes = GraphNodeRegistry::with_capacity();
    nodes.merge(builtin_nodes());

    nodes.register::<PixelPositionNode>();
    nodes.register::<InputColorNode>();
    nodes.register::<SampleInputColorNode>();
    nodes.register::<InputBoundsNode>();
    nodes.register::<OutputColorNode>();
    nodes.register::<OutputBoundsNode>();
    nodes.register::<SelectionMaskNode>();
    nodes.register::<LayerPixelColorNode>();
    nodes.register::<BlendWithLayerNode>();
    nodes.register::<BlendWithInputNode>();

    nodes
}
