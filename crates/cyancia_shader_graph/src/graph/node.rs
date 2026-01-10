use std::any::Any;

use cyancia_id::Id;
use iced_core::{Color, Element, Point};
use iced_widget::Column;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    GraphRenderer, GraphTheme,
    editor::slot::input_slot,
    graph::{
        slot::{
            ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot,
            GraphInputSlotData, GraphOutputSlotData, GraphSlots,
        },
        variable::GraphTypeCastersStorage,
    },
};

pub trait GraphNode: Send + Sync + 'static {
    type State: Send + Sync + 'static + Serialize + DeserializeOwned;
    type Message: Send + Sync + 'static;
    fn name(&self) -> &'static str;
    fn default_state(&self) -> Self::State;
    fn header_color(&self) -> Color;
    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot>;
    fn view_body(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer>;
    fn update_body(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        ctx: GraphNodeUpdateContext,
    );
    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError>;
    fn serialize_state(&self, state: &Self::State) -> Result<toml::Value, toml::ser::Error> {
        toml::Value::try_from(state)
    }
    fn deserialize_state(&self, value: toml::Value) -> Result<Self::State, toml::de::Error> {
        Self::State::deserialize(value)
    }
}

pub struct ErasedGraphNodeMessage {
    pub inner: Box<dyn Any + Send + Sync>,
    pub id: Id<GraphNodeData>,
}

pub trait ErasedGraphNode: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn default_state(&self) -> Box<dyn Any + Send + Sync>;
    fn header_color(&self) -> Color;
    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot>;
    fn view(
        &self,
        node_id: Id<GraphNodeData>,
        state: &dyn Any,
        ctx: GraphNodeViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer>;
    fn update(
        &self,
        state: &mut dyn Any,
        message: ErasedGraphNodeMessage,
        ctx: GraphNodeUpdateContext,
    );
    fn generate_code(
        &self,
        state: &dyn Any,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError>;
    fn serialize_state(&self, state: &dyn Any) -> Result<toml::Value, toml::ser::Error>;
    fn deserialize_state(
        &self,
        value: toml::Value,
    ) -> Result<Box<dyn Any + Send + Sync>, toml::de::Error>;
}

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

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        self.create_inputs()
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        self.create_outputs()
    }

    fn view(
        &self,
        node_id: Id<GraphNodeData>,
        state: &dyn Any,
        ctx: GraphNodeViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.view_body(state, ctx)
            .map(move |msg| ErasedGraphNodeMessage {
                inner: Box::new(msg),
                id: node_id,
            })
    }

    fn update(
        &self,
        state: &mut dyn Any,
        message: ErasedGraphNodeMessage,
        ctx: GraphNodeUpdateContext,
    ) {
        let state = state
            .downcast_mut::<T::State>()
            .expect("Failed to downcast node state.");
        let msg = message
            .inner
            .downcast::<T::Message>()
            .expect("Failed to downcast node message.");
        self.update_body(state, *msg, ctx);
    }

    fn generate_code(
        &self,
        state: &dyn Any,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.generate_code(state, ctx)
    }

    fn serialize_state(&self, state: &dyn Any) -> Result<toml::Value, toml::ser::Error> {
        let state = state
            .downcast_ref::<T::State>()
            .expect("Failed to downcast node state.");
        self.serialize_state(state)
    }

    fn deserialize_state(
        &self,
        value: toml::Value,
    ) -> Result<Box<dyn Any + Send + Sync>, toml::de::Error> {
        let state = self.deserialize_state(value)?;
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

    pub fn view(
        &self,
        node_id: Id<GraphNodeData>,
        ctx: GraphNodeViewContext,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        self.data.view(node_id, &*self.state, ctx)
    }

    pub fn update(&mut self, message: ErasedGraphNodeMessage, ctx: GraphNodeUpdateContext) {
        self.data.update(&mut *self.state, message, ctx);
    }

    pub fn generate_code(
        &self,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        self.data.generate_code(&*self.state, ctx)
    }

    pub fn serialize_state(&self) -> Result<toml::Value, toml::ser::Error> {
        self.data.serialize_state(&*self.state)
    }

    pub fn deserialize_state(&mut self, value: toml::Value) -> Result<(), toml::de::Error> {
        let state = self.data.deserialize_state(value)?;
        self.state = state;
        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct StatelessState {
    #[serde(skip)]
    _private: (),
}

pub trait StatelessCommonGraphNode: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn header_color(&self) -> Color;
    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot>;
    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot>;
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

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        self.create_inputs()
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        self.create_outputs()
    }

    fn view_body(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_inputs())
            .spacing(2)
            .into()
    }

    fn update_body(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        ctx.update_literal(message);
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        self.generate_code(ctx)
    }
}

pub struct GraphNodeData {
    pub position: Point,
    pub data: StatefulGraphNode,
    pub inputs: Vec<Id<GraphInputSlotData>>,
    pub outputs: Vec<Id<GraphOutputSlotData>>,
}

impl GraphNodeData {
    pub fn view(
        &self,
        node_id: Id<GraphNodeData>,
        slots: &GraphSlots,
    ) -> Element<'static, ErasedGraphNodeMessage, GraphTheme, GraphRenderer> {
        self.data.view(
            node_id,
            GraphNodeViewContext {
                inputs: &self.inputs,
                slots,
            },
        )
    }

    pub fn update(&mut self, message: ErasedGraphNodeMessage, slots: &mut GraphSlots) {
        self.data.update(
            message,
            GraphNodeUpdateContext {
                inputs: &self.inputs,
                slots,
            },
        );
    }
}

pub struct GraphNodeViewContext<'a> {
    inputs: &'a [Id<GraphInputSlotData>],
    slots: &'a GraphSlots,
}

impl GraphNodeViewContext<'_> {
    pub fn get_input(&self, index: usize) -> Option<&GraphInputSlotData> {
        let slot_id = self.inputs.get(index)?;
        self.slots.get_input(slot_id)
    }

    pub fn view_input(
        &self,
        index: usize,
    ) -> Option<Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer>> {
        let slot_id = self.inputs.get(index)?;
        let slot = self.slots.get_input(slot_id)?;
        Some(input_slot(*slot_id, slot))
    }

    pub fn view_all_inputs(
        &self,
    ) -> Vec<Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer>> {
        let mut e = Vec::with_capacity(self.inputs.len());
        for id in self.inputs {
            if let Some(slot) = self.slots.get_input(id) {
                e.push(input_slot(*id, slot));
            }
        }
        e
    }

    pub fn input_len(&self) -> usize {
        self.inputs.len()
    }
}

pub struct GraphNodeUpdateContext<'a> {
    inputs: &'a [Id<GraphInputSlotData>],
    slots: &'a mut GraphSlots,
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

pub struct GraphNodeCodeGenContext<'a> {
    pub inputs: &'a [Id<GraphInputSlotData>],
    pub outputs: &'a [Id<GraphOutputSlotData>],
    pub graph_slots: &'a mut GraphSlots,
    pub casters: &'a GraphTypeCastersStorage,
}

impl GraphNodeCodeGenContext<'_> {
    pub fn get_input<const N: usize>(&self) -> Result<String, GraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(N)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;

        let Some(connected) = slot.connected else {
            // Literal value should always has the same type as the slot type.
            return slot
                .data
                .to_code()
                .ok_or(GraphNodeCodeGenError::LiteralToCodeFailed);
        };

        let output_slot = self
            .graph_slots
            .get_output(&connected)
            .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;

        if output_slot.data.ty().name() != slot.data.ty().name() {
            self.casters
                .try_cast(&output_slot.data, slot.data.ty())
                .ok_or(GraphNodeCodeGenError::FailedToCastVariable)
        } else {
            Ok(output_slot.data.identifier().to_string())
        }
    }

    pub fn get_input_raw<const N: usize, T: 'static>(&self) -> Result<&T, GraphNodeCodeGenError> {
        let slot_id = self
            .inputs
            .get(N)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_input(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingInputSlot)?;

        Ok(slot.data.as_ref::<T>())
    }

    pub fn get_output<const N: usize>(&self) -> Result<String, GraphNodeCodeGenError> {
        let slot_id = self
            .outputs
            .get(N)
            .ok_or(GraphNodeCodeGenError::SlotIndexOutOfBounds)?;

        let slot = self
            .graph_slots
            .get_output(slot_id)
            .ok_or(GraphNodeCodeGenError::MissingOutputSlot)?;

        Ok(slot.data.identifier().to_string())
    }
}

#[derive(Default)]
pub struct GraphNodeCreatorStorage {
    creators: IndexMap<&'static str, Box<dyn ErasedGraphNodeCreator>>,
}

impl GraphNodeCreatorStorage {
    pub fn register<T: GraphNodeCreator + Default>(&mut self) {
        let node = T::default().create();
        let creator = Box::new(T::default());
        self.creators.insert(GraphNode::name(&node), creator);
    }

    pub fn register_non_default<T: GraphNodeCreator>(&mut self, creator: T) {
        let node = creator.create();
        let creator = Box::new(creator);
        self.creators.insert(GraphNode::name(&node), creator);
    }

    pub fn get(&self, name: &str) -> Option<&Box<dyn ErasedGraphNodeCreator>> {
        self.creators.get(name)
    }

    pub fn all(&self) -> &IndexMap<&'static str, Box<dyn ErasedGraphNodeCreator>> {
        &self.creators
    }
}

pub trait GraphNodeCreator: 'static {
    type NodeType: GraphNode;
    fn create(&self) -> Self::NodeType;
}

pub trait ErasedGraphNodeCreator: 'static {
    fn create(&self) -> Box<dyn ErasedGraphNode>;
}

impl<T: GraphNodeCreator> ErasedGraphNodeCreator for T {
    fn create(&self) -> Box<dyn ErasedGraphNode> {
        Box::new(self.create())
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
    pub node_id: Id<GraphNodeData>,
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
