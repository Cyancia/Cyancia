use cyancia_math::curve::CubicCurve;
use cyancia_widgets::curve_edit::CurveEdit;
use glam::Vec2;
use iced_core::{Element, color};
use iced_widget::Column;
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    editor::NODE_WIDTH,
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext, StatelessCommonGraphNode,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::{F32Type, Vec2FType},
};

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

    fn create_inputs(&self, state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
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
let {} = render::math::sample_cubic_curve(
    render::math::CubicCurve(
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
        &["Scalar Value", "Vec2 Value"]
    }

    fn header_color(&self) -> iced_core::Color {
        color!(0x79edf2)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![GraphDefaultInputSlot::new::<F32Type>(0.0)]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
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
            "let {} = render::hash::hash11({});\nlet {} = render::hash::hash21({});\n",
            ctx.get_output(0)?,
            ctx.get_input(0)?,
            ctx.get_output(1)?,
            ctx.get_input(0)?
        ))
    }
}
