use cyancia_utils::count;
use iced_core::{Color, Element, color};
use iced_widget::{Column, pick_list};
use serde::{Deserialize, Serialize};
use glam::Vec2;

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreator,
            GraphNodeUpdateContext, GraphNodeViewContext,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::{F32Type, Vec2FType},
};

macro_rules! unary_math {
    (
        $default:ident,
        $(($mode:ident, $name:literal, $wgsl_fn:expr)),* $(,)?
    ) => {
        unary_math!(
            UnaryScalarMathMode, UnaryScalarMathNode, UnaryScalarMathMessage, F32Type, 0.0, color!(0xf28379), "Unary Scalar Math" ;
            $default,
            $(($mode, $name, $wgsl_fn)),*
        );
        unary_math!(
            UnaryVectorMathMode, UnaryVectorMathNode, UnaryVectorMathMessage, Vec2FType, Vec2::ZERO, color!(0xf279bb), "Unary Vector Math" ;
            $default,
            $(($mode, $name, $wgsl_fn)),*
        );
    };

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
    (Trunc, "Trunc", "trunc")
);
