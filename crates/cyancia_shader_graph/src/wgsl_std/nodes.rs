use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use anyhow::anyhow;

use cyancia_math::curve::CubicCurve;
use cyancia_utils::wrapper;
use cyancia_widgets::{curve_edit::CurveEdit, fluent_builder::When, popover::Popover};
use glam::{Vec2, Vec3, Vec3Swizzles};
use iced_core::{Color, Length};
use iced_widget::{button, column, container, pick_list, row, text, text_input};
use indexmap::IndexMap;
use parking_lot::Mutex;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    GraphElement,
    graph::{
        Graph, GraphData, GraphResources, GraphVarIdentGenerator,
        external::{ExternalVariableId, generate_external_variable_name},
        function::GraphFunctionId,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeDefaultStateContext, GraphNodeRegistry, GraphNodeUpdateContext,
            GraphNodeUpdateSignatureContext, GraphNodeViewContext, StatelessCommonGraphNode,
        },
        slot::{
            ErasedGraphLiteralUpdateMessage, ErasedGraphValueType, GraphDefaultInputSlot,
            GraphDefaultOutputSlot, GraphValueType,
        },
        texture::TextureId,
    },
    save::{GraphSerializable, SerializableGraph},
    wgsl_std::{
        themed_color,
        types::{BoolType, ColorType, F32Type, RectType, TextureType, Vec2FType},
    },
};

use cyancia_shader_graph_derive::stateless;

#[derive(Default, Clone)]
pub struct ScalarMathNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
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
    Mix,
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
    pub const ALL: [ScalarMathNodeMode; 35] = [
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
        ScalarMathNodeMode::Mix,
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

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        ScalarMathNodeMode::Add
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(ScalarMathNode), is_dark)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        match state {
            ScalarMathNodeMode::Add | ScalarMathNodeMode::Max | ScalarMathNodeMode::Min => vec![
                GraphDefaultInputSlot::new::<F32Type>("A".into()),
                GraphDefaultInputSlot::new::<F32Type>("B".into()),
            ],
            ScalarMathNodeMode::Subtract => vec![
                GraphDefaultInputSlot::new::<F32Type>("Minuend".into()),
                GraphDefaultInputSlot::new::<F32Type>("Subtrahend".into()),
            ],
            ScalarMathNodeMode::Multiply => vec![
                GraphDefaultInputSlot::new::<F32Type>("A".into()),
                GraphDefaultInputSlot::new::<F32Type>("B".into()),
            ],
            ScalarMathNodeMode::Divide => vec![
                GraphDefaultInputSlot::new::<F32Type>("Dividend".into()),
                GraphDefaultInputSlot::new::<F32Type>("Divisor".into()),
            ],
            ScalarMathNodeMode::Pow => vec![
                GraphDefaultInputSlot::new::<F32Type>("Base".into()),
                GraphDefaultInputSlot::new::<F32Type>("Exponent".into()),
            ],
            ScalarMathNodeMode::Acosh => {
                vec![GraphDefaultInputSlot::new::<F32Type>("X".into())]
            }
            ScalarMathNodeMode::Mix => vec![
                GraphDefaultInputSlot::new::<F32Type>("A".into()),
                GraphDefaultInputSlot::new::<F32Type>("B".into()),
                GraphDefaultInputSlot::new::<F32Type>("Factor".into()),
            ],
            ScalarMathNodeMode::Ln
            | ScalarMathNodeMode::Log2
            | ScalarMathNodeMode::Sqrt
            | ScalarMathNodeMode::InverseSqrt => {
                vec![GraphDefaultInputSlot::new::<F32Type>("X".into())]
            }
            ScalarMathNodeMode::Acos
            | ScalarMathNodeMode::Asin
            | ScalarMathNodeMode::Asinh
            | ScalarMathNodeMode::Atan
            | ScalarMathNodeMode::Atanh
            | ScalarMathNodeMode::Ceil
            | ScalarMathNodeMode::Cos
            | ScalarMathNodeMode::Cosh
            | ScalarMathNodeMode::Degrees
            | ScalarMathNodeMode::Exp
            | ScalarMathNodeMode::Exp2
            | ScalarMathNodeMode::Floor
            | ScalarMathNodeMode::Fract
            | ScalarMathNodeMode::Radians
            | ScalarMathNodeMode::Round
            | ScalarMathNodeMode::Saturate
            | ScalarMathNodeMode::Sign
            | ScalarMathNodeMode::Sin
            | ScalarMathNodeMode::Sinh
            | ScalarMathNodeMode::Tan
            | ScalarMathNodeMode::Tanh
            | ScalarMathNodeMode::Trunc => {
                vec![GraphDefaultInputSlot::new::<F32Type>("X".into())]
            }
        }
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Result".into())]
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        ctx.view_all_slots_with_header(
            pick_list(
                ScalarMathNodeMode::ALL,
                Some(*state),
                ScalarMathNodeMessage::ModeChanged,
            )
            .width(Length::Fill),
            ScalarMathNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            ScalarMathNodeMessage::ModeChanged(mode) => *state = mode,
            ScalarMathNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let a = ctx.get_input(0);
        let b = ctx.get_input(1);
        let c = ctx.get_input(2);

        let expression = match state {
            ScalarMathNodeMode::Add => format!("{} + {}", a?, b?),
            ScalarMathNodeMode::Subtract => format!("{} - {}", a?, b?),
            ScalarMathNodeMode::Multiply => format!("{} * {}", a?, b?),
            ScalarMathNodeMode::Divide => format!("{} / {}", a?, b?),
            ScalarMathNodeMode::Acos => format!("acos({})", a?),
            ScalarMathNodeMode::Acosh => format!("acosh({})", a?),
            ScalarMathNodeMode::Asin => format!("asin({})", a?),
            ScalarMathNodeMode::Asinh => format!("asinh({})", a?),
            ScalarMathNodeMode::Atan => format!("atan({})", a?),
            ScalarMathNodeMode::Atanh => format!("atanh({})", a?),
            ScalarMathNodeMode::Ceil => format!("ceil({})", a?),
            ScalarMathNodeMode::Cos => format!("cos({})", a?),
            ScalarMathNodeMode::Cosh => format!("cosh({})", a?),
            ScalarMathNodeMode::Degrees => format!("degrees({})", a?),
            ScalarMathNodeMode::Exp => format!("exp({})", a?),
            ScalarMathNodeMode::Exp2 => format!("exp2({})", a?),
            ScalarMathNodeMode::Floor => format!("floor({})", a?),
            ScalarMathNodeMode::Fract => format!("fract({})", a?),
            ScalarMathNodeMode::InverseSqrt => format!("inverseSqrt({})", a?),
            ScalarMathNodeMode::Ln => format!("log({})", a?),
            ScalarMathNodeMode::Log2 => format!("log2({})", a?),
            ScalarMathNodeMode::Max => format!("max({}, {})", a?, b?),
            ScalarMathNodeMode::Min => format!("min({}, {})", a?, b?),
            ScalarMathNodeMode::Mix => format!("mix({}, {}, {})", a?, b?, c?),
            ScalarMathNodeMode::Pow => format!("pow({}, {})", a?, b?),
            ScalarMathNodeMode::Radians => format!("radians({})", a?),
            ScalarMathNodeMode::Round => format!("round({})", a?),
            ScalarMathNodeMode::Saturate => format!("saturate({})", a?),
            ScalarMathNodeMode::Sign => format!("sign({})", a?),
            ScalarMathNodeMode::Sin => format!("sin({})", a?),
            ScalarMathNodeMode::Sinh => format!("sinh({})", a?),
            ScalarMathNodeMode::Sqrt => format!("sqrt({})", a?),
            ScalarMathNodeMode::Tan => format!("tan({})", a?),
            ScalarMathNodeMode::Tanh => format!("tanh({})", a?),
            ScalarMathNodeMode::Trunc => format!("trunc({})", a?),
        };
        let output = ctx.get_output(0)?;

        Ok(format!("let {} = {};\n", output, expression))
    }
}

#[derive(Default, Clone)]
pub struct VectorMathNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
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

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        VectorMathNodeMode::Add
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(VectorMathNode), is_dark)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        match state {
            VectorMathNodeMode::Add | VectorMathNodeMode::Max | VectorMathNodeMode::Min => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("A".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("B".into()),
            ],
            VectorMathNodeMode::Subtract => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("Minuend".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("Subtrahend".into()),
            ],
            VectorMathNodeMode::Multiply => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("A".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("B".into()),
            ],
            VectorMathNodeMode::Divide => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("Dividend".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("Divisor".into()),
            ],
            VectorMathNodeMode::Pow => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("Base".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("Exponent".into()),
            ],
            VectorMathNodeMode::Distance | VectorMathNodeMode::Dot => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("A".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("B".into()),
            ],
            VectorMathNodeMode::Reflect => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("Incident".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("Normal".into()),
            ],
            VectorMathNodeMode::Mix => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("A".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("B".into()),
                GraphDefaultInputSlot::new::<Vec2FType>("Factor".into()),
            ],
            VectorMathNodeMode::Acosh => {
                vec![GraphDefaultInputSlot::new::<Vec2FType>("X".into())]
            }
            VectorMathNodeMode::Ln
            | VectorMathNodeMode::Log2
            | VectorMathNodeMode::Sqrt
            | VectorMathNodeMode::InverseSqrt => {
                vec![GraphDefaultInputSlot::new::<Vec2FType>("X".into())]
            }
            VectorMathNodeMode::Length => {
                vec![GraphDefaultInputSlot::new::<Vec2FType>("Vector".into())]
            }
            VectorMathNodeMode::Acos
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
            | VectorMathNodeMode::Radians
            | VectorMathNodeMode::Round
            | VectorMathNodeMode::Saturate
            | VectorMathNodeMode::Sign
            | VectorMathNodeMode::Sin
            | VectorMathNodeMode::Sinh
            | VectorMathNodeMode::Tan
            | VectorMathNodeMode::Tanh
            | VectorMathNodeMode::Trunc => {
                vec![GraphDefaultInputSlot::new::<Vec2FType>("X".into())]
            }
        }
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
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
            | VectorMathNodeMode::Trunc => {
                vec![GraphDefaultOutputSlot::new::<Vec2FType>("Result".into())]
            }

            VectorMathNodeMode::Dot | VectorMathNodeMode::Distance | VectorMathNodeMode::Length => {
                vec![GraphDefaultOutputSlot::new::<F32Type>("Result".into())]
            }
        }
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        ctx.view_all_slots_with_header(
            pick_list(
                VectorMathNodeMode::ALL,
                Some(*state),
                VectorMathNodeMessage::ModeChanged,
            )
            .width(Length::Fill),
            VectorMathNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            VectorMathNodeMessage::ModeChanged(mode) => *state = mode,
            VectorMathNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let a = ctx.get_input(0);
        let b = ctx.get_input(1);
        let c = ctx.get_input(2);
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = {};\n",
            output,
            match state {
                VectorMathNodeMode::Add => format!("{} + {}", a?, b?),
                VectorMathNodeMode::Subtract => format!("{} - {}", a?, b?),
                VectorMathNodeMode::Multiply => format!("{} * {}", a?, b?),
                VectorMathNodeMode::Divide => format!("{} / {}", a?, b?),
                VectorMathNodeMode::Acos => format!("acos({})", a?),
                VectorMathNodeMode::Acosh => format!("acosh({})", a?),
                VectorMathNodeMode::Asin => format!("asin({})", a?),
                VectorMathNodeMode::Asinh => format!("asinh({})", a?),
                VectorMathNodeMode::Atan => format!("atan({})", a?),
                VectorMathNodeMode::Atanh => format!("atanh({})", a?),
                VectorMathNodeMode::Ceil => format!("ceil({})", a?),
                VectorMathNodeMode::Cos => format!("cos({})", a?),
                VectorMathNodeMode::Cosh => format!("cosh({})", a?),
                VectorMathNodeMode::Degrees => format!("degrees({})", a?),
                VectorMathNodeMode::Distance => format!("distance({}, {})", a?, b?),
                VectorMathNodeMode::Dot => format!("dot({}, {})", a?, b?),
                VectorMathNodeMode::Exp => format!("exp({})", a?),
                VectorMathNodeMode::Exp2 => format!("exp2({})", a?),
                VectorMathNodeMode::Floor => format!("floor({})", a?),
                VectorMathNodeMode::Fract => format!("fract({})", a?),
                VectorMathNodeMode::InverseSqrt => format!("inverseSqrt({})", a?),
                VectorMathNodeMode::Ln => format!("log({})", a?),
                VectorMathNodeMode::Length => format!("length({})", a?),
                VectorMathNodeMode::Log2 => format!("log2({})", a?),
                VectorMathNodeMode::Max => format!("max({}, {})", a?, b?),
                VectorMathNodeMode::Min => format!("min({}, {})", a?, b?),
                VectorMathNodeMode::Mix => format!("mix({}, {}, {})", a?, b?, c?),
                VectorMathNodeMode::Pow => format!("pow({}, {})", a?, b?),
                VectorMathNodeMode::Radians => format!("radians({})", a?),
                VectorMathNodeMode::Reflect => format!("reflect({}, {})", a?, b?),
                VectorMathNodeMode::Round => format!("round({})", a?),
                VectorMathNodeMode::Saturate => format!("saturate({})", a?),
                VectorMathNodeMode::Sign => format!("sign({})", a?),
                VectorMathNodeMode::Sin => format!("sin({})", a?),
                VectorMathNodeMode::Sinh => format!("sinh({})", a?),
                VectorMathNodeMode::Sqrt => format!("sqrt({})", a?),
                VectorMathNodeMode::Tan => format!("tan({})", a?),
                VectorMathNodeMode::Tanh => format!("tanh({})", a?),
                VectorMathNodeMode::Trunc => format!("trunc({})", a?),
            }
        ))
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

impl RectMathNodeMode {
    pub const ALL: [RectMathNodeMode; 4] = [
        RectMathNodeMode::Union,
        RectMathNodeMode::Intersection,
        RectMathNodeMode::Inflate,
        RectMathNodeMode::Shrink,
    ];
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

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        RectMathNodeMode::Union
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(RectMathNode), is_dark)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        match state {
            RectMathNodeMode::Union | RectMathNodeMode::Intersection => vec![
                GraphDefaultInputSlot::new::<RectType>("A".into()),
                GraphDefaultInputSlot::new::<RectType>("B".into()),
            ],
            RectMathNodeMode::Inflate | RectMathNodeMode::Shrink => {
                vec![
                    GraphDefaultInputSlot::new::<RectType>("Rect".into()),
                    GraphDefaultInputSlot::new::<Vec2FType>("Amount".into()),
                ]
            }
        }
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<RectType>("Result".into())]
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        ctx.view_all_slots_with_header(
            pick_list(
                RectMathNodeMode::ALL,
                Some(*state),
                RectMathNodeMessage::ModeChanged,
            )
            .width(Length::Fill),
            RectMathNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            RectMathNodeMessage::ModeChanged(mode) => *state = mode,
            RectMathNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
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
}

#[derive(Default, Clone)]
pub struct CompareNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display)]
pub enum CompareNodeMode {
    #[display("Less Than")]
    LessThan,
    #[display("Less Equal")]
    LessEqual,
    #[display("Greater Than")]
    GreaterThan,
    #[display("Greater Equal")]
    GreaterEqual,
    Equal,
}

impl CompareNodeMode {
    pub const ALL: [CompareNodeMode; 5] = [
        CompareNodeMode::LessThan,
        CompareNodeMode::LessEqual,
        CompareNodeMode::GreaterThan,
        CompareNodeMode::GreaterEqual,
        CompareNodeMode::Equal,
    ];
}

#[derive(Clone)]
pub enum CompareNodeMessage {
    ModeChanged(CompareNodeMode),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for CompareNode {
    type State = CompareNodeMode;
    type Message = CompareNodeMessage;

    fn name(&self) -> &'static str {
        "Compare"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        CompareNodeMode::LessThan
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(CompareNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("Lhs".into()),
            GraphDefaultInputSlot::new::<F32Type>("Rhs".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<BoolType>("Result".into())]
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        ctx.view_all_slots_with_header(
            pick_list(
                CompareNodeMode::ALL,
                Some(*state),
                CompareNodeMessage::ModeChanged,
            )
            .width(Length::Fill),
            CompareNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            CompareNodeMessage::ModeChanged(mode) => *state = mode,
            CompareNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let lhs = ctx.get_input(0)?;
        let rhs = ctx.get_input(1)?;
        let output = ctx.get_output(0)?;
        let operator = match state {
            CompareNodeMode::LessThan => "<",
            CompareNodeMode::LessEqual => "<=",
            CompareNodeMode::GreaterThan => ">",
            CompareNodeMode::GreaterEqual => ">=",
            CompareNodeMode::Equal => "==",
        };

        Ok(format!("let {} = {} {} {};\n", output, lhs, operator, rhs))
    }
}

#[derive(Default, Clone)]
pub struct ScalarSelectNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for ScalarSelectNode {
    fn name(&self) -> &'static str {
        "Scalar Select"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(ScalarSelectNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<BoolType>("Condition".into()),
            GraphDefaultInputSlot::new::<F32Type>("False".into()),
            GraphDefaultInputSlot::new::<F32Type>("True".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Result".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let condition = ctx.get_input(0)?;
        let false_value = ctx.get_input(1)?;
        let true_value = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = select({}, {}, {});\n",
            output, false_value, true_value, condition
        ))
    }
}

#[derive(Default, Clone)]
pub struct VectorSelectNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for VectorSelectNode {
    fn name(&self) -> &'static str {
        "Vector Select"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(VectorSelectNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<BoolType>("Condition".into()),
            GraphDefaultInputSlot::new::<Vec2FType>("False".into()),
            GraphDefaultInputSlot::new::<Vec2FType>("True".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>("Result".into())]
    }

    fn generate_code(
        &self,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let condition = ctx.get_input(0)?;
        let false_value = ctx.get_input(1)?;
        let true_value = ctx.get_input(2)?;
        let output = ctx.get_output(0)?;

        Ok(format!(
            "let {} = select({}, {}, {});\n",
            output, false_value, true_value, condition
        ))
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

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(TimeNode), is_dark)
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
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("Now".into()),
            GraphDefaultOutputSlot::new::<F32Type>("Stroke Begin".into()),
        ]
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
}

#[derive(Default, Clone)]
pub struct ClampNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for ClampNode {
    fn name(&self) -> &'static str {
        "Clamp"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(ClampNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("Value".into()),
            GraphDefaultInputSlot::new::<F32Type>("Min".into()),
            GraphDefaultInputSlot::new::<F32Type>("Max".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Result".into())]
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
}

#[derive(Default, Clone)]
pub struct StepNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for StepNode {
    fn name(&self) -> &'static str {
        "Step"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(StepNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("Edge".into()),
            GraphDefaultInputSlot::new::<F32Type>("X".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Result".into())]
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
}

#[derive(Default, Clone)]
pub struct SmoothStepNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for SmoothStepNode {
    fn name(&self) -> &'static str {
        "Smooth Step"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(SmoothStepNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("Edge0".into()),
            GraphDefaultInputSlot::new::<F32Type>("Edge1".into()),
            GraphDefaultInputSlot::new::<F32Type>("X".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Result".into())]
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
}

#[derive(Default, Clone)]
pub struct SplitComponentsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for SplitComponentsNode {
    fn name(&self) -> &'static str {
        "Split Components"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(SplitComponentsNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>("Vector".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("X".into()),
            GraphDefaultOutputSlot::new::<F32Type>("Y".into()),
        ]
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
}

#[derive(Default, Clone)]
pub struct CombineComponentsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for CombineComponentsNode {
    fn name(&self) -> &'static str {
        "Combine Components"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(CombineComponentsNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("X".into()),
            GraphDefaultInputSlot::new::<F32Type>("Y".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>("Vector".into())]
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
}

#[derive(Default, Clone)]
pub struct CombineColorComponentsNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for CombineColorComponentsNode {
    fn name(&self) -> &'static str {
        "Combine Color Components"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(CombineColorComponentsNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("R".into()),
            GraphDefaultInputSlot::new::<F32Type>("G".into()),
            GraphDefaultInputSlot::new::<F32Type>("B".into()),
            GraphDefaultInputSlot::new::<F32Type>("A".into()),
        ]
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

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for SplitColorComponentsNode {
    fn name(&self) -> &'static str {
        "Split Color Components"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(SplitColorComponentsNode), is_dark)
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
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("R".into()),
            GraphDefaultOutputSlot::new::<F32Type>("G".into()),
            GraphDefaultOutputSlot::new::<F32Type>("B".into()),
            GraphDefaultOutputSlot::new::<F32Type>("A".into()),
        ]
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
}

#[derive(Default, Clone)]
pub struct GetPixelColorNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for GetPixelColorNode {
    fn name(&self) -> &'static str {
        "Get Pixel Color"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(GetPixelColorNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>("Texture".into()),
            GraphDefaultInputSlot::new::<Vec2FType>("Position".into()),
        ]
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
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for TextureNode {
    type State = TextureId;
    type Message = TextureNodeMessage;

    fn name(&self) -> &'static str {
        "Texture"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        TextureId::NULL
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(TextureNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<TextureType>("Texture".into())]
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        let texture_storage = ctx.resources.textures.load();
        let textures = texture_storage.all().values().cloned().collect::<Vec<_>>();
        let selected = textures
            .iter()
            .find(|texture| Some(texture.external_id) == **state)
            .cloned();
        ctx.view_all_slots_with_header(
            pick_list(textures, selected, |texture| {
                TextureNodeMessage::TextureChanged(TextureId(Some(texture.external_id)))
            })
            .width(Length::Fill),
            TextureNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            TextureNodeMessage::TextureChanged(id) => *state = id,
            TextureNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
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
}

// TODO: Mixing in different color spaces.
#[derive(Default, Clone)]
pub struct ColorMixNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for ColorMixNode {
    fn name(&self) -> &'static str {
        "Color Mix"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(ColorMixNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Color A".into()),
            GraphDefaultInputSlot::new::<ColorType>("Color B".into()),
            GraphDefaultInputSlot::new::<F32Type>("Factor".into()),
        ]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<ColorType>("Result".into())]
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
}

#[derive(Default, Clone)]
pub struct TextureSizeNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for TextureSizeNode {
    fn name(&self) -> &'static str {
        "Texture Size"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(TextureSizeNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<TextureType>("Texture".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<Vec2FType>("Size".into())]
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
}

static UNIQUE_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Default, Clone)]
pub struct GraphFunctionNode;

#[derive(Clone)]
pub struct GraphFunctionReference {
    pub id: GraphFunctionId,
    pub name: String,
}

impl std::fmt::Display for GraphFunctionReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
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
    FunctionChanged(GraphFunctionId),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for GraphFunctionNode {
    type State = GraphFunctionNodeState;
    type Message = GraphFunctionNodeMessage;

    fn name(&self) -> &'static str {
        "Function"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        GraphFunctionNodeState { id: None }
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(GraphFunctionNode), is_dark)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        let functions = ctx.resources.functions.load();
        let Some(func) = state.id.as_ref().and_then(|id| functions.get(id)) else {
            return Vec::new();
        };

        func.graph
            .signature()
            .inputs
            .iter()
            .map(|(_, var)| {
                GraphDefaultInputSlot::new_boxed(
                    var.identifier().to_string(),
                    dyn_clone::clone_box(var.ty()),
                )
            })
            .collect()
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        let functions = ctx.resources.functions.load();
        let Some(func) = state.id.as_ref().and_then(|id| functions.get(id)) else {
            return Vec::new();
        };

        func.graph
            .signature()
            .outputs
            .iter()
            .map(|(_, var)| {
                GraphDefaultOutputSlot::new_boxed(
                    var.identifier().to_string(),
                    dyn_clone::clone_box(var.ty()),
                )
            })
            .collect()
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        let function_storage = ctx.resources.functions.load();
        let functions = function_storage
            .all()
            .iter()
            .map(|(id, graph)| GraphFunctionReference {
                id: *id,
                name: graph.name.clone(),
            })
            .collect::<Vec<_>>();
        let selected = functions
            .iter()
            .find(|reference| Some(reference.id) == state.id)
            .cloned();
        ctx.view_all_slots_with_header(
            pick_list(functions, selected, |reference| {
                GraphFunctionNodeMessage::FunctionChanged(reference.id)
            })
            .width(Length::Fill),
            GraphFunctionNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            GraphFunctionNodeMessage::FunctionChanged(id) => state.id = Some(id),
            GraphFunctionNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
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
        let functions = ctx.resources.functions.load();
        let Some(func) = functions.get(id) else {
            return Ok(Default::default());
        };

        let input_idents = (0..ctx.inputs.len()).try_fold(
            Vec::with_capacity(ctx.inputs.len()),
            |mut acc, i| {
                acc.push(ctx.get_input(i)?);
                Ok::<_, GraphNodeCodeGenError>(acc)
            },
        )?;

        let (output_idents, _, code) = func
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

#[derive(Default, Serialize, Deserialize)]
pub struct GraphInputNodeState {
    pub name: String,
    pub ty: Option<&'static str>,
}

#[derive(Clone)]
pub enum GraphInputNodeMessage {
    NameChanged(String),
    TypeChanged(&'static str),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for GraphInputNode {
    type State = GraphInputNodeState;
    type Message = GraphInputNodeMessage;

    fn name(&self) -> &'static str {
        "Graph Input"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        GraphInputNodeState::default()
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(GraphInputNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        let Some(ty) = state
            .ty
            .and_then(|ty| ctx.resources.type_registry.get_type(ty))
            .map(dyn_clone::clone_box)
        else {
            return vec![];
        };

        vec![GraphDefaultOutputSlot::new_boxed(state.name.clone(), ty)]
    }

    fn update_signature(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeUpdateSignatureContext<'_, Data>,
    ) {
        ctx.require_output_slot_as_graph_input(0, state.name.clone());
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        let types = ctx
            .resources
            .type_registry
            .all_types()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        ctx.view_all_slots_with_header(
            column![
                text_input("Name", &state.name).on_input(GraphInputNodeMessage::NameChanged),
                pick_list(types, state.ty, GraphInputNodeMessage::TypeChanged).width(Length::Fill),
            ]
            .spacing(2),
            GraphInputNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            GraphInputNodeMessage::NameChanged(name) => state.name = name,
            GraphInputNodeMessage::TypeChanged(ty) => state.ty = Some(ty),
            GraphInputNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
        }
    }

    fn generate_code(
        &self,
        _: &Self::State,
        _: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(Default::default())
    }
}

#[derive(Default, Clone)]
pub struct GraphOutputNode;

#[derive(Default, Serialize, Deserialize)]
pub struct GraphOutputNodeState {
    pub name: String,
    pub ty: Option<&'static str>,
}

#[derive(Clone)]
pub enum GraphOutputNodeMessage {
    NameChanged(String),
    TypeChanged(&'static str),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for GraphOutputNode {
    type State = GraphOutputNodeState;
    type Message = GraphOutputNodeMessage;

    fn name(&self) -> &'static str {
        "Graph Output"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        GraphOutputNodeState::default()
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(GraphOutputNode), is_dark)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        let Some(ty) = state
            .ty
            .and_then(|ty| ctx.resources.type_registry.get_type(ty))
            .map(dyn_clone::clone_box)
        else {
            return vec![];
        };

        vec![GraphDefaultInputSlot::new_boxed(state.name.clone(), ty)]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        let types = ctx
            .resources
            .type_registry
            .all_types()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        ctx.view_all_slots_with_header(
            column![
                text_input("Name", &state.name).on_input(GraphOutputNodeMessage::NameChanged),
                pick_list(types, state.ty, GraphOutputNodeMessage::TypeChanged).width(Length::Fill),
            ]
            .spacing(2),
            GraphOutputNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            GraphOutputNodeMessage::NameChanged(name) => state.name = name,
            GraphOutputNodeMessage::TypeChanged(ty) => state.ty = Some(ty),
            GraphOutputNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
        }
    }

    fn generate_code(
        &self,
        _: &Self::State,
        _: GraphNodeCodeGenContext<'_, Data>,
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

impl std::fmt::Display for ExternalVariableReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Default, Clone)]
pub struct ExternalVariableNode;

#[derive(Clone)]
pub enum ExternalVariableNodeMessage {
    VariableChanged(ExternalVariableId),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for ExternalVariableNode {
    type State = Option<ExternalVariableId>;
    type Message = ExternalVariableNodeMessage;

    fn name(&self) -> &'static str {
        "External Variable"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(ExternalVariableNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
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
                Some(var) => vec![GraphDefaultOutputSlot::new_boxed(
                    var.name.clone(),
                    dyn_clone::clone_box(var.value.ty()),
                )],
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

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        None
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        let variables = ctx
            .resources
            .external_vars
            .all()
            .iter()
            .map(|entry| ExternalVariableReference {
                id: entry.id,
                name: entry.name.clone(),
            })
            .collect::<Vec<_>>();
        let selected = variables
            .iter()
            .find(|reference| Some(reference.id) == *state)
            .cloned();
        ctx.view_all_slots_with_header(
            pick_list(variables, selected, |reference| {
                ExternalVariableNodeMessage::VariableChanged(reference.id)
            })
            .width(Length::Fill),
            ExternalVariableNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            ExternalVariableNodeMessage::VariableChanged(id) => *state = Some(id),
            ExternalVariableNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
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
    CurveChanged(CubicCurve),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for CurveNode {
    type State = CurveNodeState;
    type Message = CurveNodeMessage;

    fn name(&self) -> &'static str {
        "Curve"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        Default::default()
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(CurveNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>("X".into())]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<F32Type>("Y".into())]
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        ctx.view_all_slots_with_header(
            CurveEdit::new(CubicCurve::new(state.control_points.clone()))
                .width(Length::Fill)
                .height(Length::Fixed(128.0))
                .on_change(CurveNodeMessage::CurveChanged),
            CurveNodeMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            CurveNodeMessage::CurveChanged(curve) => {
                state.control_points = curve.control_points().to_vec();
            }
            CurveNodeMessage::LiteralUpdate(message) => ctx.update_literal(message),
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
}

#[derive(Default, Clone)]
pub struct RandomNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for RandomNode {
    fn name(&self) -> &'static str {
        "Random Number"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(RandomNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>("Seed".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![
            GraphDefaultOutputSlot::new::<F32Type>("Scalar".into()),
            GraphDefaultOutputSlot::new::<Vec2FType>("Vec2".into()),
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

#[derive(Default, Clone)]
pub struct BreakBeforeNextIterationNode;

#[stateless]
impl<Data: GraphData> StatelessCommonGraphNode<Data> for BreakBeforeNextIterationNode {
    fn name(&self) -> &'static str {
        "Break Before Next Iteration"
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(BreakBeforeNextIterationNode), is_dark)
    }

    fn create_inputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<BoolType>("Condition".into())]
    }

    fn create_outputs(
        &self,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn generate_code(
        &self,
        _: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        // This is handled by while node
        Ok(String::new())
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    pub WhileVariableId : Uuid
}

#[derive(Clone)]
pub struct WhileLocalSchema {
    pub id: WhileVariableId,
    pub name: String,
    pub ty: Box<dyn ErasedGraphValueType>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SerializableWhileLocalSchema {
    pub id: WhileVariableId,
    pub name: String,
    pub ty: String,
}

impl<Data: GraphData> GraphSerializable<Data> for WhileLocalSchema {
    fn to_toml(&self) -> anyhow::Result<toml::Value> {
        let serializable = SerializableWhileLocalSchema {
            id: self.id,
            name: self.name.clone(),
            ty: self.ty.name().to_string(),
        };
        Ok(toml::Value::try_from(serializable)?)
    }

    fn from_toml(value: toml::Value, resources: &GraphResources<Data>) -> anyhow::Result<Self> {
        let serializable = SerializableWhileLocalSchema::deserialize(value)?;
        let ty = resources
            .type_registry
            .get_type(&serializable.ty)
            .ok_or_else(|| anyhow::anyhow!("Unknown type: {}", serializable.ty))?;
        Ok(WhileLocalSchema {
            id: serializable.id,
            name: serializable.name,
            ty: dyn_clone::clone_box(ty),
        })
    }
}

#[derive(Clone)]
struct WhileLocalSchemaDraft {
    pub id: WhileVariableId,
    pub name: String,
    pub ty: Option<Box<dyn ErasedGraphValueType>>,
}

#[derive(Clone, Default)]
struct WhileSchemaDraft {
    locals: IndexMap<WhileVariableId, WhileLocalSchemaDraft>,
}

impl WhileSchemaDraft {
    pub fn new(locals: &IndexMap<WhileVariableId, WhileLocalSchema>) -> Self {
        Self {
            locals: locals
                .iter()
                .map(|(id, schema)| {
                    (
                        *id,
                        WhileLocalSchemaDraft {
                            id: *id,
                            name: schema.name.clone(),
                            ty: Some(schema.ty.clone()),
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn finalize(&self) -> IndexMap<WhileVariableId, WhileLocalSchema> {
        self.locals
            .iter()
            .map(|(id, schema)| {
                (
                    *id,
                    WhileLocalSchema {
                        id: *id,
                        name: schema.name.clone(),
                        ty: schema.ty.clone().unwrap(),
                    },
                )
            })
            .collect()
    }
}

pub struct WhileNodeState<Data: GraphData> {
    locals: Arc<Mutex<IndexMap<WhileVariableId, WhileLocalSchema>>>,
    revision: u64,
    body: Graph<Data>,
    schema_draft: Option<WhileSchemaDraft>,
}

impl<Data: GraphData> WhileNodeState<Data> {
    pub fn body(&self) -> &Graph<Data> {
        &self.body
    }

    pub fn body_mut(&mut self) -> &mut Graph<Data> {
        &mut self.body
    }

    pub fn add_local<T: GraphValueType + Default>(&mut self, name: String) -> WhileVariableId {
        let id = WhileVariableId::new(Uuid::new_v4());
        self.locals.lock().insert(
            id,
            WhileLocalSchema {
                id,
                name,
                ty: Box::new(T::default()),
            },
        );
        self.revision += 1;
        id
    }

    pub fn sync_body_nodes(&mut self) {
        let input_ids = self
            .body
            .nodes
            .iter()
            .filter(|(_, node)| node.data.state::<WhileNodeInput>().is_some())
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        let output_ids = self
            .body
            .nodes
            .iter()
            .filter(|(_, node)| node.data.state::<WhileNodeOutput>().is_some())
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();

        let locals = self.locals.lock().clone();
        for id in input_ids {
            self.body.update_node_state::<WhileNodeInput>(id, |st| {
                if let Some(variable) = st.variable
                    && !locals.contains_key(&variable)
                {
                    st.variable = None;
                }
            });
        }
        for id in output_ids {
            self.body.update_node_state::<WhileNodeOutput>(id, |st| {
                if let Some(variable) = st.variable
                    && !locals.contains_key(&variable)
                {
                    st.variable = None;
                }
            });
        }
        self.body.invalidate_cache();
    }
}

#[derive(Serialize, Deserialize)]
struct SerializableWhileNodeState {
    locals: Vec<SerializableWhileLocalSchema>,
    body: SerializableGraph,
}

impl<Data: GraphData> GraphSerializable<Data> for WhileNodeState<Data> {
    fn to_toml(&self) -> anyhow::Result<toml::Value> {
        let locals = self
            .locals
            .lock()
            .values()
            .map(|local| SerializableWhileLocalSchema {
                id: local.id,
                name: local.name.clone(),
                ty: local.ty.clone().name().to_string(),
            })
            .collect();
        let body = self.body.as_serialized()?;
        Ok(toml::Value::try_from(SerializableWhileNodeState {
            locals,
            body,
        })?)
    }

    fn from_toml(value: toml::Value, resources: &GraphResources<Data>) -> anyhow::Result<Self> {
        let serialized = SerializableWhileNodeState::deserialize(value)?;
        let locals =
            serialized
                .locals
                .into_iter()
                .try_fold(IndexMap::new(), |mut locals, local| {
                    locals.insert(
                        local.id,
                        WhileLocalSchema {
                            id: local.id,
                            name: local.name,
                            ty: dyn_clone::clone_box(
                                resources.type_registry.get_type(&local.ty).ok_or_else(|| {
                                    anyhow!("Type {} not found in registry", local.ty)
                                })?,
                            ),
                        },
                    );

                    Result::<_, anyhow::Error>::Ok(locals)
                })?;
        let locals = Arc::new(Mutex::new(locals));

        let while_node_extra = {
            let mut r = GraphNodeRegistry::default();
            r.register_boxed(Box::new(WhileNodeInput {
                locals: locals.clone(),
            }));
            r.register_boxed(Box::new(WhileNodeOutput {
                locals: locals.clone(),
            }));
            r.register::<BreakBeforeNextIterationNode>();
            r
        };

        let mut node_registry = resources.node_registry.as_ref().clone();
        node_registry.merge(while_node_extra);

        let body_resources = GraphResources {
            type_registry: resources.type_registry.clone(),
            node_registry: Arc::new(node_registry),
            textures: resources.textures.clone(),
            functions: resources.functions.clone(),
            external_vars: resources.external_vars.clone(),
        };
        let (body, errors) = Graph::from_serialized(&serialized.body, body_resources);
        if !errors.is_empty() {
            return Err(anyhow!("While body deserialization failed: {errors:?}"));
        }
        let body = body.ok_or_else(|| anyhow!("While body is missing"))?;

        let mut state = Self {
            locals,
            revision: 0,
            body,
            schema_draft: None,
        };
        state.sync_body_nodes();
        Ok(state)
    }
}

#[derive(Default, Clone)]
pub struct WhileNodeInput {
    pub locals: Arc<Mutex<IndexMap<WhileVariableId, WhileLocalSchema>>>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct WhileNodeInputState {
    pub variable: Option<WhileVariableId>,
}

#[derive(Clone)]
pub enum WhileNodeInputMessage {
    VariableChanged(WhileVariableId),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for WhileNodeInput {
    type State = WhileNodeInputState;
    type Message = WhileNodeInputMessage;

    fn name(&self) -> &'static str {
        "While Node Input"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        WhileNodeInputState::default()
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(WhileNodeInput), is_dark)
    }

    fn create_inputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        Vec::new()
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        let locals = self.locals.lock();
        let Some(local) = state.variable.as_ref().and_then(|id| locals.get(id)) else {
            return Vec::new();
        };
        vec![GraphDefaultOutputSlot::new_boxed(
            format!("{} Current", local.name),
            local.ty.clone(),
        )]
    }

    fn update_signature(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeUpdateSignatureContext<'_, Data>,
    ) {
        let locals = self.locals.lock();
        let Some(local) = state.variable.as_ref().and_then(|id| locals.get(id)) else {
            return;
        };
        ctx.require_output_slot_as_graph_input(0, local.name.clone());
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        let locals = while_variable_references(&self.locals.lock());
        let selected = state
            .variable
            .and_then(|id| locals.iter().find(|reference| reference.id == id).cloned());
        ctx.view_all_slots_with_header(
            pick_list(locals, selected, |reference| {
                WhileNodeInputMessage::VariableChanged(reference.id)
            })
            .width(Length::Fill),
            WhileNodeInputMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            WhileNodeInputMessage::VariableChanged(variable) => state.variable = Some(variable),
            WhileNodeInputMessage::LiteralUpdate(literal) => ctx.update_literal(literal),
        }
    }

    fn generate_code(
        &self,
        _: &Self::State,
        _: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(String::new())
    }
}

#[derive(Default, Clone)]
pub struct WhileNodeOutput {
    pub locals: Arc<Mutex<IndexMap<WhileVariableId, WhileLocalSchema>>>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct WhileNodeOutputState {
    pub variable: Option<WhileVariableId>,
}

#[derive(Clone)]
pub enum WhileNodeOutputMessage {
    VariableChanged(WhileVariableId),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<Data: GraphData> GraphNode<Data> for WhileNodeOutput {
    type State = WhileNodeOutputState;
    type Message = WhileNodeOutputMessage;

    fn name(&self) -> &'static str {
        "While Node Output"
    }

    fn default_state(&self, _: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        WhileNodeOutputState::default()
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(WhileNodeOutput), is_dark)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        let locals = self.locals.lock();
        let Some(local) = state.variable.as_ref().and_then(|id| locals.get(id)) else {
            return Vec::new();
        };
        vec![GraphDefaultInputSlot::new_boxed(
            format!("{} Next", local.name),
            local.ty.clone(),
        )]
    }

    fn create_outputs(
        &self,
        _: &Self::State,
        _: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        Vec::new()
    }

    fn update_signature(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeUpdateSignatureContext<'_, Data>,
    ) {
        let locals = self.locals.lock();
        let Some(local) = state.variable.as_ref().and_then(|id| locals.get(id)) else {
            return;
        };
        ctx.require_input_slot_as_graph_output(0, local.name.clone());
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        let locals = while_variable_references(&self.locals.lock());
        let selected = state
            .variable
            .and_then(|id| locals.iter().find(|reference| reference.id == id).cloned());
        ctx.view_all_slots_with_header(
            pick_list(locals, selected, |reference| {
                WhileNodeOutputMessage::VariableChanged(reference.id)
            })
            .width(Length::Fill),
            WhileNodeOutputMessage::LiteralUpdate,
        )
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            WhileNodeOutputMessage::VariableChanged(variable) => state.variable = Some(variable),
            WhileNodeOutputMessage::LiteralUpdate(literal) => ctx.update_literal(literal),
        }
    }

    fn generate_code(
        &self,
        _: &Self::State,
        _: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(String::new())
    }
}

#[derive(Clone)]
struct WhileVariableReference {
    id: WhileVariableId,
    name: String,
}

impl std::fmt::Display for WhileVariableReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.name.fmt(f)
    }
}

impl PartialEq for WhileVariableReference {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

fn while_variable_references(
    locals: &IndexMap<WhileVariableId, WhileLocalSchema>,
) -> Vec<WhileVariableReference> {
    locals
        .values()
        .map(|local| WhileVariableReference {
            id: local.id,
            name: local.name.clone(),
        })
        .collect()
}

#[derive(Default, Clone)]
pub struct WhileNode;

#[derive(Clone)]
pub enum WhileNodeMessage {
    ToggleEditor,
    EditorAddLocal,
    EditorRemoveLocal(WhileVariableId),
    EditorMoveLocalUp(WhileVariableId),
    EditorMoveLocalDown(WhileVariableId),
    EditorRenameLocal(WhileVariableId, String),
    EditorChangeLocalType(WhileVariableId, String),
    EditorConfirm,
    EditorCancel,
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

fn while_schema_editor_view<Data: GraphData>(
    state: &WhileNodeState<Data>,
    resources: &GraphResources<Data>,
) -> GraphElement<'static, WhileNodeMessage> {
    let type_names = resources
        .type_registry
        .all_types()
        .keys()
        .copied()
        .collect::<Vec<&'static str>>();
    let draft = state.schema_draft.as_ref().expect("editor must be open");

    let rows = draft
        .locals
        .values()
        .map(|local| {
            let id = local.id;
            column![
                text_input("Variable Name", &local.name)
                    .on_input(move |name| { WhileNodeMessage::EditorRenameLocal(id, name) }),
                row![
                    pick_list(
                        type_names.clone(),
                        local.ty.as_ref().map(|t| t.name()),
                        move |ty| { WhileNodeMessage::EditorChangeLocalType(id, ty.to_string()) },
                    )
                    .width(Length::Fill),
                    button("Up").on_press(WhileNodeMessage::EditorMoveLocalUp(id)),
                    button("Down").on_press(WhileNodeMessage::EditorMoveLocalDown(id)),
                    button("Delete").on_press(WhileNodeMessage::EditorRemoveLocal(id)),
                ]
                .spacing(4),
            ]
            .spacing(4)
            .into()
        })
        .collect::<Vec<GraphElement<'static, WhileNodeMessage>>>();

    let valid = draft
        .locals
        .values()
        .all(|local| !local.name.is_empty() && local.name.trim() == local.name)
        && draft.locals.values().all(|local| {
            draft
                .locals
                .values()
                .filter(|other| other.name == local.name)
                .count()
                == 1
        });

    let panel = column(rows)
        .width(Length::Fixed(300.0))
        .padding(4)
        .spacing(6)
        .push(row![button("Add Variable").on_press(WhileNodeMessage::EditorAddLocal)].spacing(6))
        .push(
            row![
                button("Cancel").on_press(WhileNodeMessage::EditorCancel),
                button("Confirm").when(valid, |b| b.on_press(WhileNodeMessage::EditorConfirm))
            ]
            .spacing(4),
        );

    container(panel)
        .style(|theme| container::Style {
            background: Some(theme.extended_palette().background.base.color.into()),
            ..container::transparent(theme)
        })
        .into()
}

impl<Data: GraphData> GraphNode<Data> for WhileNode {
    type State = WhileNodeState<Data>;

    type Message = WhileNodeMessage;

    fn name(&self) -> &'static str {
        "While"
    }

    fn default_state(&self, ctx: GraphNodeDefaultStateContext<'_, Data>) -> Self::State {
        let locals = Arc::new(Mutex::new(IndexMap::new()));
        let while_node_extra = {
            let mut r = GraphNodeRegistry::default();
            r.register_boxed(Box::new(WhileNodeInput {
                locals: locals.clone(),
            }));
            r.register_boxed(Box::new(WhileNodeOutput {
                locals: locals.clone(),
            }));
            r.register::<BreakBeforeNextIterationNode>();
            r
        };

        let mut node_registry = ctx.resources.node_registry.as_ref().clone();
        node_registry.merge(while_node_extra);

        let body_resources = GraphResources {
            type_registry: ctx.resources.type_registry.clone(),
            node_registry: Arc::new(node_registry),
            textures: ctx.resources.textures.clone(),
            functions: ctx.resources.functions.clone(),
            external_vars: ctx.resources.external_vars.clone(),
        };

        WhileNodeState {
            locals,
            revision: 0,
            body: Graph::new(body_resources),
            schema_draft: None,
        }
    }

    fn header_color(&self, is_dark: bool) -> Color {
        themed_color(stringify!(WhileNode), is_dark)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        state
            .locals
            .lock()
            .values()
            .map(|local| {
                GraphDefaultInputSlot::new_boxed(format!("{} In", local.name), local.ty.clone())
            })
            .collect()
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultOutputSlot> {
        state
            .locals
            .lock()
            .values()
            .map(|local| {
                GraphDefaultOutputSlot::new_boxed(format!("{} Out", local.name), local.ty.clone())
            })
            .collect()
    }

    fn view(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext<'_, Data>,
    ) -> GraphElement<'static, Self::Message> {
        let trigger = button(text("Edit")).on_press(WhileNodeMessage::ToggleEditor);
        let content = state
            .schema_draft
            .as_ref()
            .map(|_| while_schema_editor_view(state, ctx.resources));
        let popover = Popover::new(trigger).content(content);
        ctx.view_all_slots_with_header(popover, WhileNodeMessage::LiteralUpdate)
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext<'_, Data>,
    ) {
        match message {
            WhileNodeMessage::ToggleEditor => {
                if state.schema_draft.is_some() {
                    state.schema_draft = None;
                } else {
                    state.schema_draft = Some(WhileSchemaDraft::new(&state.locals.lock()));
                }
            }
            WhileNodeMessage::EditorAddLocal => {
                if let Some(draft) = &mut state.schema_draft {
                    let new_id = WhileVariableId::new(Uuid::new_v4());
                    draft.locals.insert(
                        new_id,
                        WhileLocalSchemaDraft {
                            id: new_id,
                            name: String::new(),
                            ty: None,
                        },
                    );
                }
            }
            WhileNodeMessage::EditorRemoveLocal(id) => {
                if let Some(draft) = &mut state.schema_draft {
                    draft.locals.shift_remove(&id);
                }
            }
            WhileNodeMessage::EditorMoveLocalUp(id) => {
                if let Some(draft) = &mut state.schema_draft
                    && let Some(index) = draft.locals.get_index_of(&id)
                    && index > 0
                {
                    draft.locals.swap_indices(index, index - 1);
                }
            }
            WhileNodeMessage::EditorMoveLocalDown(id) => {
                if let Some(draft) = &mut state.schema_draft
                    && let Some(index) = draft.locals.get_index_of(&id)
                    && index + 1 < draft.locals.len()
                {
                    draft.locals.swap_indices(index, index + 1);
                }
            }
            WhileNodeMessage::EditorRenameLocal(id, name) => {
                if let Some(draft) = &mut state.schema_draft
                    && let Some(local) = draft.locals.get_mut(&id)
                {
                    local.name = name;
                }
            }
            WhileNodeMessage::EditorChangeLocalType(id, ty) => {
                if let Some(draft) = &mut state.schema_draft
                    && let Some(local) = draft.locals.get_mut(&id)
                {
                    local.ty = Some(dyn_clone::clone_box(
                        ctx.resources.type_registry.get_type(&ty).unwrap(),
                    ));
                }
            }
            WhileNodeMessage::EditorConfirm => {
                let Some(draft) = &mut state.schema_draft else {
                    return;
                };
                *state.locals.lock() = draft.finalize();
                state.revision += 1;
                state.schema_draft = None;
                state.sync_body_nodes();
            }
            WhileNodeMessage::EditorCancel => {
                state.schema_draft = None;
            }
            WhileNodeMessage::LiteralUpdate(literal) => ctx.update_literal(literal),
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let locals = state.locals.lock().clone();
        if locals.len() != ctx.inputs.len() || locals.len() != ctx.outputs.len() {
            return Err(anyhow!("While parent slot invariant is invalid").into());
        }

        let body = &state.body;
        let signature = body.signature();

        let mut current = HashMap::with_capacity(locals.len());
        let mut code = String::new();
        for (index, local) in locals.values().enumerate() {
            let value = ctx.ident_generator.next_output();
            code.push_str(&format!(
                "var {value} = {};
",
                ctx.get_input(index)?
            ));
            current.insert(local.id, value);
        }

        let mut body_inputs = Vec::with_capacity(signature.inputs.len());
        for slot_id in signature.inputs.keys() {
            let slot = body
                .slots
                .get_output(slot_id)
                .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;
            let node = body.get_node(&slot.node_id).ok_or_else(|| {
                GraphNodeCodeGenError::Custom(anyhow!("While body node is missing"))
            })?;
            let variable = node
                .data
                .state::<WhileNodeInput>()
                .and_then(|state| state.variable)
                .ok_or_else(|| anyhow!("While Node Input has an invalid variable"))?;
            body_inputs.push(
                current
                    .get(&variable)
                    .cloned()
                    .ok_or_else(|| anyhow!("While variable {variable} is not a local"))?,
            );
        }

        let mut next_slots = HashMap::with_capacity(locals.len());
        for slot_id in signature.outputs.keys() {
            let slot = body
                .slots
                .get_input(slot_id)
                .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;
            let node = body
                .get_node(&slot.node_id)
                .ok_or_else(|| anyhow!("While body node is missing"))?;
            let variable = node
                .data
                .state::<WhileNodeOutput>()
                .and_then(|state| state.variable)
                .ok_or_else(|| anyhow!("While Node Output has an invalid variable"))?;
            if next_slots.insert(variable, *slot_id).is_some() {
                return Err(anyhow!("While variable {variable} has duplicate outputs").into());
            }
        }
        for local in locals.values() {
            if !next_slots.contains_key(&local.id) {
                return Err(anyhow!(
                    "While body is missing a While Node Output for variable '{}'",
                    local.name
                )
                .into());
            }
        }

        code.push_str(
            "loop {
",
        );
        let (body_output_idents, body_output_slot_idents, body_code) = body
            .compile(
                body_inputs,
                GraphVarIdentGenerator::new(format!(
                    "while_{}",
                    UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
                )),
                ctx.texture_usage,
            )
            .map_err(|error| GraphNodeCodeGenError::Custom(error.into()))?;
        code.push_str(&body_code);

        let body_outputs = signature
            .outputs
            .keys()
            .copied()
            .zip(body_output_idents)
            .collect::<HashMap<_, _>>();
        for local in locals.values() {
            let input_slot_id = next_slots[&local.id];
            let next = body_outputs
                .get(&input_slot_id)
                .ok_or_else(|| anyhow!("While body output value of {} is missing", local.name))?;
            let input_slot = body
                .slots
                .get_input(&input_slot_id)
                .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;
            let next = if let Some(connected) = input_slot.connected {
                let output_slot = body
                    .slots
                    .get_output(&connected)
                    .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;
                if output_slot.data_ty.name() != input_slot.data.ty().name() {
                    ctx.resources
                        .type_registry
                        .try_wgsl_cast(&*output_slot.data_ty, input_slot.data.ty(), next)
                        .ok_or(GraphNodeCodeGenError::FailedToCastVariable)?
                } else {
                    next.clone()
                }
            } else {
                next.clone()
            };
            code.push_str(&format!(
                "{} = {next};
",
                current[&local.id]
            ));
        }

        let break_conditions = body
            .nodes
            .values()
            .filter(|node| node.data.is::<BreakBeforeNextIterationNode>())
            .filter_map(|node| {
                let input_id = node.inputs.first()?;
                let slot = body.slots.get_input(input_id)?;
                if let Some(connected) = slot.connected {
                    body_output_slot_idents.get(&connected).cloned()
                } else {
                    slot.data.to_code()
                }
            })
            .map(|ident| format!("({ident})"))
            .collect::<Vec<_>>();
        if break_conditions.is_empty() {
            return Err(anyhow!("While body has no break condition.").into());
        }

        let condition = break_conditions.join(" || ");
        code.push_str(&format!(
            "if {condition} {{ break; }}
"
        ));
        code.push_str(
            "}
",
        );

        for (slot_id, local) in ctx.outputs.iter().zip(locals.values()) {
            ctx.output_slot_idents
                .insert(*slot_id, current[&local.id].clone());
        }

        Ok(code)
    }

    fn subgraphs<'a>(&self, state: &'a Self::State) -> Vec<&'a Graph<Data>> {
        vec![&state.body]
    }

    fn subgraphs_mut<'a>(&mut self, state: &'a mut Self::State) -> Vec<&'a mut Graph<Data>> {
        vec![&mut state.body]
    }
}
