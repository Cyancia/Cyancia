use std::{collections::HashMap, sync::Arc};

use cyancia_utils::{count, wrapper};
use glam::{Vec2, Vec4};
use iced_core::{Color, Element, color};
use iced_widget::{Column, pick_list, space};
use indexmap::{IndexMap, map::Entry};
use parking_lot::{RwLock, RwLockReadGuard};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext, StatelessCommonGraphNode,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::{ColorType, F32Type, TextureLocalIndex, TextureType, Vec2FType},
};

macro_rules! impl_math_format {
    ($fmt:expr, $a:expr, $b:expr, $c:expr, ($one:literal)) => {
        format!($fmt, $a)
    };
    ($fmt:expr, $a:expr, $b:expr, $c:expr, ($one:literal, $two:literal)) => {
        format!($fmt, $a, $b)
    };
    ($fmt:expr, $a:expr, $b:expr, $c:expr, ($one:literal, $two:literal, $three:literal)) => {
        format!($fmt, $a, $b, $c)
    };
}

macro_rules! math_node {
    (
        $node_name:ident,
        $node_mode_name:ident,
        $node_message_name:ident,
        $node_title:literal,
        $header_color:expr,
        $slot_ty:ty = $slot_default:expr,
        $default_op:ident,
        $($op_name:ident, $op_str:literal => ($func_call:expr, $($operands_name:literal),* $(,)?)),* $(,)?
    ) => {
        #[derive(Default, Clone)]
        pub struct $node_name;

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $node_mode_name {
            $($op_name),*
        }

        impl $node_mode_name {
            pub const ALL: [$node_mode_name; count!($($op_name)*)] = [
                $($node_mode_name::$op_name),*
            ];

            pub fn operands_names(&self) -> &[&'static str] {
                match self {
                    $( $node_mode_name::$op_name => &[$($operands_name),*], )*
                }
            }

            #[allow(unused_variables)]
            pub fn func_call(&self, input_a: &str, input_b: &str, input_c: &str) -> String {
                match self {
                    $(
                        $node_mode_name::$op_name => {
                            impl_math_format!($func_call, input_a, input_b, input_c, ($($operands_name),*))
                        },
                    )*
                }
            }
        }

        impl ToString for $node_mode_name {
            fn to_string(&self) -> String {
                match self {
                    $( $node_mode_name::$op_name => $op_str, )*
                }
                .to_string()
            }
        }

        #[derive(Clone)]
        pub enum $node_message_name {
            ModeChanged($node_mode_name),
            LiteralUpdate(ErasedGraphLiteralUpdateMessage),
        }

        impl GraphNode for $node_name {
            type State = $node_mode_name;

            type Message = $node_message_name;

            fn name(&self) -> &'static str {
                $node_title
            }

            fn default_state(&self) -> Self::State {
                $node_mode_name::$default_op
            }

            fn header_color(&self) -> Color {
                $header_color
            }

            fn create_inputs(&self, _state: &Self::State) -> Vec<GraphDefaultInputSlot> {
                vec![
                    GraphDefaultInputSlot::new::<$slot_ty>($slot_default),
                    GraphDefaultInputSlot::new::<$slot_ty>($slot_default),
                    GraphDefaultInputSlot::new::<$slot_ty>($slot_default),
                ]
            }

            fn create_outputs(&self, _state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
                vec![GraphDefaultOutputSlot::new::<$slot_ty>()]
            }

            fn view_inputs(
                &self,
                state: &Self::State,
                ctx: GraphNodeInputsViewContext,
            ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
                let mut column = Column::new().spacing(2);
                column = column.push(pick_list(
                    $node_mode_name::ALL,
                    Some(*state),
                    $node_message_name::ModeChanged,
                ));

                for (i, slot_name) in state.operands_names().iter().enumerate() {
                    if let Some(elem) = ctx.view_input(slot_name, i) {
                        column = column.push(elem.map(|m| $node_message_name::LiteralUpdate(m)));
                    }
                }

                column.spacing(2).into()
            }

            fn view_outputs(
                &self,
                _state: &Self::State,
                ctx: GraphNodeOutputsViewContext,
            ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
                let mut column = Column::new().spacing(2);

                if let Some(elem) = ctx.view_output("Result", 0) {
                    column = column.push(elem.map(|m| $node_message_name::LiteralUpdate(m)));
                }

                column.into()
            }

            fn update(
                &self,
                state: &mut Self::State,
                message: Self::Message,
                mut ctx: GraphNodeUpdateContext,
            ) {
                match message {
                    $node_message_name::ModeChanged(mode) => {
                        *state = mode;
                    }
                    $node_message_name::LiteralUpdate(msg) => {
                        ctx.update_literal(msg);
                    }
                }
            }

            fn generate_code(
                &self,
                state: &Self::State,
                mut ctx: GraphNodeCodeGenContext,
            ) -> Result<String, GraphNodeCodeGenError> {
                let input_a = ctx.get_input(0)?;
                let input_b = ctx.get_input(1)?;
                let input_c = ctx.get_input(2)?;
                let output = ctx.get_output(0)?;

                Ok(format!(
                    "let {} = {};\n",
                    output,
                    state.func_call(&input_a, &input_b, &input_c)
                ))
            }
        }
    };
}

math_node!(
    ScalarMathNode,
    ScalarMathNodeMode,
    ScalarMathNodeMessage,
    "Scalar Math",
    color!(0x90be6d),
    F32Type = 0.0,
    Add,
    Add, "Add" => ("{} + {}", "A", "B"),
    Subtract, "Subtract" => ("{} - {}", "Minuend", "Subtrahend"),
    Multiply, "Multiply" => ("{} * {}", "A", "B"),
    Divide, "Divide" => ("{} / {}", "Dividend", "Divisor"),
    Acos, "Acos" => ("acos({})", "X"),
    Acosh, "Acosh" => ("acosh({})", "X"),
    Asin, "Asin" => ("asin({})", "X"),
    Asinh, "Asinh" => ("asinh({})", "X"),
    Atan, "Atan" => ("atan({})", "X"),
    Atanh, "Atanh" => ("atanh({})", "X"),
    Ceil, "Ceil" => ("ceil({})", "X"),
    Cos, "Cos" => ("cos({})", "X"),
    Cosh, "Cosh" => ("cosh({})", "X"),
    Degrees, "Degrees" => ("degrees({})", "X"),
    Exp, "Exp" => ("exp({})", "X"),
    Exp2, "Exp2" => ("exp2({})", "X"),
    Floor, "Floor" => ("floor({})", "X"),
    Fract, "Fract" => ("fract({})", "X"),
    InverseSqrt, "Inverse Sqrt" => ("inverseSqrt({})", "X"),
    Ln, "Ln" => ("log({})", "X"),
    Log2, "Log2" => ("log2({})", "X"),
    Max, "Max" => ("max({}, {})", "A", "B"),
    Min, "Min" => ("min({}, {})", "A", "B"),
    Pow, "Pow" => ("pow({}, {})", "Base", "Exponent"),
    Radians, "Radians" => ("radians({})", "X"),
    Round, "Round" => ("round({})", "X"),
    Saturate, "Saturate" => ("saturate({})", "X"),
    Sign, "Sign" => ("sign({})", "X"),
    Sin, "Sin" => ("sin({})", "X"),
    Sinh, "Sinh" => ("sinh({})", "X"),
    Sqrt, "Sqrt" => ("sqrt({})", "X"),
    Tan, "Tan" => ("tan({})", "X"),
    Tanh, "Tanh" => ("tanh({})", "X"),
    Trunc, "Trunc" => ("trunc({})", "X"),
);

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

impl GraphNode for VectorMathNode {
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

    fn create_inputs(&self, _state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
        ]
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
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
            | VectorMathNodeMode::Length
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

            VectorMathNodeMode::Dot | VectorMathNodeMode::Distance => {
                vec![GraphDefaultOutputSlot::new::<F32Type>()]
            }
        }
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        ctx: GraphNodeOutputsViewContext,
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
        mut ctx: GraphNodeUpdateContext,
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
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct ClampNode;

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

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct StepNode;

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

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_edge = ctx.get_input(0)?;
        let input_x = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = step({}, {});\n",
            output, input_edge, input_x
        ))
    }
}

#[derive(Default, Clone)]
pub struct SmoothStepNode;

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

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct SplitComponentsNode;

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

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_vector = ctx.get_input(0)?;
        let output_x = ctx.get_output(0)?;
        let output_y = ctx.get_output(1)?;

        Ok(format!(
            "let {} = {}.x;\nlet {} = {}.y;\n",
            output_x, input_vector, output_y, input_vector
        ))
    }
}

#[derive(Default, Clone)]
pub struct CombineComponentsNode;

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

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_x = ctx.get_input(0)?;
        let input_y = ctx.get_input(1)?;
        let output_vector = ctx.get_output(0)?;

        Ok(format!(
            "let {} = vec2f({}, {});\n",
            output_vector, input_x, input_y
        ))
    }
}

#[derive(Default, Clone)]
pub struct CombineColorComponentsNode;

impl StatelessCommonGraphNode for CombineColorComponentsNode {
    fn name(&self) -> &'static str {
        "Combine Color Components"
    }

    fn header_color(&self) -> Color {
        color!(0xae79f2)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
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
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct SplitColorComponentsNode;

impl StatelessCommonGraphNode for SplitColorComponentsNode {
    fn name(&self) -> &'static str {
        "Split Color Components"
    }

    fn header_color(&self) -> Color {
        color!(0xa3f279)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO)]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
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
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct GetPixelColorNode;

impl StatelessCommonGraphNode for GetPixelColorNode {
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

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>(TextureLocalIndex::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_texture = ctx.get_input(0)?;
        let input_position = ctx.get_input(1)?;
        let output_color = ctx.get_output(0)?;

        Ok(format!(
            "let {} = textureLoad(textures[{}], vec2u({}), 0);\n",
            output_color, input_texture, input_position
        ))
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub TextureId : Uuid
}

impl TextureId {
    // Null texture should have a default fallback, so they're also valid.
    pub const NULL: Self = Self(Uuid::nil());
}

#[derive(Clone)]
pub struct TextureObject {
    pub external_id: TextureId,
    pub name: String,
}

impl PartialEq for TextureObject {
    fn eq(&self, other: &Self) -> bool {
        self.external_id == other.external_id
    }
}

impl ToString for TextureObject {
    fn to_string(&self) -> String {
        self.name.clone()
    }
}

#[derive(Default)]
pub struct TextureStorage {
    inner: RwLock<HashMap<TextureId, TextureObject>>,
}

impl TextureStorage {
    pub fn new(map: Vec<TextureObject>) -> Self {
        Self {
            inner: RwLock::new(map.into_iter().map(|obj| (obj.external_id, obj)).collect()),
        }
    }

    pub fn insert(&self, object: TextureObject) {
        self.inner.write().insert(object.external_id, object);
    }

    pub fn get(&self, id: &TextureId) -> Option<TextureObject> {
        self.inner.read().get(id).cloned()
    }

    pub fn all(&self) -> RwLockReadGuard<'_, HashMap<TextureId, TextureObject>> {
        self.inner.read()
    }
}

// TODO: Is there any better way to compute local indices?
#[derive(Default)]
pub struct TextureUsageRecorder {
    inner: RwLock<IndexMap<TextureId, u32>>,
}

impl TextureUsageRecorder {
    pub fn use_texture(&self, id: TextureId) -> u32 {
        let mut inner = self.inner.write();
        let e = inner.entry(id);
        let local_index = e.index() as u32;
        e.and_modify(|index| *index += 1).or_insert(0);
        local_index
    }

    pub fn reset(&self) {
        let mut inner = self.inner.write();
        inner.clear();
        // We will always have at lease an empty texture.
        inner.insert(TextureId::NULL, 0);
    }

    pub fn get_usage(&self) -> RwLockReadGuard<'_, IndexMap<TextureId, u32>> {
        self.inner.read()
    }
}

#[derive(Clone)]
pub struct TextureNode {
    storage: Arc<TextureStorage>,
    recorder: Arc<TextureUsageRecorder>,
}

impl TextureNode {
    pub fn new(storage: Arc<TextureStorage>, recorder: Arc<TextureUsageRecorder>) -> Self {
        Self { storage, recorder }
    }
}

#[derive(Clone)]
pub enum TextureNodeMessage {
    TextureChanged(TextureId),
}

impl GraphNode for TextureNode {
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

    fn create_inputs(&self, state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<TextureType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        pick_list(
            self.storage.all().values().cloned().collect::<Vec<_>>(),
            self.storage.get(state),
            |t| TextureNodeMessage::TextureChanged(t.external_id),
        )
        .into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Texture"])).into()
    }

    fn update(&self, state: &mut Self::State, message: Self::Message, ctx: GraphNodeUpdateContext) {
        match message {
            TextureNodeMessage::TextureChanged(id) => {
                *state = id;
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        // It's the external user's responsibility to generate the correct texture binding.
        // The binding should be a texture binding_array. The index of each used texture in graph
        // is corresponding to array index returned by TextureStorage::used_textures()
        let index = self.recorder.use_texture(*state);
        dbg!(state, index);
        Ok(format!("let {} = {}u;\n", ctx.get_output(0)?, index))
    }
}

// TODO: Mixing in different color spaces.
#[derive(Default, Clone)]
pub struct ColorMixNode;

impl StatelessCommonGraphNode for ColorMixNode {
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

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct TextureSizeNode;

impl StatelessCommonGraphNode for TextureSizeNode {
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

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<TextureType>(
            TextureLocalIndex::NULL,
        )]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_texture = ctx.get_input(0)?;
        let output_size = ctx.get_output(0)?;

        Ok(format!(
            "let {} = vec2f(textureDimensions(textures[{}]));\n",
            output_size, input_texture
        ))
    }
}
