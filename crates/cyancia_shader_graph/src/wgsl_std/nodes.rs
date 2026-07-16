use std::sync::atomic::{AtomicU32, Ordering};

use cyancia_math::curve::CubicCurve;
use cyancia_utils::random_oklch;
use cyancia_widgets::curve_edit::{CurveEdit, CurveEditEvent, CurveEditState};
use glam::{Vec2, Vec3, Vec3Swizzles};
use gpui::{
    AnyElement, App, AppContext, Entity, ParentElement, Pixels, Rgba, SharedString, Styled, div, px,
};
use gpui_component::{
    IndexPath, Sizable,
    input::{Input, InputEvent, InputState},
    searchable_list::SearchableListItem,
    select::{SearchableVec, Select, SelectEvent, SelectState},
};
use parse_display::Display;
use serde::{Deserialize, Serialize};

use crate::{
    graph::{
        GraphData, GraphVarIdentGenerator,
        external::{ExternalVariableId, generate_external_variable_name},
        function::GraphFunctionId,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeRenderContext, GraphNodeUpdateSignatureContext, StatelessCommonGraphNode,
        },
        slot::{GraphDefaultInputSlot, GraphDefaultOutputSlot},
        texture::TextureId,
    },
    wgsl_std::types::{ColorType, F32Type, RectType, TextureType, Vec2FType},
};

use cyancia_shader_graph_derive::stateless;

const NODE_HEADER_FIELD_GAP: Pixels = px(2.0);

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
}

impl SearchableListItem for ScalarMathNodeMode {
    type Value = Self;

    fn title(&self) -> SharedString {
        format!("{}", self).into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

impl<Data: GraphData> GraphNode<Data> for ScalarMathNode {
    type State = ScalarMathNodeMode;

    fn name(&self) -> &'static str {
        "Scalar Math"
    }

    fn default_state(&self) -> Self::State {
        ScalarMathNodeMode::Add
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(ScalarMathNode, cx)
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

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(ScalarMathNodeMode::ALL),
                        ScalarMathNodeMode::ALL
                            .iter()
                            .position(|mode| mode == state)
                            .map(IndexPath::new),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let val = *val;
                            let _ = graph.update(cx, |graph, cx| {
                                graph.update_node_state::<Self>(cx, node_id, move |state| {
                                    *state = val;
                                });
                            });
                        }
                    },
                )
                .detach();

                state
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(select_state).small())
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext<'_, Data>,
    ) -> Result<String, GraphNodeCodeGenError> {
        let input_a = ctx.get_input(0)?;
        let input_b = ctx.get_input(1)?;
        let __c = ctx.get_input(2)?;
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

impl SearchableListItem for VectorMathNodeMode {
    type Value = Self;

    fn title(&self) -> SharedString {
        format!("{}", self).into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

impl<Data: GraphData> GraphNode<Data> for VectorMathNode {
    type State = VectorMathNodeMode;

    fn name(&self) -> &'static str {
        "Vector Math"
    }

    fn default_state(&self) -> Self::State {
        VectorMathNodeMode::Add
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(VectorMathNode, cx)
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

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(VectorMathNodeMode::ALL),
                        VectorMathNodeMode::ALL
                            .iter()
                            .position(|mode| mode == state)
                            .map(IndexPath::new),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let val = *val;
                            let _ = graph.update(cx, |graph, cx| {
                                graph.update_node_state::<Self>(cx, node_id, move |state| {
                                    *state = val;
                                });
                            });
                        }
                    },
                )
                .detach();

                state
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(select_state).small())
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

impl SearchableListItem for RectMathNodeMode {
    type Value = Self;

    fn title(&self) -> SharedString {
        format!("{}", self).into()
    }

    fn value(&self) -> &Self::Value {
        self
    }
}

impl<Data: GraphData> GraphNode<Data> for RectMathNode {
    type State = RectMathNodeMode;

    fn name(&self) -> &'static str {
        "Rect Math"
    }

    fn default_state(&self) -> Self::State {
        RectMathNodeMode::Union
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(RectMathNode, cx)
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

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state = cx.new(|cx| {
                    SelectState::new(
                        SearchableVec::new(RectMathNodeMode::ALL),
                        RectMathNodeMode::ALL
                            .iter()
                            .position(|mode| mode == state)
                            .map(IndexPath::new),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let val = *val;
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        *state = val;
                                    });
                                })
                                .ok();
                        }
                    },
                )
                .detach();
                state
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(select_state).small())
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(TimeNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(ClampNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(StepNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(SmoothStepNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(SplitComponentsNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(CombineComponentsNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(CombineColorComponentsNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(SplitColorComponentsNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(GetPixelColorNode, cx)
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

impl<Data: GraphData> GraphNode<Data> for TextureNode {
    type State = TextureId;

    fn name(&self) -> &'static str {
        "Texture"
    }

    fn default_state(&self) -> Self::State {
        TextureId::NULL
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(TextureNode, cx)
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

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let node_id = ctx.node_id;
        let all_textures = ctx
            .resources
            .textures
            .all()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let selected = all_textures
            .iter()
            .position(|r| r.external_id == *state)
            .map(IndexPath::new);

        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let select_state =
                    cx.new(|cx| SelectState::new(all_textures.clone(), selected, window, cx));

                cx.subscribe_in(
                    &select_state,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        *state = *val;
                                    });
                                })
                                .ok();
                        }
                    },
                )
                .detach();

                select_state
            });

        let select_state = select_state.read(ctx.cx).clone();
        select_state.update(ctx.cx, |state, cx| {
            state.set_items(all_textures, ctx.window, cx);
            state.set_selected_index(selected, ctx.window, cx);
        });

        ctx.render_all_slots_with_header(Select::new(&select_state).small())
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(ColorMixNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(TextureSizeNode, cx)
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

impl SearchableListItem for GraphFunctionReference {
    type Value = GraphFunctionId;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.id
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

impl<Data: GraphData> GraphNode<Data> for GraphFunctionNode {
    type State = GraphFunctionNodeState;

    fn name(&self) -> &'static str {
        "Function"
    }

    fn default_state(&self) -> Self::State {
        GraphFunctionNodeState { id: None }
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(GraphFunctionNode, cx)
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
            .read(ctx.cx)
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
        let Some(func) = state
            .id
            .as_ref()
            .and_then(|id| ctx.resources.functions.get(id))
        else {
            return Vec::new();
        };

        func.graph
            .read(ctx.cx)
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

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let all_refs = ctx
            .resources
            .functions
            .all()
            .iter()
            .map(|(id, graph)| GraphFunctionReference {
                id: *id,
                name: graph.name.clone(),
            })
            .collect::<Vec<_>>();
        let cur_ref = all_refs
            .iter()
            .position(|r| Some(&r.id) == state.id.as_ref())
            .map(IndexPath::new);
        let graph = ctx.cx.entity().downgrade();

        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state = cx.new(|cx| SelectState::new(all_refs, cur_ref, window, cx));

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state,
                    window,
                    move |_: &mut Entity<SelectState<_>>, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let val = *val;
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        state.id = Some(val);
                                    });
                                })
                                .ok();
                        }
                    },
                )
                .detach();

                state
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(select_state).small())
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
            .read(ctx.cx)
            .compile(
                input_idents,
                GraphVarIdentGenerator::new(format!(
                    "{}_{}",
                    id.to_string().replace('-', "_"),
                    UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
                )),
                ctx.texture_usage,
                ctx.cx,
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

impl<Data: GraphData> GraphNode<Data> for GraphInputNode {
    type State = GraphInputNodeState;

    fn name(&self) -> &'static str {
        "Graph Input"
    }

    fn default_state(&self) -> Self::State {
        GraphInputNodeState::default()
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(GraphInputNode, cx)
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
            .and_then(|ty| ctx.type_registry.get_type(ty))
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

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let input_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let name_state = cx.new(|cx| InputState::new(window, cx));
                let ty_state = cx.new(|cx| {
                    SelectState::new(
                        ctx.type_registry
                            .all_types()
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>(),
                        state.ty.and_then(|ty| {
                            ctx.type_registry
                                .all_types()
                                .keys()
                                .position(|k| *k == ty)
                                .map(IndexPath::new)
                        }),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(&name_state, window, {
                    let graph = graph.clone();
                    move |_, input, event: &InputEvent, _, cx| match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            let name = input.read(cx).value();
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        state.name = name.into();
                                    });
                                })
                                .ok();
                        }
                        InputEvent::Change | InputEvent::Focus => {}
                    }
                })
                .detach();
                cx.subscribe_in(&ty_state, window, {
                    move |_, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let val = *val;
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        state.ty = Some(val);
                                    });
                                })
                                .ok();
                        }
                    }
                })
                .detach();

                (name_state, ty_state)
            });

        let (name_state, ty_state) = input_state.read(ctx.cx);
        ctx.render_all_slots_with_header(
            div()
                .flex()
                .flex_col()
                .gap(NODE_HEADER_FIELD_GAP)
                .child(Input::new(name_state).small())
                .child(Select::new(ty_state).small()),
        )
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

impl<Data: GraphData> GraphNode<Data> for GraphOutputNode {
    type State = GraphOutputNodeState;

    fn name(&self) -> &'static str {
        "Graph Output"
    }

    fn default_state(&self) -> Self::State {
        GraphOutputNodeState::default()
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(GraphOutputNode, cx)
    }

    fn create_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        let Some(ty) = state
            .ty
            .and_then(|ty| ctx.type_registry.get_type(ty))
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

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let input_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let name_state = cx.new(|cx| InputState::new(window, cx));
                let ty_state = cx.new(|cx| {
                    SelectState::new(
                        ctx.type_registry
                            .all_types()
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>(),
                        state.ty.and_then(|ty| {
                            ctx.type_registry
                                .all_types()
                                .keys()
                                .position(|k| *k == ty)
                                .map(IndexPath::new)
                        }),
                        window,
                        cx,
                    )
                });

                cx.subscribe_in(&name_state, window, {
                    let graph = graph.clone();
                    let node_id = ctx.node_id;
                    move |_, input, event: &InputEvent, _, cx| match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            let name = input.read(cx).value();
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        state.name = name.into();
                                    });
                                })
                                .ok();
                        }
                        InputEvent::Change | InputEvent::Focus => {}
                    }
                })
                .detach();
                cx.subscribe_in(&ty_state, window, {
                    let node_id = ctx.node_id;
                    move |_, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            let val = *val;
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        state.ty = Some(val);
                                    });
                                })
                                .ok();
                        }
                    }
                })
                .detach();

                (name_state, ty_state)
            });

        let (name_state, ty_state) = input_state.read(ctx.cx);
        ctx.render_all_slots_with_header(
            div()
                .flex()
                .flex_col()
                .gap(NODE_HEADER_FIELD_GAP)
                .child(Input::new(name_state).small())
                .child(Select::new(ty_state).small()),
        )
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

impl SearchableListItem for ExternalVariableReference {
    type Value = ExternalVariableId;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }
}

#[derive(Default, Clone)]
pub struct ExternalVariableNode;

impl<Data: GraphData> GraphNode<Data> for ExternalVariableNode {
    type State = Option<ExternalVariableId>;

    fn name(&self) -> &'static str {
        "External Variable"
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(ExternalVariableNode, cx)
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

    fn default_state(&self) -> Self::State {
        None
    }

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let all_refs = ctx
            .resources
            .external_vars
            .all()
            .iter()
            .map(|entry| ExternalVariableReference {
                id: entry.id,
                name: entry.name.clone(),
            })
            .collect::<Vec<_>>();
        let selected = state.as_ref().and_then(|id| {
            all_refs
                .iter()
                .position(|r| r.id == *id)
                .map(IndexPath::new)
        });

        let graph = ctx.cx.entity().downgrade();
        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state = cx.new(|cx| SelectState::new(all_refs.clone(), selected, window, cx));

                let node_id = ctx.node_id;
                cx.subscribe_in(&state, window, {
                    move |_, _, event: &SelectEvent<_>, _, cx| {
                        if let SelectEvent::Confirm(Some(val)) = event {
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        *state = Some(*val);
                                    });
                                })
                                .ok();
                        }
                    }
                })
                .detach();

                state
            });

        let select_state = select_state.read(ctx.cx).clone();
        select_state.update(ctx.cx, |state, cx| {
            state.set_items(all_refs, ctx.window, cx);
            state.set_selected_index(selected, ctx.window, cx);
        });
        ctx.render_all_slots_with_header(Select::new(&select_state).small())
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

impl<Data: GraphData> GraphNode<Data> for CurveNode {
    type State = CurveNodeState;

    fn name(&self) -> &'static str {
        "Curve"
    }

    fn default_state(&self) -> Self::State {
        Default::default()
    }

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(CurveNode, cx)
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

    fn render(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeRenderContext<'_, '_, Data>,
    ) -> AnyElement {
        let graph = ctx.cx.entity().downgrade();
        let edit_state: Entity<Entity<_>> =
            ctx.window
                .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                    let state = cx.new(|cx| {
                        CurveEditState::new(CubicCurve::new(state.control_points.clone()), cx)
                    });

                    cx.subscribe_in(&state, window, {
                        let node_id = ctx.node_id;
                        move |_, edit, event: &CurveEditEvent, _, cx| match event {
                            CurveEditEvent::ControlPointsChanged => {
                                let edit = edit.read(cx);
                                let control_points = edit.value().control_points().to_vec();
                                let _ = graph.update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        state.control_points = control_points;
                                    });
                                });
                            }
                        }
                    })
                    .detach();

                    state
                });

        let state = edit_state.read(ctx.cx);
        ctx.render_all_slots_with_header(
            div().w_full().aspect_square().child(CurveEdit::new(state)),
        )
    }

    // fn view_inputs(
    //     &self,
    //     state: &Self::State,
    //     ctx: GraphNodeInputsViewContext<'_, Data>,
    // ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
    //     Column::with_children(ctx.view_all_inputs(&["X"], CurveNodeMessage::LiteralUpdate))
    //         .push(
    //             CurveEdit::new(CubicCurve::new(state.control_points.clone()))
    //                 .width(NODE_WIDTH)
    //                 .height(NODE_WIDTH * 0.75)
    //                 .on_point_created(CurveNodeMessage::CurvePointCreated)
    //                 .on_point_moved(CurveNodeMessage::CurvePointMoved)
    //                 .on_point_deleted(CurveNodeMessage::CurvePointDeleted),
    //         )
    //         .into()
    // }

    // fn view_outputs(
    //     &self,
    //     state: &Self::State,
    //     ctx: GraphNodeOutputsViewContext<'_, Data>,
    // ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
    //     Column::with_children(ctx.view_all_outputs(&["Y"])).into()
    // }

    // fn update(
    //     &self,
    //     state: &mut Self::State,
    //     message: Self::Message,
    //     mut ctx: GraphNodeUpdateContext<'_, Data>,
    // ) {
    //     match message {
    //         CurveNodeMessage::LiteralUpdate(message) => {
    //             ctx.update_literal(message);
    //         }
    //         CurveNodeMessage::CurvePointCreated(index, position) => {
    //             state.control_points.insert(index, position);
    //         }
    //         CurveNodeMessage::CurvePointMoved(index, position) => {
    //             if let Some(point) = state.control_points.get_mut(index) {
    //                 *point = position;
    //             }
    //         }
    //         CurveNodeMessage::CurvePointDeleted(index) => {
    //             if state.control_points.len() > 2 {
    //                 state.control_points.remove(index);
    //             }
    //         }
    //     }
    // }

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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(RandomNode, cx)
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
