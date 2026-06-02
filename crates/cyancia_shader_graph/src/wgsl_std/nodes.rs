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
use cyancia_utils::{count, random_oklch, themed_color::themed_oklch, wrapper};
use cyancia_widgets::curve_edit::{CurveEdit, CurveEditEvent, CurveEditState};
use glam::{Vec2, Vec3, Vec3Swizzles, Vec4};
use gpui::{
    AnyElement, App, AppContext, Canvas, Entity, ParentElement, Pixels, Rgba, SharedString, Styled,
    div, px, rgb, rgba,
};
use gpui_component::{
    IndexPath, Sizable,
    input::{Input, InputEvent, InputState},
    searchable_list::SearchableListItem,
    select::{SearchableVec, Select, SelectEvent, SelectItem, SelectState},
};
use indexmap::{IndexMap, map::Entry};
use parking_lot::{RwLock, RwLockReadGuard};
use parse_display::Display;
use serde::{Deserialize, Serialize, de::value};
use uuid::Uuid;

use crate::{
    graph::{
        Graph, GraphData, GraphVarIdentGenerator,
        external::{ExternalVariableId, generate_external_variable_name},
        function::GraphFunctionId,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreateSlotsContext,
            GraphNodeRenderContext, GraphNodeRunContext, GraphNodeUpdateSignatureContext,
            StatelessCommonGraphNode,
        },
        slot::{ErasedGraphValueType, GraphDefaultInputSlot, GraphDefaultOutputSlot},
        texture::TextureId,
        variable::GraphTypeRegistry,
    },
    save::GraphSerializable,
    wgsl_std::types::{ColorType, F32Type, RectType, TextureReference, TextureType, Vec2FType},
};

use crate::graph::node::GraphNodeRunError;
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
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        match state {
            ScalarMathNodeMode::Add | ScalarMathNodeMode::Max | ScalarMathNodeMode::Min => vec![
                GraphDefaultInputSlot::new::<F32Type>("A".into(), 0.0),
                GraphDefaultInputSlot::new::<F32Type>("B".into(), 0.0),
            ],
            ScalarMathNodeMode::Subtract => vec![
                GraphDefaultInputSlot::new::<F32Type>("Minuend".into(), 0.0),
                GraphDefaultInputSlot::new::<F32Type>("Subtrahend".into(), 0.0),
            ],
            ScalarMathNodeMode::Multiply => vec![
                GraphDefaultInputSlot::new::<F32Type>("A".into(), 1.0),
                GraphDefaultInputSlot::new::<F32Type>("B".into(), 1.0),
            ],
            ScalarMathNodeMode::Divide => vec![
                GraphDefaultInputSlot::new::<F32Type>("Dividend".into(), 0.0),
                GraphDefaultInputSlot::new::<F32Type>("Divisor".into(), 1.0),
            ],
            ScalarMathNodeMode::Pow => vec![
                GraphDefaultInputSlot::new::<F32Type>("Base".into(), 1.0),
                GraphDefaultInputSlot::new::<F32Type>("Exponent".into(), 2.0),
            ],
            ScalarMathNodeMode::Acosh => {
                vec![GraphDefaultInputSlot::new::<F32Type>("X".into(), 1.0)]
            }
            ScalarMathNodeMode::Ln
            | ScalarMathNodeMode::Log2
            | ScalarMathNodeMode::Sqrt
            | ScalarMathNodeMode::InverseSqrt => {
                vec![GraphDefaultInputSlot::new::<F32Type>("X".into(), 1.0)]
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
                vec![GraphDefaultInputSlot::new::<F32Type>("X".into(), 0.0)]
            }
        }
    }

    fn create_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
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
                            .map(|i| IndexPath::new(i)),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state,
                    window,
                    move |state: &mut Entity<SelectState<_>>,
                          select,
                          event: &SelectEvent<_>,
                          window,
                          cx| match event {
                        SelectEvent::Confirm(Some(val)) => {
                            let val = *val;
                            graph.update(cx, |graph, cx| {
                                graph.update_node_state::<Self>(cx, node_id, move |state| {
                                    *state = val;
                                });
                            });
                        }
                        _ => {}
                    },
                )
                .detach();

                state
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(&select_state).small())
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
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        match state {
            VectorMathNodeMode::Add | VectorMathNodeMode::Max | VectorMathNodeMode::Min => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("A".into(), Vec2::ZERO),
                GraphDefaultInputSlot::new::<Vec2FType>("B".into(), Vec2::ZERO),
            ],
            VectorMathNodeMode::Subtract => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("Minuend".into(), Vec2::ZERO),
                GraphDefaultInputSlot::new::<Vec2FType>("Subtrahend".into(), Vec2::ZERO),
            ],
            VectorMathNodeMode::Multiply => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("A".into(), Vec2::ONE),
                GraphDefaultInputSlot::new::<Vec2FType>("B".into(), Vec2::ONE),
            ],
            VectorMathNodeMode::Divide => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("Dividend".into(), Vec2::ZERO),
                GraphDefaultInputSlot::new::<Vec2FType>("Divisor".into(), Vec2::ONE),
            ],
            VectorMathNodeMode::Pow => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("Base".into(), Vec2::ONE),
                GraphDefaultInputSlot::new::<Vec2FType>("Exponent".into(), Vec2::splat(2.0)),
            ],
            VectorMathNodeMode::Distance | VectorMathNodeMode::Dot => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("A".into(), Vec2::ZERO),
                GraphDefaultInputSlot::new::<Vec2FType>("B".into(), Vec2::ZERO),
            ],
            VectorMathNodeMode::Reflect => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("Incident".into(), Vec2::new(1.0, -1.0)),
                GraphDefaultInputSlot::new::<Vec2FType>("Normal".into(), Vec2::Y),
            ],
            VectorMathNodeMode::Mix => vec![
                GraphDefaultInputSlot::new::<Vec2FType>("A".into(), Vec2::ZERO),
                GraphDefaultInputSlot::new::<Vec2FType>("B".into(), Vec2::ONE),
                GraphDefaultInputSlot::new::<Vec2FType>("Factor".into(), Vec2::splat(0.5)),
            ],
            VectorMathNodeMode::Acosh => {
                vec![GraphDefaultInputSlot::new::<Vec2FType>(
                    "X".into(),
                    Vec2::ONE,
                )]
            }
            VectorMathNodeMode::Ln
            | VectorMathNodeMode::Log2
            | VectorMathNodeMode::Sqrt
            | VectorMathNodeMode::InverseSqrt => {
                vec![GraphDefaultInputSlot::new::<Vec2FType>(
                    "X".into(),
                    Vec2::ONE,
                )]
            }
            VectorMathNodeMode::Length => {
                vec![GraphDefaultInputSlot::new::<Vec2FType>(
                    "Vector".into(),
                    Vec2::ZERO,
                )]
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
                vec![GraphDefaultInputSlot::new::<Vec2FType>(
                    "X".into(),
                    Vec2::ZERO,
                )]
            }
        }
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
                            .map(|i| IndexPath::new(i)),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state,
                    window,
                    move |state: &mut Entity<SelectState<_>>,
                          select,
                          event: &SelectEvent<_>,
                          window,
                          cx| match event {
                        SelectEvent::Confirm(Some(val)) => {
                            let val = *val;
                            graph.update(cx, |graph, cx| {
                                graph.update_node_state::<Self>(cx, node_id, move |state| {
                                    *state = val;
                                });
                            });
                        }
                        _ => {}
                    },
                )
                .detach();

                state
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(&select_state).small())
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
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        match state {
            RectMathNodeMode::Union | RectMathNodeMode::Intersection => vec![
                GraphDefaultInputSlot::new::<RectType>("A".into(), Rect::EMPTY),
                GraphDefaultInputSlot::new::<RectType>("B".into(), Rect::EMPTY),
            ],
            RectMathNodeMode::Inflate | RectMathNodeMode::Shrink => {
                vec![
                    GraphDefaultInputSlot::new::<RectType>("Rect".into(), Rect::EMPTY),
                    GraphDefaultInputSlot::new::<Vec2FType>("Amount".into(), Vec2::ZERO),
                ]
            }
        }
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
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
                            .map(|i| IndexPath::new(i)),
                        window,
                        cx,
                    )
                });

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state,
                    window,
                    move |state: &mut Entity<SelectState<_>>,
                          select,
                          event: &SelectEvent<_>,
                          window,
                          cx| match event {
                        SelectEvent::Confirm(Some(val)) => {
                            let val = *val;
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        *state = val;
                                    });
                                })
                                .ok();
                        }

                        _ => {}
                    },
                )
                .detach();
                state
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(&select_state).small())
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(TimeNode, cx)
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(ClampNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("Value".into(), 0.0),
            GraphDefaultInputSlot::new::<F32Type>("Min".into(), 0.0),
            GraphDefaultInputSlot::new::<F32Type>("Max".into(), 1.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(StepNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("Edge".into(), 0.0),
            GraphDefaultInputSlot::new::<F32Type>("X".into(), 0.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(SmoothStepNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("Edge0".into(), 0.0),
            GraphDefaultInputSlot::new::<F32Type>("Edge1".into(), 1.0),
            GraphDefaultInputSlot::new::<F32Type>("X".into(), 0.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(SplitComponentsNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<Vec2FType>(
            "Vector".into(),
            Vec2::ZERO,
        )]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(CombineComponentsNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("X".into(), 0.0),
            GraphDefaultInputSlot::new::<F32Type>("Y".into(), 0.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(CombineColorComponentsNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<F32Type>("R".into(), 0.0),
            GraphDefaultInputSlot::new::<F32Type>("G".into(), 0.0),
            GraphDefaultInputSlot::new::<F32Type>("B".into(), 0.0),
            GraphDefaultInputSlot::new::<F32Type>("A".into(), 1.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(SplitColorComponentsNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<ColorType>(
            "Color".into(),
            Vec4::ZERO,
        )]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(GetPixelColorNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<TextureType>("Texture".into(), TextureReference::NULL),
            GraphDefaultInputSlot::new::<Vec2FType>("Position".into(), Vec2::ZERO),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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
                    move |state: &mut Entity<SelectState<_>>,
                          select,
                          event: &SelectEvent<_>,
                          window,
                          cx| match event {
                        SelectEvent::Confirm(Some(val)) => {
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        *state = *val;
                                    });
                                })
                                .ok();
                        }

                        _ => {}
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(ColorMixNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![
            GraphDefaultInputSlot::new::<ColorType>("Color A".into(), Vec4::ZERO),
            GraphDefaultInputSlot::new::<ColorType>("Color B".into(), Vec4::ZERO),
            GraphDefaultInputSlot::new::<F32Type>("Factor".into(), 0.0),
        ]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(TextureSizeNode, cx)
    }

    fn create_inputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<TextureType>(
            "Texture".into(),
            TextureReference::NULL,
        )]
    }

    fn create_outputs(
        &self,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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
            .map(|(slot, var)| {
                GraphDefaultInputSlot::new_boxed_default(
                    var.identifier().to_string(),
                    var.ty().clone(),
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
            .map(|(slot, var)| {
                GraphDefaultOutputSlot::new_boxed(var.identifier().to_string(), var.ty().clone())
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
                id: id.clone(),
                name: graph.name.clone(),
            })
            .collect::<Vec<_>>();
        let cur_ref = all_refs
            .iter()
            .position(|r| Some(&r.id) == state.id.as_ref())
            .map(|i| IndexPath::new(i));
        let graph = ctx.cx.entity().downgrade();

        let select_state = ctx
            .window
            .use_keyed_state(*ctx.node_id, ctx.cx, |window, cx| {
                let state = cx.new(|cx| SelectState::new(all_refs, cur_ref, window, cx));

                let node_id = ctx.node_id;
                cx.subscribe_in(
                    &state,
                    window,
                    move |state: &mut Entity<SelectState<_>>,
                          select,
                          event: &SelectEvent<_>,
                          window,
                          cx| match event {
                        SelectEvent::Confirm(Some(val)) => {
                            let val = *val;
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(cx, node_id, move |state| {
                                        state.id = Some(val);
                                    });
                                })
                                .ok();
                        }
                        _ => {}
                    },
                )
                .detach();

                state
            });

        let select_state = select_state.read(ctx.cx);
        ctx.render_all_slots_with_header(Select::new(&select_state).small())
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
            .read(ctx.cx)
            .run(ctx.data, input_values, ctx.cx)
            .map_err(|e| GraphNodeRunError::Custom(e.into()))?;

        for (slot_id, output_value) in ctx.outputs.iter().zip(output_values) {
            ctx.output_storage.insert(*slot_id, output_value);
        }

        Ok(())
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
        state: &Self::State,
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
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
            .and_then(|ty| ctx.type_registry.get_type(ty).cloned())
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
                    let node_id = node_id.clone();
                    let graph = graph.clone();
                    move |state, input, event: &InputEvent, window, cx| match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            let name = input.read(cx).value();
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(
                                        cx,
                                        node_id.clone(),
                                        move |state| {
                                            state.name = name.into();
                                        },
                                    );
                                })
                                .ok();
                        }
                        InputEvent::Change | InputEvent::Focus => {}
                    }
                })
                .detach();
                cx.subscribe_in(&ty_state, window, {
                    let node_id = node_id.clone();
                    move |state, select, event: &SelectEvent<_>, window, cx| match event {
                        SelectEvent::Confirm(Some(val)) => {
                            let val = *val;
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(
                                        cx,
                                        node_id.clone(),
                                        move |state| {
                                            state.ty = Some(val);
                                        },
                                    );
                                })
                                .ok();
                        }
                        _ => {}
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
                .child(Select::new(&ty_state).small()),
        )
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
        _ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        let Some(ty) = state
            .ty
            .and_then(|ty| _ctx.type_registry.get_type(ty).cloned())
        else {
            return vec![];
        };

        vec![GraphDefaultInputSlot::new_boxed_default(
            state.name.clone(),
            ty,
        )]
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
                    move |state, input, event: &InputEvent, window, cx| match event {
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            let name = input.read(cx).value();
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(
                                        cx,
                                        node_id.clone(),
                                        move |state| {
                                            state.name = name.into();
                                        },
                                    );
                                })
                                .ok();
                        }
                        InputEvent::Change | InputEvent::Focus => {}
                    }
                })
                .detach();
                cx.subscribe_in(&ty_state, window, {
                    let node_id = ctx.node_id;
                    move |state, select, event: &SelectEvent<_>, window, cx| match event {
                        SelectEvent::Confirm(Some(val)) => {
                            let val = *val;
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(
                                        cx,
                                        node_id.clone(),
                                        move |state| {
                                            state.ty = Some(val);
                                        },
                                    );
                                })
                                .ok();
                        }
                        _ => {}
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
                .child(Select::new(&ty_state).small()),
        )
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
                Some(var) => vec![GraphDefaultOutputSlot::new_boxed(
                    var.name.clone(),
                    var.value.ty().clone(),
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
                id: entry.id.clone(),
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
                    let node_id = node_id.clone();
                    move |state, select, event: &SelectEvent<_>, window, cx| match event {
                        SelectEvent::Confirm(Some(val)) => {
                            graph
                                .update(cx, |graph, cx| {
                                    graph.update_node_state::<Self>(
                                        cx,
                                        node_id.clone(),
                                        move |state| {
                                            *state = Some(*val);
                                        },
                                    );
                                })
                                .ok();
                        }
                        _ => {}
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
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>("X".into(), 0.0)]
    }

    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
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
                        move |_, edit, event: &CurveEditEvent, window, cx| match event {
                            CurveEditEvent::ControlPointsChanged => {
                                let edit = edit.read(cx);
                                let control_points = edit.value().control_points().to_vec();
                                graph.update(cx, |graph, cx| {
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

    fn header_color(&self, cx: &App) -> Rgba {
        random_oklch!(RandomNode, cx)
    }

    fn create_inputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
    ) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>("Seed".into(), 0.0)]
    }

    fn create_outputs(
        &self,
        ctx: GraphNodeCreateSlotsContext<'_, Data>,
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
