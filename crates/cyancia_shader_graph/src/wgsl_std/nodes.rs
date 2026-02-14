use cyancia_utils::count;
use glam::Vec2;
use iced_core::{Color, Element, color};
use iced_widget::{Column, pick_list};
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext, StatelessCommonGraphNode,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::{F32Type, Vec2FType},
};

macro_rules! impl_math_format {
    ($fmt:expr, $a:expr, $b:expr, ($one:literal)) => {
        format!($fmt, $a)
    };
    ($fmt:expr, $a:expr, $b:expr, ($one:literal, $two:literal)) => {
        format!($fmt, $a, $b)
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

            pub fn func_call(&self, input_a: &str, input_b: &str) -> String {
                match self {
                    $(
                        $node_mode_name::$op_name => {
                            impl_math_format!($func_call, input_a, input_b, ($($operands_name),*))
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
                let output = ctx.get_output(0)?;

                Ok(format!(
                    "let {} = {};\n",
                    output,
                    state.func_call(&input_a, &input_b)
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

math_node!(
    VectorMathNode,
    VectorMathNodeMode,
    VectorMathNodeMessage,
    "Vector Math",
    color!(0x79caf2),
    Vec2FType = Vec2::ZERO,
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
    Distance, "Distance" => ("distance({}, {})", "A", "B"),
    Dot, "Dot" => ("dot({}, {})", "A", "B"),
    Exp, "Exp" => ("exp({})", "X"),
    Exp2, "Exp2" => ("exp2({})", "X"),
    Floor, "Floor" => ("floor({})", "X"),
    Fract, "Fract" => ("fract({})", "X"),
    InverseSqrt, "Inverse Sqrt" => ("inverseSqrt({})", "X"),
    Ln, "Ln" => ("log({})", "X"),
    Length, "Length" => ("length({})", "X"),
    Log2, "Log2" => ("log2({})", "X"),
    Max, "Max" => ("max({}, {})", "A", "B"),
    Min, "Min" => ("min({}, {})", "A", "B"),
    Pow, "Pow" => ("pow({}, {})", "Base", "Exponent"),
    Radians, "Radians" => ("radians({})", "X"),
    Reflect, "Reflect" => ("reflect({}, {})", "Incident", "Normal"),
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
