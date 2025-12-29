use std::{
    alloc::{Layout, alloc, dealloc},
    any::{Any, TypeId},
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    mem::ManuallyDrop,
    ptr::{NonNull, copy_nonoverlapping},
};

use cyancia_utils::wrapper;
use iced_core::{Color, Element, Rectangle, Theme, Widget, layout::Node};
use iced_widget::{Renderer, core::Point};
use uuid::Uuid;

pub mod editor;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub InputSlotId : Uuid
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub OutputSlotId : Uuid
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub NodeId : Uuid
}

pub struct Graph {
    pub nodes: HashMap<NodeId, GraphNodeData>,
    pub slots: GraphSlots,
    pub cached_run_order: Option<Vec<NodeId>>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            slots: GraphSlots::default(),
            cached_run_order: None,
        }
    }

    pub fn add_node(&mut self, position: Point, node: Box<dyn GraphNode>) -> NodeId {
        let node_id = NodeId::new(Uuid::new_v4());
        let raw_inputs = node.crate_inputs();
        let mut inputs = Vec::with_capacity(raw_inputs.len());
        for slot in raw_inputs {
            let slot_id = InputSlotId::new(Uuid::new_v4());
            self.slots.inputs.insert(
                slot_id,
                GraphInputSlot {
                    node_id,
                    name: slot.name,
                    value_type: slot.value_type,
                    value: slot.value,
                    connected: None,
                },
            );
            inputs.push(slot_id);
        }

        let raw_outputs = node.crate_outputs();
        let mut outputs = Vec::with_capacity(raw_outputs.len());
        for slot in raw_outputs {
            let slot_id = OutputSlotId::new(Uuid::new_v4());
            self.slots.outputs.insert(
                slot_id,
                GraphOutputSlot {
                    node_id,
                    name: slot.name,
                    value_type: slot.value_type,
                    value: slot.value,
                },
            );
            outputs.push(slot_id);
        }

        self.nodes.insert(
            node_id,
            GraphNodeData {
                position,
                inputs,
                outputs,
                node,
            },
        );
        self.invalidate_cache();
        node_id
    }

    pub fn connect_slot(&mut self, from: OutputSlotId, to: InputSlotId) {
        if let Some(input_slot) = self.slots.inputs.get_mut(&to) {
            input_slot.connected = Some(from);
            self.invalidate_cache();
        }
    }

    pub fn disconnect_slot(&mut self, to: InputSlotId) {
        if let Some(input_slot) = self.slots.inputs.get_mut(&to) {
            input_slot.connected = None;
            self.invalidate_cache();
        }
    }

    pub fn connect_slots_by_index(
        &mut self,
        from_node: NodeId,
        from_output_index: usize,
        to_node: NodeId,
        to_input_index: usize,
    ) {
        let from_slot = self
            .nodes
            .get(&from_node)
            .and_then(|node| node.outputs.get(from_output_index))
            .cloned();
        let to_slot = self
            .nodes
            .get(&to_node)
            .and_then(|node| node.inputs.get(to_input_index))
            .cloned();

        if let (Some(from), Some(to)) = (from_slot, to_slot) {
            self.connect_slot(from, to);
            self.invalidate_cache();
        }
    }

    pub fn disconnect_slots_by_index(&mut self, to_node: NodeId, to_input_index: usize) {
        let to_slot = self
            .nodes
            .get(&to_node)
            .and_then(|node| node.inputs.get(to_input_index))
            .cloned();

        if let Some(to) = to_slot {
            self.disconnect_slot(to);
            self.invalidate_cache();
        }
    }

    pub fn run_node(&mut self, id: NodeId) -> Result<(), GraphError> {
        let node = self.nodes.get(&id).ok_or(GraphError::NodeNotFound(id))?;
        let context = GraphNodeSlotsContext {
            inputs: &node.inputs,
            outputs: &node.outputs,
            graph_slots: &mut self.slots,
        };
        node.node.run(context)
    }

    pub fn invalidate_cache(&mut self) {
        self.cached_run_order = None;
    }

    pub fn update_cache(&mut self) {
        let mut out_degrees = self
            .nodes
            .iter()
            .map(|(node_id, node)| {
                (
                    *node_id,
                    node.outputs
                        .iter()
                        .map(|output_id| {
                            self.slots
                                .inputs
                                .iter()
                                .filter(|(_, slot)| slot.connected == Some(*output_id))
                                .count()
                        })
                        .sum::<usize>(),
                )
            })
            .collect::<HashMap<_, _>>();

        let mut run_order = Vec::with_capacity(self.nodes.len());
        let mut ready_nodes = out_degrees
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(node_id, _)| *node_id)
            .collect::<VecDeque<_>>();

        while let Some(node_id) = ready_nodes.pop_front() {
            run_order.push(node_id);
            let node = self.nodes.get(&node_id).unwrap();

            for input_slot_id in &node.inputs {
                let Some(from_node_id) = self
                    .slots
                    .get_connected(input_slot_id)
                    .map(|slot| slot.node_id)
                else {
                    continue;
                };

                println!(
                    "Visiting node {:?} from {:?} {}",
                    node_id,
                    from_node_id,
                    out_degrees.get(&from_node_id).unwrap_or(&usize::MAX)
                );
                let Entry::Occupied(out_degree_of_from_node) = out_degrees.entry(from_node_id)
                else {
                    continue;
                };

                if *out_degree_of_from_node.get() == 1 {
                    out_degree_of_from_node.remove();
                    ready_nodes.push_back(from_node_id);
                } else {
                    *out_degree_of_from_node.into_mut() -= 1;
                }
            }
        }

        run_order.reverse();
        self.cached_run_order = Some(dbg!(run_order));
    }
}

#[derive(Default)]
pub struct GraphSlots {
    pub inputs: HashMap<InputSlotId, GraphInputSlot>,
    pub outputs: HashMap<OutputSlotId, GraphOutputSlot>,
}

impl GraphSlots {
    pub fn get_connected(&self, input_slot_id: &InputSlotId) -> Option<&GraphOutputSlot> {
        let input_slot = self.inputs.get(&input_slot_id)?;
        let output_slot_id = input_slot.connected?;
        self.outputs.get(&output_slot_id)
    }
}

pub struct GraphNodeData {
    pub position: Point,
    pub inputs: Vec<InputSlotId>,
    pub outputs: Vec<OutputSlotId>,
    pub node: Box<dyn GraphNode>,
}

pub struct GraphNodeSlotsContext<'a> {
    pub inputs: &'a [InputSlotId],
    pub outputs: &'a [OutputSlotId],
    pub graph_slots: &'a mut GraphSlots,
}

impl GraphNodeSlotsContext<'_> {
    pub fn get_input<const I: usize, T: 'static>(&self) -> Result<&T, GraphError> {
        let slot = self
            .inputs
            .get(I)
            .and_then(|id| self.graph_slots.inputs.get(id))
            .ok_or_else(|| GraphError::InputSlotNotFoundAt(I))?;

        let value;
        if let Some(connected) = slot.connected {
            let connected = self
                .graph_slots
                .outputs
                .get(&connected)
                .ok_or_else(|| GraphError::OutputSlotNotFound(connected))?;
            value = &connected.value;
        } else {
            value = &slot.value;
        }

        if value.is_empty() {
            return Err(GraphError::EmptyInputSlot(slot.name));
        }

        value.as_ref::<T>().ok_or_else(|| {
            GraphError::InputSlotTypeMismatch(
                slot.name,
                slot.value_type.type_name(),
                std::any::type_name::<T>(),
            )
        })
    }

    pub fn set_output<const I: usize, T: 'static>(&mut self, value: T) -> Result<(), GraphError> {
        let slot = self
            .outputs
            .get(I)
            .and_then(|id| self.graph_slots.outputs.get_mut(id))
            .ok_or_else(|| GraphError::OutputSlotNotFoundAt(I))?;

        slot.value.reset(value);
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Input slot not found at index {0}")]
    InputSlotNotFoundAt(usize),
    #[error("Input slot not found with id {0:?}")]
    InputSlotNotFound(InputSlotId),
    #[error("Input slot '{0}' is empty")]
    EmptyInputSlot(&'static str),
    #[error("Input slot '{0}' type mismatch: containing '{1}', requesting '{2}'")]
    InputSlotTypeMismatch(&'static str, &'static str, &'static str),
    #[error("Output slot not found at index {0}")]
    OutputSlotNotFoundAt(usize),
    #[error("Output slot not found with id {0:?}")]
    OutputSlotNotFound(OutputSlotId),
    #[error("Output slot '{0}' type mismatch: containing '{1}', requesting '{2}'")]
    OutputSlotTypeMismatch(&'static str, &'static str, &'static str),
    #[error("Node not found with id {0:?}")]
    NodeNotFound(NodeId),
}

pub struct DefaultGraphSlot {
    pub name: &'static str,
    pub value_type: Box<dyn GraphSlotValueType>,
    pub value: ErasedSlotValue,
}

impl std::fmt::Debug for DefaultGraphSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultGraphSlot")
            .field("name", &self.name)
            .field("value_type", &self.value_type.type_name())
            .field(
                "value",
                if self.value.is_empty() {
                    &"None"
                } else {
                    &"Some"
                },
            )
            .finish()
    }
}

pub trait GraphNode: 'static {
    fn header_color(&self) -> Color;
    fn name(&self) -> &'static str;
    fn crate_inputs(&self) -> Vec<DefaultGraphSlot>;
    fn crate_outputs(&self) -> Vec<DefaultGraphSlot>;
    fn run(&self, slots: GraphNodeSlotsContext<'_>) -> Result<(), GraphError>;
}

pub trait GraphNodeCreator: 'static {
    fn name(&self) -> &'static str;
    fn create(&self) -> Box<dyn GraphNode>;
}

pub struct GraphInputSlot {
    pub node_id: NodeId,
    pub name: &'static str,
    pub value_type: Box<dyn GraphSlotValueType>,
    pub value: ErasedSlotValue,
    pub connected: Option<OutputSlotId>,
}

pub struct GraphOutputSlot {
    pub node_id: NodeId,
    pub name: &'static str,
    pub value_type: Box<dyn GraphSlotValueType>,
    pub value: ErasedSlotValue,
}

pub struct ErasedSlotValue {
    data: Option<Box<dyn Any>>,
}

impl ErasedSlotValue {
    pub fn empty<T: 'static>() -> Self {
        Self { data: None }
    }

    pub fn new<T: 'static>(data: T) -> Self {
        Self {
            data: Some(Box::new(data)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_none()
    }

    pub fn clear(&mut self) {
        self.data = None;
    }

    pub fn as_ref<T: 'static>(&self) -> Option<&T> {
        if let Some(data) = &self.data {
            data.downcast_ref::<T>()
        } else {
            None
        }
    }

    pub fn reset<T: 'static>(&mut self, value: T) {
        self.data = Some(Box::new(value));
    }
}

pub trait GraphSlotValueType {
    fn type_name(&self) -> &'static str;
    fn color(&self) -> Color;
}
