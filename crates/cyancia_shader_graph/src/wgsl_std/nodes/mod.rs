use cyancia_utils::count;
use glam::Vec2;
use iced_core::{Color, Element, color};
use iced_widget::{Column, pick_list};
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    editor::slot::{SlotSide, valued_slot},
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreator,
            GraphNodeInputsViewContext, GraphNodeUpdateContext, StatelessCommonGraphNode,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::{F32Type, Vec2FType},
};

pub mod external;

#[derive(Default)]
pub struct ClampNode;

impl GraphNodeCreator for ClampNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        ClampNode
    }
}

impl StatelessCommonGraphNode for ClampNode {
    fn name(&self) -> &'static str {
        "Clamp"
    }

    fn header_color(&self) -> Color {
        color!(0x4cc9a3)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Value", "Min", "Max"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Result"]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let input_value = ctx.get_input(0)?;
        let input_min = ctx.get_input(1)?;
        let input_max = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = clamp({}, {}, {});",
            output, input_value, input_min, input_max
        ))
    }
}

#[derive(Default)]
pub struct StepNode;

impl GraphNodeCreator for StepNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        StepNode
    }
}

impl StatelessCommonGraphNode for StepNode {
    fn name(&self) -> &'static str {
        "Step"
    }

    fn header_color(&self) -> Color {
        color!(0x9379f2)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Edge", "X"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Result"]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let input_edge = ctx.get_input(0)?;
        let input_x = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = step({}, {});",
            output, input_edge, input_x
        ))
    }
}

#[derive(Default)]
pub struct SmoothStepNode;

impl GraphNodeCreator for SmoothStepNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        SmoothStepNode
    }
}

impl StatelessCommonGraphNode for SmoothStepNode {
    fn name(&self) -> &'static str {
        "Smooth Step"
    }

    fn header_color(&self) -> Color {
        color!(0xe09d45)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Edge0", "Edge1", "X"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Result"]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let input_edge0 = ctx.get_input(0)?;
        let input_edge1 = ctx.get_input(1)?;
        let input_x = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = smoothstep({}, {}, {});",
            output, input_edge0, input_edge1, input_x
        ))
    }
}

#[derive(Default)]
pub struct SplitComponentsNode;

impl GraphNodeCreator for SplitComponentsNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        SplitComponentsNode
    }
}

impl StatelessCommonGraphNode for SplitComponentsNode {
    fn name(&self) -> &'static str {
        "Split Components"
    }

    fn header_color(&self) -> Color {
        color!(0x65b1c9)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO)]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
        ]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Vector"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["X", "Y"]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let input_vector = ctx.get_input(0)?;
        let output_x = ctx.get_output(0)?;
        let output_y = ctx.get_output(1)?;

        Ok(format!(
            "let {} = {}.x;\nlet {} = {}.y;",
            output_x, input_vector, output_y, input_vector
        ))
    }
}

#[derive(Default)]
pub struct CombineComponentsNode;

impl GraphNodeCreator for CombineComponentsNode {
    type NodeType = Self;

    fn create(&self) -> Self::NodeType {
        CombineComponentsNode
    }
}

impl StatelessCommonGraphNode for CombineComponentsNode {
    fn name(&self) -> &'static str {
        "Combine Components"
    }

    fn header_color(&self) -> Color {
        color!(0xf279a5)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["X", "Y"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Vector"]
    }

    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError> {
        let input_x = ctx.get_input(0)?;
        let input_y = ctx.get_input(1)?;
        let output_vector = ctx.get_output(0)?;

        Ok(format!(
            "let {} = vec2f({}, {});",
            output_vector, input_x, input_y
        ))
    }
}
