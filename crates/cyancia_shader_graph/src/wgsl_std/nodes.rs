use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use bevy_math::{Rect, VectorSpace};
use cyancia_assets::asset::AssetHandle;
use cyancia_math::curve::CubicCurve;
use cyancia_utils::{count, wrapper};
use cyancia_widgets::curve_edit::CurveEdit;
use glam::{Vec2, Vec3, Vec3Swizzles, Vec4};
use iced_core::{Color, Element, color};
use iced_widget::{Column, column, keyed::column, pick_list, space, text_input};
use indexmap::{IndexMap, map::Entry};
use parking_lot::{RwLock, RwLockReadGuard};
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    GraphRenderer, GraphTheme,
    editor::{
        NODE_WIDTH,
        slot::{input_slot, output_slot},
    },
    graph::{
        Graph, GraphData, GraphVarIdentGenerator,
        external::{ExternalVariableId, generate_external_variable_name},
        function::GraphFunctionId,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeInputsViewContext, GraphNodeOutputsViewContext, GraphNodeRunContext,
            GraphNodeUpdateContext, GraphNodeUpdateSignatureContext, StatelessCommonGraphNode,
        },
        slot::{
            ErasedGraphLiteralUpdateMessage, ErasedGraphValueType, GraphDefaultInputSlot,
            GraphDefaultOutputSlot,
        },
        texture::TextureId,
        variable::GraphTypeRegistry,
    },
    save::GraphSerializable,
    wgsl_std::types::{ColorType, F32Type, RectType, TextureReference, TextureType, Vec2FType},
};

use crate::graph::node::GraphNodeRunError;
use cyancia_shader_graph_derive::stateless;

#[derive(Default, Clone)]
pub struct ScalarMathNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScalarMathNodeMode {
    Add,
    Subtract,
    Multiply,
    Divide,
    Acos,
    Acosh,
    Asin,
    Asinh,
    Atan,
    Atanh,
    Ceil,
    Cos,
    Cosh,
    Degrees,
    Exp,
    Exp2,
    Floor,
    Fract,
    InverseSqrt,
    Ln,
    Log2,
    Max,
    Min,
    Pow,
    Radians,
    Round,
    Saturate,
    Sign,
    Sin,
    Sinh,
    Sqrt,
    Tan,
    Tanh,
    Trunc,
}

impl ScalarMathNodeMode {
    pub const ALL: [ScalarMathNodeMode; 34] = [
        ScalarMathNodeMode::Add,
        ScalarMathNodeMode::Subtract,
        ScalarMathNodeMode::Multiply,
        ScalarMathNodeMode::Divide,
        ScalarMathNodeMode::Acos,
        ScalarMathNodeMode::Acosh,
        ScalarMathNodeMode::Asin,
        ScalarMathNodeMode::Asinh,
        ScalarMathNodeMode::Atan,
        ScalarMathNodeMode::Atanh,
        ScalarMathNodeMode::Ceil,
        ScalarMathNodeMode::Cos,
        ScalarMathNodeMode::Cosh,
        ScalarMathNodeMode::Degrees,
        ScalarMathNodeMode::Exp,
        ScalarMathNodeMode::Exp2,
        ScalarMathNodeMode::Floor,
        ScalarMathNodeMode::Fract,
        ScalarMathNodeMode::InverseSqrt,
        ScalarMathNodeMode::Ln,
        ScalarMathNodeMode::Log2,
        ScalarMathNodeMode::Max,
        ScalarMathNodeMode::Min,
        ScalarMathNodeMode::Pow,
        ScalarMathNodeMode::Radians,
        ScalarMathNodeMode::Round,
        ScalarMathNodeMode::Saturate,
        ScalarMathNodeMode::Sign,
        ScalarMathNodeMode::Sin,
        ScalarMathNodeMode::Sinh,
        ScalarMathNodeMode::Sqrt,
        ScalarMathNodeMode::Tan,
        ScalarMathNodeMode::Tanh,
        ScalarMathNodeMode::Trunc,
    ];

    pub fn operands_names(&self) -> &[&'static str] {
        match self {
            ScalarMathNodeMode::Add => &["A", "B"],
            ScalarMathNodeMode::Subtract => &["Minuend", "Subtrahend"],
            ScalarMathNodeMode::Multiply => &["A", "B"],
            ScalarMathNodeMode::Divide => &["Dividend", "Divisor"],
            ScalarMathNodeMode::Acos => &["X"],
            ScalarMathNodeMode::Acosh => &["X"],
            ScalarMathNodeMode::Asin => &["X"],
            ScalarMathNodeMode::Asinh => &["X"],
            ScalarMathNodeMode::Atan => &["X"],
            ScalarMathNodeMode::Atanh => &["X"],
            ScalarMathNodeMode::Ceil => &["X"],
            ScalarMathNodeMode::Cos => &["X"],
            ScalarMathNodeMode::Cosh => &["X"],
            ScalarMathNodeMode::Degrees => &["X"],
            ScalarMathNodeMode::Exp => &["X"],
            ScalarMathNodeMode::Exp2 => &["X"],
            ScalarMathNodeMode::Floor => &["X"],
            ScalarMathNodeMode::Fract => &["X"],
            ScalarMathNodeMode::InverseSqrt => &["X"],
            ScalarMathNodeMode::Ln => &["X"],
            ScalarMathNodeMode::Log2 => &["X"],
            ScalarMathNodeMode::Max => &["A", "B"],
            ScalarMathNodeMode::Min => &["A", "B"],
            ScalarMathNodeMode::Pow => &["Base", "Exponent"],
            ScalarMathNodeMode::Radians => &["X"],
            ScalarMathNodeMode::Round => &["X"],
            ScalarMathNodeMode::Saturate => &["X"],
            ScalarMathNodeMode::Sign => &["X"],
            ScalarMathNodeMode::Sin => &["X"],
            ScalarMathNodeMode::Sinh => &["X"],
            ScalarMathNodeMode::Sqrt => &["X"],
            ScalarMathNodeMode::Tan => &["X"],
            ScalarMathNodeMode::Tanh => &["X"],
            ScalarMathNodeMode::Trunc => &["X"],
        }
    }
}

impl ToString for ScalarMathNodeMode {
    fn to_string(&self) -> String {
        match self {
            ScalarMathNodeMode::Add => "Add",
            ScalarMathNodeMode::Subtract => "Subtract",
            ScalarMathNodeMode::Multiply => "Multiply",
            ScalarMathNodeMode::Divide => "Divide",
            ScalarMathNodeMode::Acos => "Acos",
            ScalarMathNodeMode::Acosh => "Acosh",
            ScalarMathNodeMode::Asin => "Asin",
            ScalarMathNodeMode::Asinh => "Asinh",
            ScalarMathNodeMode::Atan => "Atan",
            ScalarMathNodeMode::Atanh => "Atanh",
            ScalarMathNodeMode::Ceil => "Ceil",
            ScalarMathNodeMode::Cos => "Cos",
            ScalarMathNodeMode::Cosh => "Cosh",
            ScalarMathNodeMode::Degrees => "Degrees",
            ScalarMathNodeMode::Exp => "Exp",
            ScalarMathNodeMode::Exp2 => "Exp2",
            ScalarMathNodeMode::Floor => "Floor",
            ScalarMathNodeMode::Fract => "Fract",
            ScalarMathNodeMode::InverseSqrt => "Inverse Sqrt",
            ScalarMathNodeMode::Ln => "Ln",
            ScalarMathNodeMode::Log2 => "Log2",
            ScalarMathNodeMode::Max => "Max",
            ScalarMathNodeMode::Min => "Min",
            ScalarMathNodeMode::Pow => "Pow",
            ScalarMathNodeMode::Radians => "Radians",
            ScalarMathNodeMode::Round => "Round",
            ScalarMathNodeMode::Saturate => "Saturate",
            ScalarMathNodeMode::Sign => "Sign",
            ScalarMathNodeMode::Sin => "Sin",
            ScalarMathNodeMode::Sinh => "Sinh",
            ScalarMathNodeMode::Sqrt => "Sqrt",
            ScalarMathNodeMode::Tan => "Tan",
            ScalarMathNodeMode::Tanh => "Tanh",
            ScalarMathNodeMode::Trunc => "Trunc",
        }
        .to_string()
    }
}

#[derive(Clone)]
pub enum ScalarMathNodeMessage {
    ModeChanged(ScalarMathNodeMode),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for ScalarMathNode {
    type State = ScalarMathNodeMode;
    type Message = ScalarMathNodeMessage;

    fn name(&self) -> &'static str {
        "Scalar Math"
    }

    fn default_state(&self) -> Self::State {
        ScalarMathNodeMode::Add
    }

    fn header_color(&self) -> Color {
        color!(0x90be6d)
    }

    fn create_inputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = Column::new().spacing(2);
        column = column.push(pick_list(
            ScalarMathNodeMode::ALL,
            Some(*state),
            ScalarMathNodeMessage::ModeChanged,
        ));

        for (i, slot_name) in state.operands_names().iter().enumerate() {
            if let Some(elem) = ctx.view_input(slot_name, i) {
                column = column.push(elem.map(|m| ScalarMathNodeMessage::LiteralUpdate(m)));
            }
        }

        column.spacing(2).into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = Column::new().spacing(2);

        if let Some(elem) = ctx.view_output("Result", 0) {
            column = column.push(elem.map(|m| ScalarMathNodeMessage::LiteralUpdate(m)));
        }

        column.into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            ScalarMathNodeMessage::ModeChanged(mode) => {
                *state = mode;
            }
            ScalarMathNodeMessage::LiteralUpdate(msg) => {
                ctx.update_literal(msg);
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_a = ctx.get_input(0)?;
        let input_b = ctx.get_input(1)?;
        let input_c = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = {};\n",
            output,
            match state {
                ScalarMathNodeMode::Add => format!("{} + {}", input_a, input_b),
                ScalarMathNodeMode::Subtract => format!("{} - {}", input_a, input_b),
                ScalarMathNodeMode::Multiply => format!("{} * {}", input_a, input_b),
                ScalarMathNodeMode::Divide => format!("{} / {}", input_a, input_b),
                ScalarMathNodeMode::Acos => format!("acos({})", input_a),
                ScalarMathNodeMode::Acosh => format!("acosh({})", input_a),
                ScalarMathNodeMode::Asin => format!("asin({})", input_a),
                ScalarMathNodeMode::Asinh => format!("asinh({})", input_a),
                ScalarMathNodeMode::Atan => format!("atan({})", input_a),
                ScalarMathNodeMode::Atanh => format!("atanh({})", input_a),
                ScalarMathNodeMode::Ceil => format!("ceil({})", input_a),
                ScalarMathNodeMode::Cos => format!("cos({})", input_a),
                ScalarMathNodeMode::Cosh => format!("cosh({})", input_a),
                ScalarMathNodeMode::Degrees => format!("degrees({})", input_a),
                ScalarMathNodeMode::Exp => format!("exp({})", input_a),
                ScalarMathNodeMode::Exp2 => format!("exp2({})", input_a),
                ScalarMathNodeMode::Floor => format!("floor({})", input_a),
                ScalarMathNodeMode::Fract => format!("fract({})", input_a),
                ScalarMathNodeMode::InverseSqrt => format!("inverseSqrt({})", input_a),
                ScalarMathNodeMode::Ln => format!("log({})", input_a),
                ScalarMathNodeMode::Log2 => format!("log2({})", input_a),
                ScalarMathNodeMode::Max => format!("max({}, {})", input_a, input_b),
                ScalarMathNodeMode::Min => format!("min({}, {})", input_a, input_b),
                ScalarMathNodeMode::Pow => format!("pow({}, {})", input_a, input_b),
                ScalarMathNodeMode::Radians => format!("radians({})", input_a),
                ScalarMathNodeMode::Round => format!("round({})", input_a),
                ScalarMathNodeMode::Saturate => format!("saturate({})", input_a),
                ScalarMathNodeMode::Sign => format!("sign({})", input_a),
                ScalarMathNodeMode::Sin => format!("sin({})", input_a),
                ScalarMathNodeMode::Sinh => format!("sinh({})", input_a),
                ScalarMathNodeMode::Sqrt => format!("sqrt({})", input_a),
                ScalarMathNodeMode::Tan => format!("tan({})", input_a),
                ScalarMathNodeMode::Tanh => format!("tanh({})", input_a),
                ScalarMathNodeMode::Trunc => format!("trunc({})", input_a),
            }
        ))
    }

    fn run(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        let a = ctx.get_input_value::<F32Type>(0)?;
        let b = ctx.get_input_value::<F32Type>(1)?;
        let c = ctx.get_input_value::<F32Type>(2)?;

        let result = match state {
            ScalarMathNodeMode::Add => a + b,
            ScalarMathNodeMode::Subtract => a - b,
            ScalarMathNodeMode::Multiply => a * b,
            ScalarMathNodeMode::Divide => a / b,
            ScalarMathNodeMode::Acos => a.acos(),
            ScalarMathNodeMode::Acosh => a.acosh(),
            ScalarMathNodeMode::Asin => a.asin(),
            ScalarMathNodeMode::Asinh => a.asinh(),
            ScalarMathNodeMode::Atan => a.atan(),
            ScalarMathNodeMode::Atanh => a.atanh(),
            ScalarMathNodeMode::Ceil => a.ceil(),
            ScalarMathNodeMode::Cos => a.cos(),
            ScalarMathNodeMode::Cosh => a.cosh(),
            ScalarMathNodeMode::Degrees => a.to_degrees(),
            ScalarMathNodeMode::Exp => a.exp(),
            ScalarMathNodeMode::Exp2 => a.exp2(),
            ScalarMathNodeMode::Floor => a.floor(),
            ScalarMathNodeMode::Fract => a.fract(),
            ScalarMathNodeMode::InverseSqrt => 1.0 / a.sqrt(),
            ScalarMathNodeMode::Ln => a.ln(),
            ScalarMathNodeMode::Log2 => a.log2(),
            ScalarMathNodeMode::Max => a.max(b),
            ScalarMathNodeMode::Min => a.min(b),
            ScalarMathNodeMode::Pow => a.powf(b),
            ScalarMathNodeMode::Radians => a.to_radians(),
            ScalarMathNodeMode::Round => a.round(),
            ScalarMathNodeMode::Saturate => a.clamp(0.0, 1.0),
            ScalarMathNodeMode::Sign => a.signum(),
            ScalarMathNodeMode::Sin => a.sin(),
            ScalarMathNodeMode::Sinh => a.sinh(),
            ScalarMathNodeMode::Sqrt => a.sqrt(),
            ScalarMathNodeMode::Tan => a.tan(),
            ScalarMathNodeMode::Tanh => a.tanh(),
            ScalarMathNodeMode::Trunc => a.trunc(),
        };

        ctx.set_output_value::<F32Type>(0, result)?;

        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct VectorMathNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VectorMathNodeMode {
    Add,
    Subtract,
    Multiply,
    Divide,
    Acos,
    Acosh,
    Asin,
    Asinh,
    Atan,
    Atanh,
    Ceil,
    Cos,
    Cosh,
    Degrees,
    Distance,
    Dot,
    Exp,
    Exp2,
    Floor,
    Fract,
    InverseSqrt,
    Ln,
    Length,
    Log2,
    Max,
    Min,
    Mix,
    Pow,
    Radians,
    Reflect,
    Round,
    Saturate,
    Sign,
    Sin,
    Sinh,
    Sqrt,
    Tan,
    Tanh,
    Trunc,
}

impl VectorMathNodeMode {
    pub const ALL: [VectorMathNodeMode; 39] = [
        VectorMathNodeMode::Add,
        VectorMathNodeMode::Subtract,
        VectorMathNodeMode::Multiply,
        VectorMathNodeMode::Divide,
        VectorMathNodeMode::Acos,
        VectorMathNodeMode::Acosh,
        VectorMathNodeMode::Asin,
        VectorMathNodeMode::Asinh,
        VectorMathNodeMode::Atan,
        VectorMathNodeMode::Atanh,
        VectorMathNodeMode::Ceil,
        VectorMathNodeMode::Cos,
        VectorMathNodeMode::Cosh,
        VectorMathNodeMode::Degrees,
        VectorMathNodeMode::Distance,
        VectorMathNodeMode::Dot,
        VectorMathNodeMode::Exp,
        VectorMathNodeMode::Exp2,
        VectorMathNodeMode::Floor,
        VectorMathNodeMode::Fract,
        VectorMathNodeMode::InverseSqrt,
        VectorMathNodeMode::Ln,
        VectorMathNodeMode::Length,
        VectorMathNodeMode::Log2,
        VectorMathNodeMode::Max,
        VectorMathNodeMode::Min,
        VectorMathNodeMode::Mix,
        VectorMathNodeMode::Pow,
        VectorMathNodeMode::Radians,
        VectorMathNodeMode::Reflect,
        VectorMathNodeMode::Round,
        VectorMathNodeMode::Saturate,
        VectorMathNodeMode::Sign,
        VectorMathNodeMode::Sin,
        VectorMathNodeMode::Sinh,
        VectorMathNodeMode::Sqrt,
        VectorMathNodeMode::Tan,
        VectorMathNodeMode::Tanh,
        VectorMathNodeMode::Trunc,
    ];

    pub fn operands_names(&self) -> &[&'static str] {
        match self {
            VectorMathNodeMode::Add => &["A", "B"],
            VectorMathNodeMode::Subtract => &["Minuend", "Subtrahend"],
            VectorMathNodeMode::Multiply => &["A", "B"],
            VectorMathNodeMode::Divide => &["Dividend", "Divisor"],
            VectorMathNodeMode::Acos => &["X"],
            VectorMathNodeMode::Acosh => &["X"],
            VectorMathNodeMode::Asin => &["X"],
            VectorMathNodeMode::Asinh => &["X"],
            VectorMathNodeMode::Atan => &["X"],
            VectorMathNodeMode::Atanh => &["X"],
            VectorMathNodeMode::Ceil => &["X"],
            VectorMathNodeMode::Cos => &["X"],
            VectorMathNodeMode::Cosh => &["X"],
            VectorMathNodeMode::Degrees => &["X"],
            VectorMathNodeMode::Distance => &["A", "B"],
            VectorMathNodeMode::Dot => &["A", "B"],
            VectorMathNodeMode::Exp => &["X"],
            VectorMathNodeMode::Exp2 => &["X"],
            VectorMathNodeMode::Floor => &["X"],
            VectorMathNodeMode::Fract => &["X"],
            VectorMathNodeMode::InverseSqrt => &["X"],
            VectorMathNodeMode::Ln => &["X"],
            VectorMathNodeMode::Length => &["X"],
            VectorMathNodeMode::Log2 => &["X"],
            VectorMathNodeMode::Max => &["A", "B"],
            VectorMathNodeMode::Min => &["A", "B"],
            VectorMathNodeMode::Mix => &["A", "B", "T"],
            VectorMathNodeMode::Pow => &["Base", "Exponent"],
            VectorMathNodeMode::Radians => &["X"],
            VectorMathNodeMode::Reflect => &["Incident", "Normal"],
            VectorMathNodeMode::Round => &["X"],
            VectorMathNodeMode::Saturate => &["X"],
            VectorMathNodeMode::Sign => &["X"],
            VectorMathNodeMode::Sin => &["X"],
            VectorMathNodeMode::Sinh => &["X"],
            VectorMathNodeMode::Sqrt => &["X"],
            VectorMathNodeMode::Tan => &["X"],
            VectorMathNodeMode::Tanh => &["X"],
            VectorMathNodeMode::Trunc => &["X"],
        }
    }
}

impl ToString for VectorMathNodeMode {
    fn to_string(&self) -> String {
        match self {
            VectorMathNodeMode::Add => "Add",
            VectorMathNodeMode::Subtract => "Subtract",
            VectorMathNodeMode::Multiply => "Multiply",
            VectorMathNodeMode::Divide => "Divide",
            VectorMathNodeMode::Acos => "Acos",
            VectorMathNodeMode::Acosh => "Acosh",
            VectorMathNodeMode::Asin => "Asin",
            VectorMathNodeMode::Asinh => "Asinh",
            VectorMathNodeMode::Atan => "Atan",
            VectorMathNodeMode::Atanh => "Atanh",
            VectorMathNodeMode::Ceil => "Ceil",
            VectorMathNodeMode::Cos => "Cos",
            VectorMathNodeMode::Cosh => "Cosh",
            VectorMathNodeMode::Degrees => "Degrees",
            VectorMathNodeMode::Distance => "Distance",
            VectorMathNodeMode::Dot => "Dot",
            VectorMathNodeMode::Exp => "Exp",
            VectorMathNodeMode::Exp2 => "Exp2",
            VectorMathNodeMode::Floor => "Floor",
            VectorMathNodeMode::Fract => "Fract",
            VectorMathNodeMode::InverseSqrt => "Inverse Sqrt",
            VectorMathNodeMode::Ln => "Ln",
            VectorMathNodeMode::Length => "Length",
            VectorMathNodeMode::Log2 => "Log2",
            VectorMathNodeMode::Max => "Max",
            VectorMathNodeMode::Min => "Min",
            VectorMathNodeMode::Mix => "Mix",
            VectorMathNodeMode::Pow => "Pow",
            VectorMathNodeMode::Radians => "Radians",
            VectorMathNodeMode::Reflect => "Reflect",
            VectorMathNodeMode::Round => "Round",
            VectorMathNodeMode::Saturate => "Saturate",
            VectorMathNodeMode::Sign => "Sign",
            VectorMathNodeMode::Sin => "Sin",
            VectorMathNodeMode::Sinh => "Sinh",
            VectorMathNodeMode::Sqrt => "Sqrt",
            VectorMathNodeMode::Tan => "Tan",
            VectorMathNodeMode::Tanh => "Tanh",
            VectorMathNodeMode::Trunc => "Trunc",
        }
        .to_string()
    }
}

#[derive(Clone)]
pub enum VectorMathNodeMessage {
    ModeChanged(VectorMathNodeMode),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for VectorMathNode {
    type State = VectorMathNodeMode;

    type Message = VectorMathNodeMessage;

    fn name(&self) -> &'static str {
        "Vector Math"
    }

    fn default_state(&self) -> Self::State {
        VectorMathNodeMode::Add
    }

    fn header_color(&self) -> Color {
        color!(0x79caf2)
    }

    fn create_inputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
        ]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        match state {
            VectorMathNodeMode::Add
            | VectorMathNodeMode::Subtract
            | VectorMathNodeMode::Multiply
            | VectorMathNodeMode::Divide
            | VectorMathNodeMode::Acos
            | VectorMathNodeMode::Acosh
            | VectorMathNodeMode::Asin
            | VectorMathNodeMode::Asinh
            | VectorMathNodeMode::Atan
            | VectorMathNodeMode::Atanh
            | VectorMathNodeMode::Ceil
            | VectorMathNodeMode::Cos
            | VectorMathNodeMode::Cosh
            | VectorMathNodeMode::Degrees
            | VectorMathNodeMode::Exp
            | VectorMathNodeMode::Exp2
            | VectorMathNodeMode::Floor
            | VectorMathNodeMode::Fract
            | VectorMathNodeMode::InverseSqrt
            | VectorMathNodeMode::Ln
            | VectorMathNodeMode::Log2
            | VectorMathNodeMode::Max
            | VectorMathNodeMode::Min
            | VectorMathNodeMode::Mix
            | VectorMathNodeMode::Pow
            | VectorMathNodeMode::Radians
            | VectorMathNodeMode::Reflect
            | VectorMathNodeMode::Round
            | VectorMathNodeMode::Saturate
            | VectorMathNodeMode::Sign
            | VectorMathNodeMode::Sin
            | VectorMathNodeMode::Sinh
            | VectorMathNodeMode::Sqrt
            | VectorMathNodeMode::Tan
            | VectorMathNodeMode::Tanh
            | VectorMathNodeMode::Trunc => vec![GraphDefaultOutputSlot::new::<Vec2FType>()],

            VectorMathNodeMode::Dot | VectorMathNodeMode::Distance | VectorMathNodeMode::Length => {
                vec![GraphDefaultOutputSlot::new::<F32Type>()]
            }
        }
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = Column::new().spacing(2);
        column = column.push(pick_list(
            VectorMathNodeMode::ALL,
            Some(*state),
            VectorMathNodeMessage::ModeChanged,
        ));

        for (i, slot_name) in state.operands_names().iter().enumerate() {
            if let Some(elem) = ctx.view_input(slot_name, i) {
                column = column.push(elem.map(VectorMathNodeMessage::LiteralUpdate));
            }
        }

        column.spacing(2).into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = Column::new().spacing(2);

        if let Some(elem) = ctx.view_output("Result", 0) {
            column = column.push(elem.map(VectorMathNodeMessage::LiteralUpdate));
        }

        column.into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            VectorMathNodeMessage::ModeChanged(mode) => {
                *state = mode;
            }
            VectorMathNodeMessage::LiteralUpdate(msg) => {
                ctx.update_literal(msg);
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_a = ctx.get_input(0)?;
        let input_b = ctx.get_input(1)?;
        let input_c = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = {};\n",
            output,
            match state {
                VectorMathNodeMode::Add => format!("{} + {}", input_a, input_b),
                VectorMathNodeMode::Subtract => format!("{} - {}", input_a, input_b),
                VectorMathNodeMode::Multiply => format!("{} * {}", input_a, input_b),
                VectorMathNodeMode::Divide => format!("{} / {}", input_a, input_b),
                VectorMathNodeMode::Acos => format!("acos({})", input_a),
                VectorMathNodeMode::Acosh => format!("acosh({})", input_a),
                VectorMathNodeMode::Asin => format!("asin({})", input_a),
                VectorMathNodeMode::Asinh => format!("asinh({})", input_a),
                VectorMathNodeMode::Atan => format!("atan({})", input_a),
                VectorMathNodeMode::Atanh => format!("atanh({})", input_a),
                VectorMathNodeMode::Ceil => format!("ceil({})", input_a),
                VectorMathNodeMode::Cos => format!("cos({})", input_a),
                VectorMathNodeMode::Cosh => format!("cosh({})", input_a),
                VectorMathNodeMode::Degrees => format!("degrees({})", input_a),
                VectorMathNodeMode::Distance => format!("distance({}, {})", input_a, input_b),
                VectorMathNodeMode::Dot => format!("dot({}, {})", input_a, input_b),
                VectorMathNodeMode::Exp => format!("exp({})", input_a),
                VectorMathNodeMode::Exp2 => format!("exp2({})", input_a),
                VectorMathNodeMode::Floor => format!("floor({})", input_a),
                VectorMathNodeMode::Fract => format!("fract({})", input_a),
                VectorMathNodeMode::InverseSqrt => format!("inverseSqrt({})", input_a),
                VectorMathNodeMode::Ln => format!("log({})", input_a),
                VectorMathNodeMode::Length => format!("length({})", input_a),
                VectorMathNodeMode::Log2 => format!("log2({})", input_a),
                VectorMathNodeMode::Max => format!("max({}, {})", input_a, input_b),
                VectorMathNodeMode::Min => format!("min({}, {})", input_a, input_b),
                VectorMathNodeMode::Mix => format!("mix({}, {}, {})", input_a, input_b, input_c),
                VectorMathNodeMode::Pow => format!("pow({}, {})", input_a, input_b),
                VectorMathNodeMode::Radians => format!("radians({})", input_a),
                VectorMathNodeMode::Reflect => format!("reflect({}, {})", input_a, input_b),
                VectorMathNodeMode::Round => format!("round({})", input_a),
                VectorMathNodeMode::Saturate => format!("saturate({})", input_a),
                VectorMathNodeMode::Sign => format!("sign({})", input_a),
                VectorMathNodeMode::Sin => format!("sin({})", input_a),
                VectorMathNodeMode::Sinh => format!("sinh({})", input_a),
                VectorMathNodeMode::Sqrt => format!("sqrt({})", input_a),
                VectorMathNodeMode::Tan => format!("tan({})", input_a),
                VectorMathNodeMode::Tanh => format!("tanh({})", input_a),
                VectorMathNodeMode::Trunc => format!("trunc({})", input_a),
            }
        ))
    }

    fn run(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        let a = ctx.get_input_value::<Vec2FType>(0)?;
        let b = ctx.get_input_value::<Vec2FType>(1)?;
        let c = ctx.get_input_value::<Vec2FType>(2)?;

        match state {
            VectorMathNodeMode::Dot => {
                ctx.set_output_value::<F32Type>(0, a.dot(b))?;
            }
            VectorMathNodeMode::Distance => {
                ctx.set_output_value::<F32Type>(0, a.distance(b))?;
            }
            VectorMathNodeMode::Length => {
                ctx.set_output_value::<F32Type>(0, a.length())?;
            }
            _ => {
                let result = match state {
                    VectorMathNodeMode::Add => a + b,
                    VectorMathNodeMode::Subtract => a - b,
                    VectorMathNodeMode::Multiply => a * b,
                    VectorMathNodeMode::Divide => a / b,
                    VectorMathNodeMode::Acos => Vec2::new(a.x.acos(), a.y.acos()),
                    VectorMathNodeMode::Acosh => Vec2::new(a.x.acosh(), a.y.acosh()),
                    VectorMathNodeMode::Asin => Vec2::new(a.x.asin(), a.y.asin()),
                    VectorMathNodeMode::Asinh => Vec2::new(a.x.asinh(), a.y.asinh()),
                    VectorMathNodeMode::Atan => Vec2::new(a.x.atan(), a.y.atan()),
                    VectorMathNodeMode::Atanh => Vec2::new(a.x.atanh(), a.y.atanh()),
                    VectorMathNodeMode::Ceil => a.ceil(),
                    VectorMathNodeMode::Cos => Vec2::new(a.x.cos(), a.y.cos()),
                    VectorMathNodeMode::Cosh => Vec2::new(a.x.cosh(), a.y.cosh()),
                    VectorMathNodeMode::Degrees => Vec2::new(a.x.to_degrees(), a.y.to_degrees()),
                    VectorMathNodeMode::Exp => Vec2::new(a.x.exp(), a.y.exp()),
                    VectorMathNodeMode::Exp2 => Vec2::new(a.x.exp2(), a.y.exp2()),
                    VectorMathNodeMode::Floor => a.floor(),
                    VectorMathNodeMode::Fract => a.fract(),
                    VectorMathNodeMode::InverseSqrt => {
                        Vec2::new(1.0 / a.x.sqrt(), 1.0 / a.y.sqrt())
                    }
                    VectorMathNodeMode::Ln => Vec2::new(a.x.ln(), a.y.ln()),
                    VectorMathNodeMode::Log2 => Vec2::new(a.x.log2(), a.y.log2()),
                    VectorMathNodeMode::Max => a.max(b),
                    VectorMathNodeMode::Min => a.min(b),
                    VectorMathNodeMode::Mix => a.lerp(b, c.x),
                    VectorMathNodeMode::Pow => Vec2::new(a.x.powf(b.x), a.y.powf(b.y)),
                    VectorMathNodeMode::Radians => Vec2::new(a.x.to_radians(), a.y.to_radians()),
                    VectorMathNodeMode::Reflect => a - b * 2.0 * a.dot(b),
                    VectorMathNodeMode::Round => a.round(),
                    VectorMathNodeMode::Saturate => a.clamp(Vec2::ZERO, Vec2::ONE),
                    VectorMathNodeMode::Sign => Vec2::new(a.x.signum(), a.y.signum()),
                    VectorMathNodeMode::Sin => Vec2::new(a.x.sin(), a.y.sin()),
                    VectorMathNodeMode::Sinh => Vec2::new(a.x.sinh(), a.y.sinh()),
                    VectorMathNodeMode::Sqrt => Vec2::new(a.x.sqrt(), a.y.sqrt()),
                    VectorMathNodeMode::Tan => Vec2::new(a.x.tan(), a.y.tan()),
                    VectorMathNodeMode::Tanh => Vec2::new(a.x.tanh(), a.y.tanh()),
                    VectorMathNodeMode::Trunc => a.trunc(),
                    VectorMathNodeMode::Dot
                    | VectorMathNodeMode::Distance
                    | VectorMathNodeMode::Length => unreachable!(),
                };
                ctx.set_output_value::<Vec2FType>(0, result)?;
            }
        }

        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct RectMathNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum RectMathNodeMode {
    Union,
    Intersection,
    Inflate,
    Shrink,
}

#[derive(Clone)]
pub enum RectMathNodeMessage {
    ModeChanged(RectMathNodeMode),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for RectMathNode {
    type State = RectMathNodeMode;

    type Message = RectMathNodeMessage;

    fn name(&self) -> &'static str {
        "Rect Math"
    }

    fn default_state(&self) -> Self::State {
        RectMathNodeMode::Union
    }

    fn header_color(&self) -> Color {
        color!(0xe8638)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        match state {
            RectMathNodeMode::Union | RectMathNodeMode::Intersection => vec![
                GraphDefaultInputSlot::new::<RectType>(Rect::EMPTY),
                GraphDefaultInputSlot::new::<RectType>(Rect::EMPTY),
            ],
            RectMathNodeMode::Inflate | RectMathNodeMode::Shrink => {
                vec![
                    GraphDefaultInputSlot::new::<RectType>(Rect::EMPTY),
                    GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
                ]
            }
        }
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<RectType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let pick_list = pick_list(
            [
                RectMathNodeMode::Union,
                RectMathNodeMode::Intersection,
                RectMathNodeMode::Inflate,
                RectMathNodeMode::Shrink,
            ],
            Some(*state),
            RectMathNodeMessage::ModeChanged,
        );
        Column::new()
            .push(pick_list)
            .extend(ctx.view_all_inputs(
                match state {
                    RectMathNodeMode::Union | RectMathNodeMode::Intersection => &["A", "B"],
                    RectMathNodeMode::Inflate | RectMathNodeMode::Shrink => &["Rect", "Amount"],
                },
                RectMathNodeMessage::LiteralUpdate,
            ))
            .spacing(2)
            .into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Result"]))
            .spacing(2)
            .into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            RectMathNodeMessage::ModeChanged(mode) => {
                *state = mode;
            }
            RectMathNodeMessage::LiteralUpdate(msg) => {
                ctx.update_literal(msg);
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let a = ctx.get_input(0)?;
        let b = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = {};\n",
            output,
            match state {
                RectMathNodeMode::Union => {
                    format!("Rect(min({}.min, {}.min), max({}.max, {}.max))", a, b, a, b)
                }
                RectMathNodeMode::Intersection => {
                    format!("Rect(max({}.min, {}.min), min({}.max, {}.max))", a, b, a, b)
                }
                RectMathNodeMode::Inflate => {
                    let min = ctx.ident_generator.next_output();
                    let max = ctx.ident_generator.next_output();
                    format!(
                        "
                        let {min} = {a}.min - {b};
                        let {max} = {a}.max + {b};
                        var {output} = Rect({min} , {max});
                        if any({min} > {max}) {{
                            {output} = Rect(vec2f(1.0, -1.0));
                        }}
                    ",
                    )
                }
                RectMathNodeMode::Shrink => {
                    let min = ctx.ident_generator.next_output();
                    let max = ctx.ident_generator.next_output();
                    format!(
                        "
                        let {min} = {a}.min + {b};
                        let {max} = {a}.max - {b};
                        var {output} = Rect({min} , {max});
                        if any({min} > {max}) {{
                            {output} = Rect(vec2f(1.0, -1.0));
                        }}
                    ",
                    )
                }
            }
        ))
    }

    fn run(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        let result = match state {
            RectMathNodeMode::Union => {
                let a = ctx.get_input_value::<RectType>(0)?;
                let b = ctx.get_input_value::<RectType>(1)?;
                a.union(b)
            }
            RectMathNodeMode::Intersection => {
                let a = ctx.get_input_value::<RectType>(0)?;
                let b = ctx.get_input_value::<RectType>(1)?;
                a.intersect(b)
            }
            RectMathNodeMode::Inflate => {
                let rect = ctx.get_input_value::<RectType>(0)?;
                let amount = ctx.get_input_value::<Vec2FType>(1)?;
                let min = rect.min - amount;
                let max = rect.max + amount;
                if min.x > max.x || min.y > max.y {
                    Rect::EMPTY
                } else {
                    Rect { min, max }
                }
            }
            RectMathNodeMode::Shrink => {
                let rect = ctx.get_input_value::<RectType>(0)?;
                let amount = ctx.get_input_value::<Vec2FType>(1)?;
                let min = rect.min + amount;
                let max = rect.max - amount;
                if min.x > max.x || min.y > max.y {
                    Rect::EMPTY
                } else {
                    Rect { min, max }
                }
            }
        };

        ctx.set_output_value::<RectType>(0, result)?;

        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct TimeNode;

pub struct GraphTimes {
    pub now: f32,
    pub stroke_begin: f32,
}

pub trait GraphDataWithTime: GraphData {
    fn time(&self) -> GraphTimes;
    fn wgsl_variable() -> String;
}

#[stateless]
impl<Data: GraphDataWithTime> StatelessCommonGraphNode<Data> for TimeNode {
    fn name(&self) -> &'static str {
        "Time"
    }

    fn header_color(&self) -> Color {
        color!(0xf28482)
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
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
        ]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &[]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Now", "Stroke Begin"]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let now = ctx.get_output(0)?;
        let stroke_begin = ctx.get_output(1)?;
        let accessor = Data::wgsl_variable();

        Ok(format!(
            "
let {} = {}.now;
let {} = {}.stroke_begin;
                ",
            now, accessor, stroke_begin, accessor
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let time = ctx.data.time();
        ctx.set_output_value::<F32Type>(0, time.now)?;
        ctx.set_output_value::<F32Type>(1, time.stroke_begin)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct ClampNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for ClampNode {
    fn name(&self) -> &'static str {
        "Clamp"
    }

    fn header_color(&self) -> Color {
        color!(0x4cc9a3)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Value", "Min", "Max"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Result"]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_value = ctx.get_input(0)?;
        let input_min = ctx.get_input(1)?;
        let input_max = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = clamp({}, {}, {});\n",
            output, input_value, input_min, input_max
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let value = ctx.get_input_value::<F32Type>(0)?;
        let min = ctx.get_input_value::<F32Type>(1)?;
        let max = ctx.get_input_value::<F32Type>(2)?;
        ctx.set_output_value::<F32Type>(0, value.clamp(min, max))?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct StepNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for StepNode {
    fn name(&self) -> &'static str {
        "Step"
    }

    fn header_color(&self) -> Color {
        color!(0x9379f2)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Edge", "X"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Result"]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_edge = ctx.get_input(0)?;
        let input_x = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = step({}, {});\n",
            output, input_edge, input_x
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let edge = ctx.get_input_value::<F32Type>(0)?;
        let x = ctx.get_input_value::<F32Type>(1)?;
        ctx.set_output_value::<F32Type>(0, if x < edge { 0.0 } else { 1.0 })?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct SmoothStepNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for SmoothStepNode {
    fn name(&self) -> &'static str {
        "Smooth Step"
    }

    fn header_color(&self) -> Color {
        color!(0xe09d45)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Edge0", "Edge1", "X"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Result"]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_edge0 = ctx.get_input(0)?;
        let input_edge1 = ctx.get_input(1)?;
        let input_x = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = smoothstep({}, {}, {});\n",
            output, input_edge0, input_edge1, input_x
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let edge0 = ctx.get_input_value::<F32Type>(0)?;
        let edge1 = ctx.get_input_value::<F32Type>(1)?;
        let x = ctx.get_input_value::<F32Type>(2)?;
        let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
        ctx.set_output_value::<F32Type>(0, t * t * (3.0 - 2.0 * t))?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct SplitComponentsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for SplitComponentsNode {
    fn name(&self) -> &'static str {
        "Split Components"
    }

    fn header_color(&self) -> Color {
        color!(0x65b1c9)
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

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_vector = ctx.get_input(0)?;
        let output_x = ctx.get_output(0)?;
        let output_y = ctx.get_output(1)?;

        Ok(format!(
            "let {} = {}.x;\nlet {} = {}.y;\n",
            output_x, input_vector, output_y, input_vector
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let v = ctx.get_input_value::<Vec2FType>(0)?;
        ctx.set_output_value::<F32Type>(0, v.x)?;
        ctx.set_output_value::<F32Type>(1, v.y)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct CombineComponentsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for CombineComponentsNode {
    fn name(&self) -> &'static str {
        "Combine Components"
    }

    fn header_color(&self) -> Color {
        color!(0xf279a5)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["X", "Y"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Vector"]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_x = ctx.get_input(0)?;
        let input_y = ctx.get_input(1)?;
        let output_vector = ctx.get_output(0)?;

        Ok(format!(
            "let {} = vec2f({}, {});\n",
            output_vector, input_x, input_y
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let x = ctx.get_input_value::<F32Type>(0)?;
        let y = ctx.get_input_value::<F32Type>(1)?;
        ctx.set_output_value::<Vec2FType>(0, Vec2::new(x, y))?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct CombineColorComponentsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for CombineColorComponentsNode {
    fn name(&self) -> &'static str {
        "Combine Color Components"
    }

    fn header_color(&self) -> Color {
        color!(0xae79f2)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["R", "G", "B", "A"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_r = ctx.get_input(0)?;
        let input_g = ctx.get_input(1)?;
        let input_b = ctx.get_input(2)?;
        let input_a = ctx.get_input(3)?;
        let output_color = ctx.get_output(0)?;

        Ok(format!(
            "let {} = vec4f({}, {}, {}, {});\n",
            output_color, input_r, input_g, input_b, input_a
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let r = ctx.get_input_value::<F32Type>(0)?;
        let g = ctx.get_input_value::<F32Type>(1)?;
        let b = ctx.get_input_value::<F32Type>(2)?;
        let a = ctx.get_input_value::<F32Type>(3)?;
        ctx.set_output_value::<ColorType>(0, Vec4::new(r, g, b, a))?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct SplitColorComponentsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for SplitColorComponentsNode {
    fn name(&self) -> &'static str {
        "Split Color Components"
    }

    fn header_color(&self) -> Color {
        color!(0xa3f279)
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
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<F32Type>(),
        ]
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["R", "G", "B", "A"]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_color = ctx.get_input(0)?;
        let output_r = ctx.get_output(0)?;
        let output_g = ctx.get_output(1)?;
        let output_b = ctx.get_output(2)?;
        let output_a = ctx.get_output(3)?;

        Ok(format!(
            "let {} = {}.r;\nlet {} = {}.g;\nlet {} = {}.b;\nlet {} = {}.a;\n",
            output_r,
            input_color,
            output_g,
            input_color,
            output_b,
            input_color,
            output_a,
            input_color
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let c = ctx.get_input_value::<ColorType>(0)?;
        ctx.set_output_value::<F32Type>(0, c.x)?;
        ctx.set_output_value::<F32Type>(1, c.y)?;
        ctx.set_output_value::<F32Type>(2, c.z)?;
        ctx.set_output_value::<F32Type>(3, c.w)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct GetPixelColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for GetPixelColorNode {
    fn name(&self) -> &'static str {
        "Get Pixel Color"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        // TODO: sample modes
        &["Texture", "Position"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Color"]
    }

    fn header_color(&self) -> Color {
        color!(0xf279d1)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>(TextureReference::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
        ]
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
        let input_texture = ctx.get_input(0)?;
        let input_position = ctx.get_input(1)?;
        let output_color = ctx.get_output(0)?;

        Ok(format!(
            // TODO: sample_local_texture is only defined in `brush_template.wesl`
            "let {} = sample_local_texture({}, vec2u({}));\n",
            output_color, input_texture, input_position
        ))
    }
}

#[derive(Default, Clone)]
pub struct TextureNode;

#[derive(Clone)]
pub enum TextureNodeMessage {
    TextureChanged(TextureId),
}

impl<Data: GraphData> GraphNode<Data> for TextureNode {
    type State = TextureId;

    type Message = TextureNodeMessage;

    fn name(&self) -> &'static str {
        "Texture"
    }

    fn default_state(&self) -> Self::State {
        TextureId::NULL
    }

    fn header_color(&self) -> Color {
        color!(0xbd79f2)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<TextureType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        pick_list(
            ctx.resources
                .textures
                .all()
                .values()
                .cloned()
                .collect::<Vec<_>>(),
            ctx.resources.textures.get(state).cloned(),
            |t| TextureNodeMessage::TextureChanged(t.external_id),
        )
        .into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Texture"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            TextureNodeMessage::TextureChanged(id) => {
                *state = id;
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        // It's the external user's responsibility to generate the correct texture binding.
        // The binding should be a texture binding_array. The index of each used texture in graph
        // is corresponding to array index returned by TextureStorage::used_textures()
        let index = ctx.texture_usage.use_texture(*state);
        Ok(format!("let {} = {}u;\n", ctx.get_output(0)?, index))
    }

    fn run(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        ctx.set_output_value::<TextureType>(
            0,
            TextureReference {
                local_index: 0, // We are not using local index on CPU, so this can be 0.
                external_id: *state,
            },
        )?;
        Ok(())
    }
}

// TODO: Mixing in different color spaces.
#[derive(Default, Clone)]
pub struct ColorMixNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for ColorMixNode {
    fn name(&self) -> &'static str {
        "Color Mix"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Color A", "Color B", "Factor"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Result"]
    }

    fn header_color(&self) -> Color {
        color!(0x79caf2)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
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
        let input_color_a = ctx.get_input(0)?;
        let input_color_b = ctx.get_input(1)?;
        let input_factor = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = mix({}, {}, {});\n",
            output, input_color_a, input_color_b, input_factor
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let a = ctx.get_input_value::<ColorType>(0)?;
        let b = ctx.get_input_value::<ColorType>(1)?;
        let t = ctx.get_input_value::<F32Type>(2)?;
        ctx.set_output_value::<ColorType>(0, a.lerp(b, t))?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct TextureSizeNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for TextureSizeNode {
    fn name(&self) -> &'static str {
        "Texture Size"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Texture"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Size"]
    }

    fn header_color(&self) -> Color {
        color!(0xf2ab79)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<TextureType>(
            TextureReference::NULL,
        )]
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
        let input_texture = ctx.get_input(0)?;
        let output_size = ctx.get_output(0)?;

        Ok(format!(
            "let {} = vec2f(texture_bounds[{}].max - texture_bounds[{}].min);\n",
            output_size, input_texture, input_texture
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let reference = ctx.get_input_value::<TextureType>(0)?;
        let texture_object = ctx
            .resources
            .textures
            .get(&reference.external_id)
            .expect("Texture not found");
        let texture = texture_object.handle.get().expect("Unable to load texture");
        ctx.set_output_value::<Vec2FType>(
            0,
            Vec2::new(texture.image.width() as f32, texture.image.height() as f32),
        )?;
        Ok(())
    }
}

static UNIQUE_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Default, Clone)]
pub struct GraphFunctionNode;

#[derive(Clone)]
pub struct GraphFunctionReference {
    pub id: GraphFunctionId,
    pub name: String,
}

impl ToString for GraphFunctionReference {
    fn to_string(&self) -> String {
        self.name.clone()
    }
}

impl PartialEq for GraphFunctionReference {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Serialize, Deserialize)]
pub struct GraphFunctionNodeState {
    pub id: Option<GraphFunctionId>,
}

#[derive(Clone)]
pub enum GraphFunctionNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    FunctionChanged(GraphFunctionId),
}

impl<Data: GraphData> GraphNode<Data> for GraphFunctionNode {
    type State = GraphFunctionNodeState;

    type Message = GraphFunctionNodeMessage;

    fn name(&self) -> &'static str {
        "Function"
    }

    fn default_state(&self) -> Self::State {
        GraphFunctionNodeState { id: None }
    }

    fn header_color(&self) -> Color {
        color!(0xb379f2)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        let Some(func) = state
            .id
            .as_ref()
            .and_then(|id| ctx.resources.functions.get(id))
        else {
            return Vec::new();
        };

        func.graph
            .signature()
            .inputs
            .iter()
            .map(|(slot, var)| GraphDefaultInputSlot::new_boxed_default(var.ty().clone()))
            .collect()
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        let Some(func) = state
            .id
            .as_ref()
            .and_then(|id| ctx.resources.functions.get(id))
        else {
            return Vec::new();
        };

        func.graph
            .signature()
            .outputs
            .iter()
            .map(|(slot, var)| GraphDefaultOutputSlot::new_boxed(var.ty().clone()))
            .collect()
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let all_refs = ctx
            .resources
            .functions
            .all()
            .iter()
            .map(|(id, graph)| GraphFunctionReference {
                id: id.clone(),
                name: graph.name.clone(),
            })
            .collect::<Vec<_>>();
        let cur_ref = state.id.as_ref().and_then(|id| {
            ctx.resources
                .functions
                .get(id)
                .map(|f| GraphFunctionReference {
                    id: id.clone(),
                    name: f.name.clone(),
                })
        });

        let column = column![pick_list(all_refs, cur_ref, |r| {
            GraphFunctionNodeMessage::FunctionChanged(r.id)
        },)];
        let Some(func) = state
            .id
            .as_ref()
            .and_then(|id| ctx.resources.functions.get(id))
        else {
            return column.into();
        };

        let signature = func.graph.signature();
        let slots = ctx
            .all_inputs()
            .zip(signature.inputs.values())
            .map(|((id, slot), var)| {
                input_slot(*id, var.identifier().to_string(), slot)
                    .map(GraphFunctionNodeMessage::LiteralUpdate)
            })
            .collect::<Vec<_>>();

        column.extend(slots).into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let Some(func) = state
            .id
            .as_ref()
            .and_then(|id| ctx.resources.functions.get(id))
        else {
            return space().into();
        };

        let signature = func.graph.signature();
        let slots = ctx
            .all_outputs()
            .zip(signature.outputs.values())
            .map(|((id, slot), var)| output_slot(*id, var.identifier().to_string(), slot))
            .collect::<Vec<_>>();
        Column::with_children(slots).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            GraphFunctionNodeMessage::LiteralUpdate(m) => {
                ctx.update_literal(m);
            }
            GraphFunctionNodeMessage::FunctionChanged(id) => {
                state.id = Some(id);
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let Some(id) = state.id.as_ref() else {
            return Ok(Default::default());
        };
        let Some(func) = ctx.resources.functions.get(id) else {
            return Ok(Default::default());
        };

        let input_idents = (0..ctx.inputs.len()).try_fold(
            Vec::with_capacity(ctx.inputs.len()),
            |mut acc, i| {
                acc.push(ctx.get_input(i)?);
                Ok::<_, GraphNodeCodeGenError>(acc)
            },
        )?;

        let (output_idents, code) = func
            .graph
            .compile(
                input_idents,
                GraphVarIdentGenerator::new(format!(
                    "{}_{}",
                    id.to_string().replace('-', "_"),
                    UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
                )),
                ctx.texture_usage,
            )
            .map_err(|e| GraphNodeCodeGenError::Custom(e.into()))?;

        for (slot_id, output_ident) in ctx.outputs.iter().zip(output_idents) {
            ctx.output_slot_idents.insert(*slot_id, output_ident);
        }

        Ok(code)
    }

    fn run(
        &self,
        state: &Self::State,
        ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        let Some(id) = state.id.as_ref() else {
            return Ok(Default::default());
        };
        let Some(func) = ctx.resources.functions.get(id) else {
            return Ok(Default::default());
        };

        let input_values = (0..ctx.inputs.len()).try_fold(
            Vec::with_capacity(ctx.inputs.len()),
            |mut acc, i| {
                acc.push(ctx.get_input_value_raw(i)?.clone());
                Ok::<_, GraphNodeRunError>(acc)
            },
        )?;

        let output_values = func
            .graph
            .run(ctx.data, input_values)
            .map_err(|e| GraphNodeRunError::Custom(e.into()))?;

        for (slot_id, output_value) in ctx.outputs.iter().zip(output_values) {
            ctx.output_storage.insert(*slot_id, output_value);
        }

        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct GraphInputNode;

#[derive(Default)]
pub struct GraphInputNodeState {
    pub name: String,
    pub ty: Option<Box<dyn ErasedGraphValueType>>,
}

#[derive(Serialize, Deserialize)]
struct SerializableGraphInputNodeState {
    pub name: String,
    pub ty_name: Option<String>,
}

impl GraphSerializable for GraphInputNodeState {
    fn to_toml(&self) -> Result<toml::Value, toml::ser::Error> {
        toml::Value::try_from(SerializableGraphInputNodeState {
            name: self.name.clone(),
            ty_name: self.ty.as_ref().map(|t| t.name().to_string()),
        })
    }

    fn from_toml(
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<Self, toml::de::Error> {
        let de = SerializableGraphInputNodeState::deserialize(value)?;
        let ty = de.ty_name.map(|ty| {
            type_registry.get_type(&ty).ok_or_else(|| {
                <toml::de::Error as serde::de::Error>::custom(format!(
                    "Type '{}' not found in storage",
                    ty
                ))
            })
        });
        let ty = match ty {
            Some(ty) => Some(ty?),
            None => None,
        };

        Ok(GraphInputNodeState {
            name: de.name,
            ty: ty.map(|t| dyn_clone::clone_box(&**t)),
        })
    }
}

#[derive(Clone)]
pub enum GraphInputNodeMessage {
    VarNameChanged(String),
    TypeChanged(&'static str),
}

impl<Data: GraphData> GraphNode<Data> for GraphInputNode {
    type State = GraphInputNodeState;

    type Message = GraphInputNodeMessage;

    fn name(&self) -> &'static str {
        "Graph Input"
    }

    fn default_state(&self) -> Self::State {
        GraphInputNodeState::default()
    }

    fn header_color(&self) -> Color {
        color!(0x79f2c1)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        dbg!(state.ty.is_some());
        match &state.ty {
            Some(ty) => vec![
                // Comment that prevents ugly formatting
                GraphDefaultOutputSlot::new_boxed(dyn_clone::clone_box(&**ty)),
            ],
            None => vec![],
        }
    }

    fn update_signature(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeUpdateSignatureContext<'_, Data>,
    ) {
        ctx.require_output_slot_as_graph_input(0, state.name.clone());
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        column![
            text_input("Variable Name", &state.name)
                .on_input(GraphInputNodeMessage::VarNameChanged),
            pick_list(
                ctx.type_registry
                    .all_types()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                state.ty.as_ref().map(|t| t.name()),
                GraphInputNodeMessage::TypeChanged
            )
        ]
        .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            GraphInputNodeMessage::VarNameChanged(name) => state.name = name,
            GraphInputNodeMessage::TypeChanged(ty_name) => {
                state.ty = ctx.type_registry.get_type(ty_name).cloned();
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(Default::default())
    }
}

#[derive(Default, Clone)]
pub struct GraphOutputNode;

#[derive(Default)]
pub struct GraphOutputNodeState {
    pub name: String,
    pub ty: Option<Box<dyn ErasedGraphValueType>>,
}

#[derive(Serialize, Deserialize)]
struct SerializableGraphOutputNodeState {
    pub name: String,
    pub ty_name: Option<String>,
}

impl GraphSerializable for GraphOutputNodeState {
    fn to_toml(&self) -> Result<toml::Value, toml::ser::Error> {
        toml::Value::try_from(SerializableGraphOutputNodeState {
            name: self.name.clone(),
            ty_name: self.ty.as_ref().map(|t| t.name().to_string()),
        })
    }

    fn from_toml(
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<Self, toml::de::Error> {
        let de = SerializableGraphOutputNodeState::deserialize(value)?;
        let ty = de.ty_name.map(|ty| {
            type_registry.get_type(&ty).ok_or_else(|| {
                <toml::de::Error as serde::de::Error>::custom(format!(
                    "Type '{}' not found in storage",
                    ty
                ))
            })
        });
        let ty = match ty {
            Some(ty) => Some(ty?),
            None => None,
        };

        Ok(GraphOutputNodeState {
            name: de.name,
            ty: ty.map(|t| dyn_clone::clone_box(&**t)),
        })
    }
}

#[derive(Clone)]
pub enum GraphOutputNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    VarNameChanged(String),
    TypeChanged(&'static str),
}

impl<Data: GraphData> GraphNode<Data> for GraphOutputNode {
    type State = GraphOutputNodeState;

    type Message = GraphOutputNodeMessage;

    fn name(&self) -> &'static str {
        "Graph Output"
    }

    fn default_state(&self) -> Self::State {
        GraphOutputNodeState::default()
    }

    fn header_color(&self) -> Color {
        color!(0x79f2c1)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        match &state.ty {
            Some(ty) => vec![
                // Comment that prevents ugly formatting
                GraphDefaultInputSlot::new_boxed_default(dyn_clone::clone_box(&**ty)),
            ],
            None => vec![],
        }
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn update_signature(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeUpdateSignatureContext<'_, Data>,
    ) {
        ctx.require_input_slot_as_graph_output(0, state.name.clone());
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        column![
            Element::new(
                text_input("Variable Name", &state.name)
                    .on_input(GraphOutputNodeMessage::VarNameChanged),
            ),
            Element::new(pick_list(
                ctx.type_registry
                    .all_types()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                state.ty.as_ref().map(|t| t.name()),
                GraphOutputNodeMessage::TypeChanged,
            )),
        ]
        .extend(ctx.view_all_inputs(&["Value"], GraphOutputNodeMessage::LiteralUpdate))
        .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            GraphOutputNodeMessage::VarNameChanged(name) => state.name = name,
            GraphOutputNodeMessage::TypeChanged(ty_name) => {
                state.ty = ctx.type_registry.get_type(ty_name).cloned();
            }
            GraphOutputNodeMessage::LiteralUpdate(_) => unreachable!(),
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(Default::default())
    }
}

#[derive(Clone)]
pub struct ExternalVariableReference {
    pub id: ExternalVariableId,
    pub name: String,
}

impl PartialEq for ExternalVariableReference {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl ToString for ExternalVariableReference {
    fn to_string(&self) -> String {
        self.name.clone()
    }
}

#[derive(Default, Clone)]
pub struct ExternalVariableNode;

#[derive(Clone)]
pub enum ExternalVariableNodeMessage {
    IdChanged(ExternalVariableId),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for ExternalVariableNode {
    type State = Option<ExternalVariableId>;

    type Message = ExternalVariableNodeMessage;

    fn name(&self) -> &'static str {
        "External Variable"
    }

    fn header_color(&self) -> Color {
        color!(0x79c9f2)
    }

    fn create_inputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        if let Some(id) = state.as_ref() {
            match ctx.resources.external_vars.get(id) {
                Some(var) => vec![GraphDefaultOutputSlot::new_boxed(var.value.ty().clone())],
                None => {
                    let all = ctx
                        .resources
                        .external_vars
                        .all()
                        .iter()
                        .map(|entry| {
                            let v = entry.value();
                            format!("{}({})", v.name, v.id)
                        })
                        .collect::<Vec<_>>()
                        .join(", ");

                    log::error!(
                        "Selected external variable {} not found in storage: {}",
                        id,
                        all
                    );
                    vec![]
                }
            }
        } else {
            vec![]
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let id = state
            .as_ref()
            .ok_or(anyhow::anyhow!("No external literal selected"))?;
        let var = ctx.resources.external_vars.get(id).ok_or(anyhow::anyhow!(
            "Selected external literal not found in storage"
        ))?;
        let output = ctx.get_output(0)?;
        Ok(format!(
            "let {} = {};\n",
            output,
            generate_external_variable_name(&var)
        ))
    }

    fn default_state(&self) -> Self::State {
        None
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = column![];

        let vars = ctx.resources.external_vars.all();
        let refs = vars
            .iter()
            .map(|entry| ExternalVariableReference {
                id: entry.id.clone(),
                name: entry.name.clone(),
            })
            .collect::<Vec<_>>();
        let selected = state.as_ref().and_then(|id| {
            vars.get(id).map(|v| ExternalVariableReference {
                id: id.clone(),
                name: v.name.clone(),
            })
        });
        column = column.push(pick_list(refs, selected, |v| {
            ExternalVariableNodeMessage::IdChanged(v.id)
        }));

        column
            .extend(ctx.view_all_inputs(&["Var"], ExternalVariableNodeMessage::LiteralUpdate))
            .into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            ExternalVariableNodeMessage::IdChanged(id) => *state = Some(id),
            ExternalVariableNodeMessage::LiteralUpdate(m) => {
                ctx.update_literal(m);
            }
        }
    }

    fn run(
        &self,
        state: &Self::State,
        ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        let id = state
            .as_ref()
            .ok_or(anyhow::anyhow!("No external literal selected"))?;
        let var = ctx.resources.external_vars.get(id).ok_or(anyhow::anyhow!(
            "Selected external literal not found in storage"
        ))?;
        ctx.output_storage.insert(ctx.outputs[0], var.value.clone());
        Ok(())
    }
}

pub const CUBIC_CURVE_MAX_CONTROL_POINTS: usize = 16;

#[derive(Default, Clone)]
pub struct CurveNode;

#[derive(Serialize, Deserialize)]
pub struct CurveNodeState {
    pub control_points: Vec<Vec2>,
}

impl Default for CurveNodeState {
    fn default() -> Self {
        Self {
            control_points: vec![Vec2::ZERO, Vec2::ONE],
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SerializableCurveNodeState {
    pub tension: f32,
    pub control_points: Vec<Vec2>,
}

#[derive(Clone)]
pub enum CurveNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    CurvePointCreated(usize, Vec2),
    CurvePointMoved(usize, Vec2),
    CurvePointDeleted(usize),
}

impl<Data: GraphData> GraphNode<Data> for CurveNode {
    type State = CurveNodeState;

    type Message = CurveNodeMessage;

    fn name(&self) -> &'static str {
        "Curve"
    }

    fn default_state(&self) -> Self::State {
        Default::default()
    }

    fn header_color(&self) -> iced_core::Color {
        color!(0x799af2)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_inputs(&["X"], CurveNodeMessage::LiteralUpdate))
            .push(
                CurveEdit::new(CubicCurve::new(state.control_points.clone()))
                    .width(NODE_WIDTH)
                    .height(NODE_WIDTH * 0.75)
                    .on_point_created(CurveNodeMessage::CurvePointCreated)
                    .on_point_moved(CurveNodeMessage::CurvePointMoved)
                    .on_point_deleted(CurveNodeMessage::CurvePointDeleted),
            )
            .into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext<'_, Data>,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Y"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            CurveNodeMessage::LiteralUpdate(message) => {
                ctx.update_literal(message);
            }
            CurveNodeMessage::CurvePointCreated(index, position) => {
                state.control_points.insert(index, position);
            }
            CurveNodeMessage::CurvePointMoved(index, position) => {
                if let Some(point) = state.control_points.get_mut(index) {
                    *point = position;
                }
            }
            CurveNodeMessage::CurvePointDeleted(index) => {
                if state.control_points.len() > 2 {
                    state.control_points.remove(index);
                }
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let num_control_points = state.control_points.len();
        let mut control_points = state.control_points.clone();
        control_points.resize(CUBIC_CURVE_MAX_CONTROL_POINTS, Vec2::ZERO);
        let mut derivatives = CubicCurve::calculate_derivatives(&state.control_points);
        derivatives.resize(CUBIC_CURVE_MAX_CONTROL_POINTS + 1, 0.0);

        Ok(format!(
            "
let {} = package::render::math::sample_cubic_curve(
    package::render::math::CubicCurve(
        array<vec2f, {}>({}),
        array<f32, {}>({}),
        {}
    ),
    {}
);
            ",
            ctx.get_output(0)?,
            CUBIC_CURVE_MAX_CONTROL_POINTS,
            control_points
                .iter()
                .map(|p| format!("vec2({:.5}, {:.5})", p.x, p.y))
                .collect::<Vec<_>>()
                .join(", "),
            CUBIC_CURVE_MAX_CONTROL_POINTS + 1,
            derivatives
                .iter()
                .map(|d| format!("{:.5}", d))
                .collect::<Vec<_>>()
                .join(", "),
            num_control_points,
            ctx.get_input(0)?
        ))
    }

    fn run(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRunContext<'_, Data>,
    ) -> Result<(), GraphNodeRunError> {
        let x = ctx.get_input_value::<F32Type>(0)?;
        let y = CubicCurve::new(state.control_points.clone()).sample(x);
        ctx.set_output_value::<F32Type>(0, y)?;
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct RandomNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for RandomNode {
    fn name(&self) -> &'static str {
        "Random Number"
    }

    fn input_slot_names(&self) -> &[&'static str] {
        &["Seed"]
    }

    fn output_slot_names(&self) -> &[&'static str] {
        &["Scalar", "Vec2"]
    }

    fn header_color(&self) -> iced_core::Color {
        color!(0x79edf2)
    }

    fn create_inputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>(),
            GraphDefaultOutputSlot::new::<Vec2FType>(),
        ]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(format!(
            "let {} = package::render::hash::hash11({});\nlet {} = package::render::hash::hash21({});\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?,
            ctx.get_output(1)?,
            ctx.get_input(0)?
        ))
    }

    fn run(&self, mut ctx: GraphNodeRunContext<'_, Data>) -> Result<(), GraphNodeRunError> {
        let seed = ctx.get_input_value::<F32Type>(0)?;
        ctx.set_output_value::<F32Type>(0, Self::hash11(seed))?;
        ctx.set_output_value::<Vec2FType>(1, Self::hash21(seed))?;
        Ok(())
    }
}

impl RandomNode {
    pub fn hash11(mut p: f32) -> f32 {
        p = (p * 0.1031).fract();
        p *= p + 33.33;
        p *= p + p;
        p.fract()
    }

    pub fn hash21(p: f32) -> Vec2 {
        let mut p3 = (Vec3::splat(p) * Vec3::new(0.1031, 0.1030, 0.0973)).fract();
        p3 += p3.dot(p3.yzx() + Vec3::splat(33.33));
        ((Vec2::new(p3.x, p3.x) + Vec2::new(p3.y, p3.z)) * Vec2::new(p3.z, p3.y)).fract()
    }
}
