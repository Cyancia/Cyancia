use cyancia_math::curve::CubicCurve;
use cyancia_widgets::curve_edit::CurveEdit;
use glam::Vec2;
use iced_core::{Element, color};
use iced_widget::Column;
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
    },
    wgsl_std::types::F32Type,
};

#[derive(Default, Clone)]
pub struct CurveNode;

#[derive(Default, Serialize, Deserialize)]
pub struct CurveNodeState {
    pub control_points: Vec<Vec2>,
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
                    .width(400)
                    .height(300)
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
                state.control_points.remove(index);
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        todo!()
    }
}
