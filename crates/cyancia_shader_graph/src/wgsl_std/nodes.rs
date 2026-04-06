use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use cyancia_math::curve::CubicCurve;
use cyancia_utils::{count, wrapper};
use cyancia_widgets::curve_edit::CurveEdit;
use glam::{Vec2, Vec4};
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
        Graph, GraphVarIdentGenerator,
        external::{ExternalVariableId, generate_external_variable_name},
        function::GraphFunctionId,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeInputsViewContext, GraphNodeOutputsViewContext, GraphNodeUpdateContext,
            GraphNodeUpdateSignatureContext, StatelessCommonGraphNode,
        },
        slot::{
            ErasedGraphLiteralUpdateMessage, ErasedGraphValueType, GraphDefaultInputSlot,
            GraphDefaultOutputSlot,
        },
        texture::TextureId,
        variable::GraphTypeRegistry,
    },
    save::GraphSerializable,
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

            fn create_inputs(&self, _state: &Self::State, ctx: GraphNodeCreateSlotsContext,) -> Vec<GraphDefaultInputSlot> {
                vec![
                    GraphDefaultInputSlot::new::<$slot_ty>($slot_default),
                    GraphDefaultInputSlot::new::<$slot_ty>($slot_default),
                    GraphDefaultInputSlot::new::<$slot_ty>($slot_default),
                ]
            }

            fn create_outputs(&self, _state: &Self::State, ctx: GraphNodeCreateSlotsContext,) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
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
        ctx: GraphNodeCreateSlotsContext,
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO)]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
            GraphDefaultInputSlot::new::<F32Type>(1.0),
        ]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO)]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>(TextureLocalIndex::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>(Vec2::ZERO),
        ]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<TextureType>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        let index = ctx.texture_usage.use_texture(*state);
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<ColorType>(Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>(0.0),
        ]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
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

    fn create_inputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<TextureType>(
            TextureLocalIndex::NULL,
        )]
    }

    fn create_outputs(&self, _ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>()]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_texture = ctx.get_input(0)?;
        let output_size = ctx.get_output(0)?;

        Ok(format!(
            "let {} = vec2f(texture_bounds[{}].max - texture_bounds[{}].min);\n",
            output_size, input_texture, input_texture
        ))
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

impl GraphNode for GraphFunctionNode {
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
        ctx: GraphNodeCreateSlotsContext,
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
        ctx: GraphNodeCreateSlotsContext,
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
        ctx: GraphNodeInputsViewContext,
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
        ctx: GraphNodeOutputsViewContext,
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
        mut ctx: GraphNodeUpdateContext,
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
        ctx: GraphNodeCodeGenContext,
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

impl GraphNode for GraphInputNode {
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
        _ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext,
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

    fn update_signature(&self, state: &Self::State, mut ctx: GraphNodeUpdateSignatureContext) {
        ctx.require_output_slot_as_graph_input(0, state.name.clone());
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(&self, state: &mut Self::State, message: Self::Message, ctx: GraphNodeUpdateContext) {
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
        ctx: GraphNodeCodeGenContext,
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

impl GraphNode for GraphOutputNode {
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
        _ctx: GraphNodeCreateSlotsContext,
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
        _ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn update_signature(&self, state: &Self::State, mut ctx: GraphNodeUpdateSignatureContext) {
        ctx.require_input_slot_as_graph_output(0, state.name.clone());
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(&self, state: &mut Self::State, message: Self::Message, ctx: GraphNodeUpdateContext) {
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
        ctx: GraphNodeCodeGenContext,
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

impl GraphNode for ExternalVariableNode {
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
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
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
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let id = state
            .as_ref()
            .ok_or(anyhow::anyhow!("No external literal selected"))?;
        let var = ctx.resources.external_vars.get(id).ok_or(anyhow::anyhow!(
            "Selected external literal not found in storage"
        ))?;
        let output = ctx.get_output(0)?;
        // TODO: Use uniform buffer to transfer external variables into shader.
        //       For current architecture, everytime user modifies them, the whole shader needs to be recompiled.
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
        ctx: GraphNodeInputsViewContext,
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
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            ExternalVariableNodeMessage::IdChanged(id) => *state = Some(id),
            ExternalVariableNodeMessage::LiteralUpdate(m) => {
                ctx.update_literal(m);
            }
        }
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

impl GraphNode for CurveNode {
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
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>()]
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
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
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Y"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
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
        mut ctx: GraphNodeCodeGenContext,
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
}

#[derive(Default, Clone)]
pub struct RandomNode;

impl StatelessCommonGraphNode for RandomNode {
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

    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
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
            "let {} = package::render::hash::hash11({});\nlet {} = package::render::hash::hash21({});\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?,
            ctx.get_output(1)?,
            ctx.get_input(0)?
        ))
    }
}
