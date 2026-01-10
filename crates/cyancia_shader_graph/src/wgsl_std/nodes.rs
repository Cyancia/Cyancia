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
            GraphNodeUpdateContext, GraphNodeViewContext, StatelessCommonGraphNode,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::{F32Type, Vec2FType},
};

macro_rules! unary_math {
    (
        $mode_name:ident, $node_name:ident, $message_name:ident, $slot_ty:ty, $slot_default:expr, $color:expr, $title:expr ;
        $default:ident,
        $(($mode:ident, $name:literal, $wgsl_fn:expr)),* $(,)?
    ) => {
        #[derive(Default)]
        pub struct $node_name;

        impl GraphNodeCreator for $node_name {
            type NodeType = Self;

            fn create(&self) -> Self::NodeType {
                $node_name
            }
        }

        #[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $mode_name {
            $(
                $mode,
            )*
        }

        impl ToString for $mode_name {
            fn to_string(&self) -> String {
                match self {
                    $(
                        $mode_name::$mode => $name,
                    )*
                }
                .to_string()
            }
        }

        impl $mode_name {
            pub const ALL: [$mode_name; count!($($mode)*)] = [
                $(
                    $mode_name::$mode,
                )*
            ];
        }

        #[derive(Clone)]
        pub enum $message_name {
            ModeChanged($mode_name),
            LiteralUpdate(ErasedGraphLiteralUpdateMessage),
        }

        impl GraphNode for $node_name {
            type State = $mode_name;

            type Message = $message_name;

            fn name(&self) -> &'static str {
                $title
            }

            fn default_state(&self) -> Self::State {
                $mode_name::$default
            }

            fn header_color(&self) -> Color {
                $color
            }

            fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
                vec![GraphDefaultInputSlot::new::<$slot_ty>("X", $slot_default)]
            }

            fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
                vec![GraphDefaultOutputSlot::new::<$slot_ty>("Result")]
            }

            fn view_body(
                &self,
                state: &Self::State,
                ctx: GraphNodeViewContext,
            ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
                let modes = pick_list(
                    $mode_name::ALL,
                    Some(*state),
                    $message_name::ModeChanged,
                );
                Column::with_children([modes.into()])
                    .extend(
                        ctx.view_all_inputs()
                            .into_iter()
                            .map(|e| e.map($message_name::LiteralUpdate)),
                    )
                    .spacing(2)
                    .into()
            }

            fn update_body(
                &self,
                state: &mut Self::State,
                message: Self::Message,
                mut ctx: GraphNodeUpdateContext,
            ) {
                match message {
                    $message_name::ModeChanged(new_mode) => {
                        *state = new_mode;
                    }
                    $message_name::LiteralUpdate(m) => {
                        ctx.update_literal(m);
                    }
                }
            }

            fn generate_code(
                &self,
                state: &Self::State,
                ctx: GraphNodeCodeGenContext,
            ) -> Result<String, GraphNodeCodeGenError> {
                let input = ctx.get_input(0)?;
                let output = ctx.get_output(0)?;
                let fn_name = match state {
                    $(
                        $mode_name::$mode => $wgsl_fn,
                    )*
                };

                Ok(format!("let {} = {}({});", output, fn_name, input))
            }
        }
    }
}
unary_math!(
    UnaryScalarMathMode, UnaryScalarMathNode, UnaryScalarMathMessage, F32Type, 0.0, color!(0xf28379), "Unary Scalar Math" ;
    Acos,
    (Acos, "Acos", "acos"),
    (Acosh, "Acosh", "acosh"),
    (Asin, "Asin", "asin"),
    (Asinh, "Asinh", "asinh"),
    (Atan, "Atan", "atan"),
    (Atanh, "Atanh", "atanh"),
    (Ceil, "Ceil", "ceil"),
    (Cos, "Cos", "cos"),
    (Cosh, "Cosh", "cosh"),
    (Exp, "Exp", "exp"),
    (Exp2, "Exp2", "exp2"),
    (Floor, "Floor", "floor"),
    (Fract, "Fract", "fract"),
    (Log, "Log", "log"),
    (Log2, "Log2", "log2"),
    (Radians, "Radians", "radians"),
    (Saturate, "Saturate", "saturate"),
    (Sign, "Sign", "sign"),
    (Sin, "Sin", "sin"),
    (Sinh, "Sinh", "sinh"),
    (Sqrt, "Sqrt", "sqrt"),
    (Tan, "Tan", "tan"),
    (Tanh, "Tanh", "tanh"),
    (Trunc, "Trunc", "trunc"),
);

unary_math!(
    UnaryVectorMathMode, UnaryVectorMathNode, UnaryVectorMathMessage, Vec2FType, Vec2::ZERO, color!(0xf279bb), "Unary Vector Math" ;
    Acos,
    (Acos, "Acos", "acos"),
    (Acosh, "Acosh", "acosh"),
    (Asin, "Asin", "asin"),
    (Asinh, "Asinh", "asinh"),
    (Atan, "Atan", "atan"),
    (Atanh, "Atanh", "atanh"),
    (Ceil, "Ceil", "ceil"),
    (Cos, "Cos", "cos"),
    (Cosh, "Cosh", "cosh"),
    (Exp, "Exp", "exp"),
    (Exp2, "Exp2", "exp2"),
    (Floor, "Floor", "floor"),
    (Fract, "Fract", "fract"),
    (Length, "Length", "length"),
    (Log, "Log", "log"),
    (Log2, "Log2", "log2"),
    (Normalize, "Normalize", "normalize"),
    (Radians, "Radians", "radians"),
    (Saturate, "Saturate", "saturate"),
    (Sign, "Sign", "sign"),
    (Sin, "Sin", "sin"),
    (Sinh, "Sinh", "sinh"),
    (Sqrt, "Sqrt", "sqrt"),
    (Tan, "Tan", "tan"),
    (Tanh, "Tanh", "tanh"),
    (Trunc, "Trunc", "trunc")
);

macro_rules! binary_math {
    (
        $mode_name:ident, $node_name:ident, $message_name:ident, $slot_ty:ty, $slot_default:expr, $color:expr, $title:expr ;
        $default:ident,
        $(($mode:ident, $name:literal, $wgsl_fn:literal, $input_a_name:literal, $input_b_name:literal)),* $(,)?
    ) => {
        #[derive(Default)]
        pub struct $node_name;

        impl GraphNodeCreator for $node_name {
            type NodeType = Self;

            fn create(&self) -> Self::NodeType {
                $node_name
            }
        }

        #[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $mode_name {
            $(
                $mode,
            )*
        }

        impl ToString for $mode_name {
            fn to_string(&self) -> String {
                match self {
                    $(
                        $mode_name::$mode => $name,
                    )*
                }
                .to_string()
            }
        }

        impl $mode_name {
            pub const ALL: [$mode_name; count!($($mode)*)] = [
                $(
                    $mode_name::$mode,
                )*
            ];
        }

        #[derive(Clone)]
        pub enum $message_name {
            ModeChanged($mode_name),
            LiteralUpdate(ErasedGraphLiteralUpdateMessage),
        }

        impl GraphNode for $node_name {
            type State = $mode_name;

            type Message = $message_name;

            fn name(&self) -> &'static str {
                $title
            }

            fn default_state(&self) -> Self::State {
                $mode_name::Add
            }

            fn header_color(&self) -> Color {
                $color
            }

            fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
                vec![
                    GraphDefaultInputSlot::new::<$slot_ty>("A", $slot_default),
                    GraphDefaultInputSlot::new::<$slot_ty>("B", $slot_default),
                ]
            }

            fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
                vec![GraphDefaultOutputSlot::new::<$slot_ty>("Result")]
            }

            fn view_body(
                &self,
                state: &Self::State,
                ctx: GraphNodeViewContext,
            ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
                let modes = pick_list($mode_name::ALL, Some(*state), $message_name::ModeChanged);
                let names = match state {
                    $(
                        $mode_name::$mode => [$input_a_name, $input_b_name],
                    )*
                };
                Column::with_children([modes.into()])
                    .extend(ctx.all_inputs().enumerate().map(|(i, (id, slot))| {
                        valued_slot(
                            (*id).into(),
                            slot.data.ty().color(),
                            names[i],
                            SlotSide::Left,
                            slot.data.ty().view_literal(*id, slot.data.value()),
                        )
                        .map($message_name::LiteralUpdate)
                    }))
                    .spacing(2)
                    .into()
            }

            fn update_body(
                &self,
                state: &mut Self::State,
                message: Self::Message,
                mut ctx: GraphNodeUpdateContext,
            ) {
                match message {
                    $message_name::ModeChanged(new_mode) => {
                        *state = new_mode;
                    }
                    $message_name::LiteralUpdate(m) => {
                        ctx.update_literal(m);
                    }
                }
            }

            fn generate_code(
                &self,
                state: &Self::State,
                ctx: GraphNodeCodeGenContext,
            ) -> Result<String, GraphNodeCodeGenError> {
                let input_a = ctx.get_input(0)?;
                let input_b = ctx.get_input(1)?;
                let output = ctx.get_output(0)?;

                Ok(match state {
                    $(
                        $mode_name::$mode => {
                            format!(
                                concat!("let {} = ", $wgsl_fn, ";"),
                                output, input_a, input_b
                            )
                        }
                    )*
                })
            }
        }
    };
}

binary_math!(
    BinaryScalarMathMode, BinaryScalarMathNode, BinaryScalarMathMessage, F32Type, 0.0, color!(0xb479f2), "Binary Scalar Math" ;
    Add,
    (Add, "Add", "{} + {}", "A", "B"),
    (Subtract, "Subtract", "{} - {}", "Minuend", "Subtrahend"),
    (Multiply, "Multiply", "{} * {}", "A", "B"),
    (Divide, "Divide", "{} / {}", "Dividend", "Divisor"),
    (Atan2, "Atan2", "atan2({}, {})", "Y", "X"),
    (Max, "Max", "max({}, {})", "A", "B"),
    (Min, "Min", "min({}, {})", "A", "B"),
    (Modulus, "Modulus", "modf({}, {})", "Dividend", "Divisor"),
    (Pow, "Pow", "pow({}, {})", "Base", "Exponent"),
);

binary_math!(
    BinaryVectorMathMode, BinaryVectorMathNode, BinaryVectorMathMessage, Vec2FType, Vec2::ZERO, color!(0x7987f2), "Binary Vector Math" ;
    Add,
    (Add, "Add", "{} + {}", "A", "B"),
    (Subtract, "Subtract", "{} - {}", "Minuend", "Subtrahend"),
    (Multiply, "Multiply", "{} * {}", "A", "B"),
    (Divide, "Divide", "{} / {}", "Dividend", "Divisor"),
    (Distance, "Distance", "distance({}, {})", "A", "B"),
    (Dot, "Dot", "dot({}, {})", "A", "B"),
    (Atan2, "Atan2", "atan2({}, {})", "Y", "X"),
    (Max, "Max", "max({}, {})", "A", "B"),
    (Min, "Min", "min({}, {})", "A", "B"),
    (Modulus, "Modulus", "modf({}, {})", "Dividend", "Divisor"),
    (Reflect, "Reflect", "reflect({}, {})", "Incident", "Normal"),
    (Pow, "Pow", "pow({}, {})", "Base", "Exponent"),
);

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
            GraphDefaultInputSlot::new::<F32Type>("Value", 0.0),
            GraphDefaultInputSlot::new::<F32Type>("Min", 0.0),
            GraphDefaultInputSlot::new::<F32Type>("Max", 1.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Result")]
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
            GraphDefaultInputSlot::new::<F32Type>("Edge", 0.0),
            GraphDefaultInputSlot::new::<F32Type>("X", 0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Result")]
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
            GraphDefaultInputSlot::new::<F32Type>("Edge0", 0.0),
            GraphDefaultInputSlot::new::<F32Type>("Edge1", 1.0),
            GraphDefaultInputSlot::new::<F32Type>("X", 0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Result")]
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
        vec![GraphDefaultInputSlot::new::<Vec2FType>(
            "Vector",
            Vec2::ZERO,
        )]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("X"),
            GraphDefaultOutputSlot::new::<F32Type>("Y"),
        ]
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
            GraphDefaultInputSlot::new::<F32Type>("X", 0.0),
            GraphDefaultInputSlot::new::<F32Type>("Y", 0.0),
        ]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>("Vector")]
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
