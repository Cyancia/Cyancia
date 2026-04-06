use std::{
    any::Any,
    collections::{BTreeMap, HashMap, hash_map::Entry},
    convert::identity,
    sync::Arc,
};

use cyancia_utils::{cloneable_any::ClonableAnySync, wrapper};
use dyn_clone::DynClone;
use iced_core::{Color, Element, Point};
use iced_widget::Column;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    GraphRenderer, GraphTheme,
    editor::slot::{input_slot, output_slot},
    graph::{
        GraphResources, GraphSignature, GraphVarIdentGenerator,
        slot::{
            ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot,
            GraphInputSlotData, GraphInputSlotId, GraphOutputSlotData, GraphOutputSlotId,
            GraphSlots,
        },
        texture::{GraphTextureStorage, GraphTextureUsageRecorder},
        variable::{GraphLiteralValue, GraphTypeRegistry, GraphVariable},
    },
    save::GraphSerializable,
};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub GraphNodeId : Uuid
}

pub trait GraphNode: Send + Sync + 'static + DynClone {
    type State: Send + Sync + 'static + GraphSerializable;
    type Message: Send + Sync + 'static + Clone;
    fn name(&self) -> &'static str;
    fn default_state(&self) -> Self::State;
    fn header_color(&self) -> Color;
    fn create_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot>;
    fn update_signature(&self, state: &Self::State, ctx: GraphNodeUpdateSignatureContext) {}
    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer>;
    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer>;
    fn update(&self, state: &mut Self::State, message: Self::Message, ctx: GraphNodeUpdateContext);
    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError>;
    fn serialize_state(&self, state: &Self::State) -> Result<toml::Value, toml::ser::Error> {
        state.to_toml()
    }
    fn deserialize_state(
        &self,
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<Self::State, toml::de::Error> {
        Self::State::from_toml(value, type_registry)
    }
}

#[derive(Clone)]
pub struct ErasedGraphNodeMessage {
    pub inner: Box<dyn ClonableAnySync>,
    pub id: GraphNodeId,
}

pub trait ErasedGraphNode: Send + Sync + 'static + DynClone {
    fn name(&self) -> &'static str;
    fn default_state(&self) -> Box<dyn Any + Send + Sync>;
    fn header_color(&self) -> Color;
    fn create_inputs(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot>;
    fn update_signature(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeUpdateSignatureContext,
    );
    fn view_inputs(
        &self,
        node_id: GraphNodeId,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer>;
    fn update(
        &self,
        state: &mut Box<dyn Any + Send + Sync>,
        message: ErasedGraphNodeMessage,
        ctx: GraphNodeUpdateContext,
    );
    fn view_outputs(
        &self,
        node_id: GraphNodeId,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer>;
    fn generate_code(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError>;
    fn serialize_state(
        &self,
        state: &Box<dyn Any + Send + Sync>,
    ) -> Result<toml::Value, toml::ser::Error>;
    fn deserialize_state(
        &self,
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<Box<dyn Any + Send + Sync>, toml::de::Error>;
}

dyn_clone::clone_trait_object!(ErasedGraphNode);

impl<T: GraphNode> ErasedGraphNode for T {
    fn name(&self) -> &'static str {
        self.name()
    }

    fn default_state(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(self.default_state())
    }

    fn header_color(&self) -> Color {
        self.header_color()
    }

    fn create_inputs(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.create_inputs(state, ctx)
    }

    fn create_outputs(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.create_outputs(state, ctx)
    }

    fn update_signature(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeUpdateSignatureContext,
    ) {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.update_signature(state, ctx);
    }

    fn view_inputs(
        &self,
        node_id: GraphNodeId,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.view_inputs(state, ctx)
            .map(move |msg| ErasedGraphNodeMessage {
                inner: Box::new(msg),
                id: node_id,
            })
    }

    fn view_outputs(
        &self,
        node_id: GraphNodeId,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.view_outputs(state, ctx)
            .map(move |msg| ErasedGraphNodeMessage {
                inner: Box::new(msg) as Box<dyn ClonableAnySync>,
                id: node_id,
            })
    }

    fn update(
        &self,
        state: &mut Box<dyn Any + Send + Sync>,
        message: ErasedGraphNodeMessage,
        ctx: GraphNodeUpdateContext,
    ) {
        let state = state
            .downcast_mut::<T::State>()
            .expect("Failed to downcast node state.");
        let msg = match message.inner.downcast::<T::Message>() {
            Ok(ok) => ok,
            Err(_) => panic!("Failed to downcast node message."),
        };
        self.update(state, *msg, ctx);
    }

    fn generate_code(
        &self,
        state: &Box<dyn Any + Send + Sync>,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.generate_code(state, ctx)
    }

    fn serialize_state(
        &self,
        state: &Box<dyn Any + Send + Sync>,
    ) -> Result<toml::Value, toml::ser::Error> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.serialize_state(state)
    }

    fn deserialize_state(
        &self,
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<Box<dyn Any + Send + Sync>, toml::de::Error> {
        let state = self.deserialize_state(value, type_registry)?;
        Ok(Box::new(state))
    }
}

pub struct StatefulGraphNode {
    state: Box<dyn Any + Send + Sync>,
    data: Box<dyn ErasedGraphNode>,
}

impl StatefulGraphNode {
    pub fn new(node: Box<dyn ErasedGraphNode>) -> Self {
        Self {
            state: node.default_state(),
            data: node,
        }
    }

    pub fn name(&self) -> &'static str {
        self.data.name()
    }

    pub fn header_color(&self) -> Color {
        self.data.header_color()
    }

    pub fn view_inputs(
        &self,
        node_id: GraphNodeId,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        self.data.view_inputs(node_id, &self.state, ctx)
    }

    fn view_outputs(
        &self,
        node_id: GraphNodeId,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        self.data.view_outputs(node_id, &self.state, ctx)
    }

    pub fn update(&mut self, message: ErasedGraphNodeMessage, ctx: GraphNodeUpdateContext) {
        self.data.update(&mut self.state, message, ctx);
    }

    pub fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        self.data.generate_code(&self.state, ctx)
    }

    pub fn serialize_state(&self) -> Result<toml::Value, toml::ser::Error> {
        self.data.serialize_state(&self.state)
    }

    pub fn deserialize_and_set_state(
        &mut self,
        value: toml::Value,
        type_registry: &GraphTypeRegistry,
    ) -> Result<(), toml::de::Error> {
        let state = self.data.deserialize_state(value, type_registry)?;
        self.state = state;
        Ok(())
    }

    pub fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot> {
        self.data.create_inputs(&self.state, ctx)
    }

    pub fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot> {
        self.data.create_outputs(&self.state, ctx)
    }

    pub fn update_signature(&self, ctx: GraphNodeUpdateSignatureContext) {
        self.data.update_signature(&self.state, ctx);
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct StatelessState {
    #[serde(skip)]
    _private: (),
}

pub trait StatelessCommonGraphNode: Send + Sync + 'static + DynClone {
    fn name(&self) -> &'static str;
    fn input_slot_names(&self) -> &[&'static str];
    fn output_slot_names(&self) -> &[&'static str];
    fn header_color(&self) -> Color;
    fn create_inputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(&self, ctx: GraphNodeCreateSlotsContext) -> Vec<GraphDefaultOutputSlot>;
    fn generate_code(&self, ctx: GraphNodeCodeGenContext) -> Result<String, GraphNodeCodeGenError>;
}

impl<T: StatelessCommonGraphNode> GraphNode for T {
    type State = StatelessState;

    type Message = ErasedGraphLiteralUpdateMessage;

    fn name(&self) -> &'static str {
        self.name()
    }

    fn default_state(&self) -> Self::State {
        StatelessState::default()
    }

    fn header_color(&self) -> Color {
        self.header_color()
    }

    fn create_inputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultInputSlot> {
        self.create_inputs(ctx)
    }

    fn create_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCreateSlotsContext,
    ) -> Vec<GraphDefaultOutputSlot> {
        self.create_outputs(ctx)
    }

    fn view_inputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_inputs(self.input_slot_names(), identity))
            .spacing(2)
            .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(self.output_slot_names()))
            .spacing(2)
            .into()
    }

    fn update(
        &self,
        _state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        ctx.update_literal(message);
    }

    fn generate_code(
        &self,
        _state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        self.generate_code(ctx)
    }
}

pub struct GraphNodeData {
    pub position: Point,
    pub data: StatefulGraphNode,
    pub inputs: Arc<[GraphInputSlotId]>,
    pub outputs: Arc<[GraphOutputSlotId]>,
}

impl GraphNodeData {
    pub fn view_inputs(
        &self,
        node_id: GraphNodeId,
        slots: &GraphSlots,
        resources: &GraphResources,
        type_registry: &GraphTypeRegistry,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        self.data.view_inputs(
            node_id,
            GraphNodeInputsViewContext {
                inputs: &self.inputs,
                slots,
                resources,
                type_registry,
            },
        )
    }

    pub fn view_outputs(
        &self,
        node_id: GraphNodeId,
        slots: &GraphSlots,
        resources: &GraphResources,
        type_registry: &GraphTypeRegistry,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        self.data.view_outputs(
            node_id,
            GraphNodeOutputsViewContext {
                outputs: &self.outputs,
                slots,
                resources,
                type_registry,
            },
        )
    }

    pub fn update(
        &mut self,
        message: ErasedGraphNodeMessage,
        slots: &mut GraphSlots,
        resources: &GraphResources,
        type_registry: &GraphTypeRegistry,
    ) {
        self.data.update(
            message,
            GraphNodeUpdateContext {
                inputs: &self.inputs,
                slots,
                resources,
                type_registry,
            },
        );
    }
}

pub struct GraphNodeCreateSlotsContext<'a> {
    pub resources: &'a GraphResources,
    pub type_registry: &'a GraphTypeRegistry,
}

pub struct GraphNodeInputsViewContext<'a> {
    pub inputs: &'a [GraphInputSlotId],
    pub slots: &'a GraphSlots,
    pub resources: &'a GraphResources,
    pub type_registry: &'a GraphTypeRegistry,
}

impl GraphNodeInputsViewContext<'_> {
    pub fn get_input(&self, index: usize) -> Option<&GraphInputSlotData> {
        let slot_id = self.inputs.get(index)?;
        self.slots.get_input(slot_id)
    }

    pub fn view_input(
        &self,
        name: &'static str,
        index: usize,
    ) -> Option<Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer>> {
        let slot_id = self.inputs.get(index)?;
        let slot = self.slots.get_input(slot_id)?;
        Some(input_slot(*slot_id, name, slot))
    }

    pub fn view_all_inputs<T: 'static>(
        &self,
        names: &[&'static str],
        f: impl Fn(ErasedGraphLiteralUpdateMessage) -> T + 'static + Copy,
    ) -> Vec<Element<'static, T, GraphTheme, GraphRenderer>> {
        let mut e = Vec::with_capacity(self.inputs.len());
        for (id, name) in self.inputs.iter().zip(names) {
            if let Some(slot) = self.slots.get_input(id) {
                e.push(input_slot(*id, *name, slot).map(f));
            }
        }
        e
    }

    pub fn all_inputs(&self) -> impl Iterator<Item = (&GraphInputSlotId, &GraphInputSlotData)> {
        self.inputs
            .iter()
            .filter_map(move |id| self.slots.get_input(id).map(|slot| (id, slot)))
    }
}

pub struct GraphNodeOutputsViewContext<'a> {
    pub outputs: &'a [GraphOutputSlotId],
    pub slots: &'a GraphSlots,
    pub resources: &'a GraphResources,
    pub type_registry: &'a GraphTypeRegistry,
}

impl GraphNodeOutputsViewContext<'_> {
    pub fn get_output(&self, index: usize) -> Option<&GraphOutputSlotData> {
        let slot_id = self.outputs.get(index)?;
        self.slots.get_output(slot_id)
    }

    pub fn view_output(
        &self,
        name: &'static str,
        index: usize,
    ) -> Option<Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer>> {
        let slot_id = self.outputs.get(index)?;
        let slot = self.slots.get_output(slot_id)?;
        Some(output_slot(*slot_id, name, slot))
    }

    pub fn view_all_outputs<T: 'static>(
        &self,
        names: &[&'static str],
    ) -> Vec<Element<'static, T, GraphTheme, GraphRenderer>> {
        let mut e = Vec::with_capacity(self.outputs.len());
        for (id, name) in self.outputs.iter().zip(names) {
            if let Some(slot) = self.slots.get_output(id) {
                e.push(output_slot(*id, *name, slot));
            }
        }
        e
    }

    pub fn all_outputs(&self) -> impl Iterator<Item = (&GraphOutputSlotId, &GraphOutputSlotData)> {
        self.outputs
            .iter()
            .filter_map(move |id| self.slots.get_output(id).map(|slot| (id, slot)))
    }
}

pub struct GraphNodeUpdateContext<'a> {
    pub inputs: &'a [GraphInputSlotId],
    pub slots: &'a mut GraphSlots,
    pub resources: &'a GraphResources,
    pub type_registry: &'a GraphTypeRegistry,
}

impl GraphNodeUpdateContext<'_> {
    pub fn get_input(&self, index: usize) -> Option<&GraphInputSlotData> {
        let slot_id = self.inputs.get(index)?;
        self.slots.get_input(slot_id)
    }

    pub fn get_input_mut(&mut self, index: usize) -> Option<&mut GraphInputSlotData> {
        let slot_id = self.inputs.get(index)?;
        self.slots.inputs.get_mut(slot_id)
    }

    pub fn update_literal(&mut self, message: ErasedGraphLiteralUpdateMessage) {
        let Some(slot) = self.slots.inputs.get_mut(&message.id) else {
            return;
        };

        slot.data.update(message);
    }
}

pub struct GraphNodeUpdateSignatureContext<'a> {
    pub inputs: &'a [GraphInputSlotId],
    pub outputs: &'a [GraphOutputSlotId],
    pub slots: &'a GraphSlots,
    pub signature: &'a mut GraphSignature,
    pub type_registry: &'a GraphTypeRegistry,
    pub resources: &'a GraphResources,
}

impl GraphNodeUpdateSignatureContext<'_> {
    pub fn require_output_slot_as_graph_input(&mut self, index: usize, name: String) {
        let Some(slot_id) = self.outputs.get(index) else {
            return;
        };
        let Some(slot) = self.slots.outputs.get(slot_id) else {
            return;
        };

        self.signature.inputs.insert(
            *slot_id,
            GraphVariable::new_boxed(name, dyn_clone::clone_box(&*slot.data_ty)),
        );
    }

    pub fn require_input_slot_as_graph_output(&mut self, index: usize, name: String) {
        let Some(slot_id) = self.inputs.get(index) else {
            return;
        };
        let Some(slot) = self.slots.inputs.get(slot_id) else {
            return;
        };

        self.signature.outputs.insert(
            *slot_id,
            GraphVariable::new_boxed(name, slot.data.ty().clone()),
        );
    }
}

pub struct GraphNodeCodeGenContext<'a> {
    pub inputs: &'a [GraphInputSlotId],
    pub outputs: &'a [GraphOutputSlotId],
    pub graph_slots: &'a GraphSlots,
    pub output_slot_idents: &'a mut HashMap<GraphOutputSlotId, String>,
    pub ident_generator: &'a mut GraphVarIdentGenerator,
    pub resources: &'a GraphResources,
    pub type_registry: &'a GraphTypeRegistry,
    pub texture_usage: &'a mut GraphTextureUsageRecorder,
}

impl GraphNodeCodeGenContext<'_> {
    pub fn get_input(&self, index: usize) -> Result<String, GraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(index)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;

        let Some(connected) = slot.connected else {
            dbg!();
            // Literal value should always has the same type as the slot type.
            return slot
                .data
                .to_code()
                .ok_or(GraphNodeCodeGenError::LiteralToCodeFailed);
        };
        dbg!();

        let output_slot = self
            .graph_slots
            .get_output(&connected)
            .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;

        let ident = self
            .output_slot_idents
            .get(&connected)
            .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;

        if output_slot.data_ty.name() != slot.data.ty().name() {
            self.type_registry
                .try_cast(&*output_slot.data_ty, slot.data.ty().as_ref(), ident)
                .ok_or(GraphNodeCodeGenError::FailedToCastVariable)
        } else {
            Ok(ident.clone())
        }
    }

    pub fn get_input_raw<T: GraphLiteralValue>(
        &self,
        index: usize,
    ) -> Result<&T, GraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(index)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;

        Ok(slot.data.as_ref::<T>())
    }

    pub fn get_output(&mut self, index: usize) -> Result<String, GraphNodeCodeGenError> {
        let slot_id = self
            .outputs
            .get(index)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let ident = match self.output_slot_idents.entry(*slot_id) {
            Entry::Occupied(e) => e.get().clone(),
            Entry::Vacant(e) => e.insert(self.ident_generator.next_output()).clone(),
        };
        Ok(ident)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GraphNodeCodeGenError {
    #[error("Input slot index out of bounds")]
    SlotIndexOutOfBounds,
    #[error("Missing input slot")]
    MissingInputSlot,
    #[error("Missing output slot")]
    MissingOutputSlot,
    #[error("Failed to cast variable")]
    FailedToCastVariable,
    #[error("Failed to convert literal to code")]
    LiteralToCodeFailed,
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
}

#[derive(Debug)]
pub struct ContextualGraphNodeCodeGenError {
    pub node_id: GraphNodeId,
    pub node_title: String,
    pub err: GraphNodeCodeGenError,
    pub code: String,
}

impl std::fmt::Display for ContextualGraphNodeCodeGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error in node {:?} of type {}: {}\nCode already generated:\n{}",
            self.node_id, self.node_title, self.err, self.code
        )
    }
}

#[derive(Default, Clone)]
pub struct GraphNodeRegistry {
    nodes: BTreeMap<&'static str, Box<dyn ErasedGraphNode>>,
}

impl GraphNodeRegistry {
    pub fn register<T: ErasedGraphNode + Default>(&mut self) {
        let node = Box::new(T::default());
        self.nodes.insert(node.name(), node);
    }

    pub fn get(&self, name: &str) -> Option<Box<dyn ErasedGraphNode>> {
        self.nodes.get(name).cloned()
    }

    pub fn all(&self) -> &BTreeMap<&'static str, Box<dyn ErasedGraphNode>> {
        &self.nodes
    }

    pub fn merge(&mut self, other: GraphNodeRegistry) {
        self.nodes.extend(other.nodes);
    }
}
