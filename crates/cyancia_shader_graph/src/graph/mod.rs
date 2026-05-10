use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    sync::Arc,
};

use iced_core::Point;
use indexmap::IndexMap;
use parking_lot::{RwLock, RwLockReadGuard};
use uuid::Uuid;

use crate::graph::{
    external::GraphExternalVariableStorage,
    function::GraphFunctionStorage,
    node::{
        ContextualGraphNodeCodeGenError, ContextualGraphNodeRunError, ErasedGraphNode,
        ErasedGraphNodeMessage, GraphNode, GraphNodeCodeGenContext, GraphNodeCreateSlotsContext,
        GraphNodeData, GraphNodeId, GraphNodeRunContext, GraphNodeUpdateSignatureContext,
        StatefulGraphNode,
    },
    slot::{
        ErasedGraphValueType, GraphDefaultInputSlot, GraphDefaultOutputSlot, GraphInputSlotData,
        GraphInputSlotId, GraphOutputSlotData, GraphOutputSlotId, GraphSlots,
    },
    texture::{GraphTextureStorage, GraphTextureUsageRecorder},
    variable::{GraphLiteral, GraphTypeRegistry, GraphVariable},
};

pub mod external;
pub mod function;
pub mod node;
pub mod slot;
pub mod texture;
pub mod variable;

pub struct Graph<Data: GraphData> {
    pub(crate) nodes: HashMap<GraphNodeId, GraphNodeData<Data>>,
    pub(crate) slots: GraphSlots,
    pub(crate) resources: GraphResources<Data>,
    pub(crate) type_registry: Arc<GraphTypeRegistry>,
    pub(crate) cached_run_order: RwLock<Option<Vec<GraphNodeId>>>,
    pub(crate) cached_signature: RwLock<Option<GraphSignature>>,
}

impl<Data: GraphData> Graph<Data> {
    pub fn new(resources: GraphResources<Data>, type_registry: Arc<GraphTypeRegistry>) -> Self {
        Self {
            nodes: HashMap::new(),
            slots: GraphSlots::default(),
            resources,
            type_registry,
            cached_run_order: Default::default(),
            cached_signature: Default::default(),
        }
    }

    pub fn add_boxed_node(
        &mut self,
        position: Point,
        node: Box<dyn ErasedGraphNode<Data>>,
    ) -> GraphNodeId {
        let node = StatefulGraphNode::new(node);
        let node_id = GraphNodeId::new(Uuid::new_v4());
        let inputs = create_input_slots(
            &mut self.slots,
            node_id,
            node.create_inputs(GraphNodeCreateSlotsContext {
                resources: &self.resources,
                type_registry: &self.type_registry,
            }),
        )
        .into();
        let outputs = create_output_slots(
            &mut self.slots,
            node_id,
            node.create_outputs(GraphNodeCreateSlotsContext {
                resources: &self.resources,
                type_registry: &self.type_registry,
            }),
        )
        .into();

        self.nodes.insert(
            node_id,
            GraphNodeData {
                position,
                inputs,
                outputs,
                data: node,
            },
        );
        self.invalidate_cache();
        node_id
    }

    pub fn add_node<T: GraphNode<Data>>(&mut self, position: Point, node: T) -> GraphNodeId {
        self.add_boxed_node(position, Box::new(node))
    }

    pub fn delete_node(&mut self, id: &GraphNodeId) {
        if let Some(node) = self.nodes.remove(id) {
            delete_all_inputs(&mut self.slots, &node.inputs);
            delete_all_outputs(&mut self.slots, &node.outputs);
            self.invalidate_cache();
        }
    }

    pub fn get_node(&self, id: &GraphNodeId) -> Option<&GraphNodeData<Data>> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &GraphNodeId) -> Option<&mut GraphNodeData<Data>> {
        self.nodes.get_mut(id)
    }

    pub fn connect_slots(&mut self, from: GraphOutputSlotId, to: GraphInputSlotId) {
        if !self.can_connect_slots(from, to) {
            return;
        }

        if let Some(input_slot) = self.slots.inputs.get_mut(&to)
            && let Some(output_slot) = self.slots.outputs.get_mut(&from)
        {
            input_slot.connected = Some(from);
            output_slot.connected.insert(to);
            self.invalidate_cache();
        }
    }

    pub fn can_connect_slots(&self, from: GraphOutputSlotId, to: GraphInputSlotId) -> bool {
        let from_slot = self.slots.outputs.get(&from);
        let to_slot = self.slots.inputs.get(&to);

        if let (Some(from), Some(to)) = (from_slot, to_slot) {
            from.data_ty.name() == to.data.ty().name()
                || self
                    .type_registry
                    .can_cast(&*from.data_ty, to.data.ty().as_ref())
        } else {
            false
        }
    }

    pub fn disconnect_slot(&mut self, to: GraphInputSlotId) {
        if let Some(input_slot) = self.slots.inputs.get_mut(&to)
            && let Some(output_slot) = input_slot
                .connected
                .and_then(|output_id| self.slots.outputs.get_mut(&output_id))
        {
            input_slot.connected = None;
            output_slot.connected.remove(&to);
            self.invalidate_cache();
        }
    }

    pub fn connect_slots_by_index(
        &mut self,
        from_node: GraphNodeId,
        from_output_index: usize,
        to_node: GraphNodeId,
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
            self.connect_slots(from, to);
            self.invalidate_cache();
        }
    }

    pub fn disconnect_slots_by_index(&mut self, to_node: GraphNodeId, to_input_index: usize) {
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

    pub fn invalidate_cache(&self) {
        self.cached_run_order.write().take();
        self.cached_signature.write().take();
    }

    pub fn update_run_order_cache(&self) {
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
            .filter(|(_, degree)| **degree == 0)
            .map(|(node_id, _)| *node_id)
            .collect::<VecDeque<_>>();

        while let Some(node_id) = ready_nodes.pop_front() {
            run_order.push(node_id);
            let node = self.nodes.get(&node_id).unwrap();

            for input_slot_id in node.inputs.iter() {
                let Some(from_node_id) = self
                    .slots
                    .get_connected(input_slot_id)
                    .map(|slot| slot.node_id)
                else {
                    continue;
                };

                // println!(
                //     "Visiting node {:?} from {:?} {}",
                //     node_id,
                //     from_node_id,
                //     out_degrees.get(&from_node_id).unwrap_or(&usize::MAX)
                // );
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
        self.cached_run_order.write().replace(run_order);
    }

    pub fn find_loops(&self) -> Vec<Vec<GraphNodeId>> {
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut loops = Vec::new();

        for node_id in self.nodes.keys() {
            self.find_loops_dfs(*node_id, &mut visited, &mut stack, &mut loops);
        }

        loops
    }

    fn find_loops_dfs(
        &self,
        node_id: GraphNodeId,
        visited: &mut HashSet<GraphNodeId>,
        stack: &mut Vec<GraphNodeId>,
        loops: &mut Vec<Vec<GraphNodeId>>,
    ) {
        if stack.contains(&node_id) {
            let loop_start_index = stack.iter().position(|&id| id == node_id).unwrap();
            let loop_nodes = stack[loop_start_index..].to_vec();
            loops.push(loop_nodes);
            return;
        }

        if !visited.insert(node_id) {
            return;
        }

        stack.push(node_id);

        if let Some(node) = self.nodes.get(&node_id) {
            for output_slot_id in node.outputs.iter() {
                for input_slot_id in self
                    .slots
                    .outputs
                    .get(output_slot_id)
                    .map(|slot| &slot.connected)
                    .into_iter()
                    .flatten()
                {
                    if let Some(connected_node_id) = self
                        .slots
                        .inputs
                        .get(input_slot_id)
                        .map(|slot| slot.node_id)
                    {
                        self.find_loops_dfs(connected_node_id, visited, stack, loops);
                    }
                }
            }
        }

        stack.pop();
    }

    pub fn update_signature_cache(&self) {
        if self.cached_run_order.read().is_none() {
            self.update_run_order_cache();
        }

        let run_order = self.cached_run_order.read();
        let run_order = run_order.as_ref().unwrap();
        let mut signature = GraphSignature::default();
        for node_id in run_order.clone() {
            let node = self.nodes.get(&node_id).unwrap();
            let ctx = GraphNodeUpdateSignatureContext {
                inputs: &node.inputs,
                outputs: &node.outputs,
                slots: &self.slots,
                signature: &mut signature,
                resources: &self.resources,
                type_registry: &self.type_registry,
            };
            node.data.update_signature(ctx);
        }
        self.cached_signature.write().replace(signature);
    }

    pub fn update_node(&mut self, message: ErasedGraphNodeMessage) {
        let Some(node) = self.nodes.get_mut(&message.id) else {
            return;
        };

        let node_id = message.id;
        node.update(
            message,
            &mut self.slots,
            &self.resources,
            &self.type_registry,
        );

        let new_inputs = node.data.create_inputs(GraphNodeCreateSlotsContext {
            resources: &self.resources,
            type_registry: &self.type_registry,
        });
        let new_outputs = node.data.create_outputs(GraphNodeCreateSlotsContext {
            resources: &self.resources,
            type_registry: &self.type_registry,
        });

        if new_inputs.len() == node.inputs.len() {
            let mut inputs_changed = false;
            for (input_slot_id, new_input_slot) in node.inputs.iter().zip(new_inputs) {
                let Some(input_slot) = self.slots.inputs.get_mut(input_slot_id) else {
                    continue;
                };

                if input_slot.data.ty().name() != new_input_slot.value.ty().name() {
                    input_slot.data = new_input_slot.value;
                    inputs_changed = true;
                }
            }

            if inputs_changed {
                disconnect_all_inputs(&mut self.slots, &node.inputs);
                self.cached_run_order.write().take();
            }
        } else {
            delete_all_inputs(&mut self.slots, &node.inputs);
            node.inputs = create_input_slots(&mut self.slots, node_id, new_inputs).into();
            self.cached_run_order.write().take();
        }

        if new_outputs.len() == node.outputs.len() {
            let mut outputs_changed = false;
            for (output_slot_id, new_output_slot) in node.outputs.iter().zip(new_outputs) {
                let Some(output_slot) = self.slots.outputs.get_mut(output_slot_id) else {
                    continue;
                };

                if output_slot.data_ty.name() != new_output_slot.ty.name() {
                    output_slot.data_ty = new_output_slot.ty;
                    outputs_changed = true;
                }
            }

            if outputs_changed {
                disconnect_all_outputs(&mut self.slots, &node.outputs);
                self.cached_run_order.write().take();
            }
        } else {
            delete_all_outputs(&mut self.slots, &node.outputs);
            node.outputs = create_output_slots(&mut self.slots, node_id, new_outputs).into();
            self.cached_run_order.write().take();
        }
    }

    pub fn signature(&self) -> GraphSignature {
        if self.cached_signature.read().is_none() {
            self.update_signature_cache();
        }
        self.cached_signature.read().as_ref().unwrap().clone()
    }

    pub fn compile(
        &self,
        graph_input_idents: Vec<String>,
        mut ident_generator: GraphVarIdentGenerator,
        texture_usage: &mut GraphTextureUsageRecorder,
    ) -> Result<(Vec<String>, String), GraphCompileError> {
        if self.cached_run_order.read().is_none() {
            self.update_run_order_cache();
        }
        if self.cached_signature.read().is_none() {
            self.update_signature_cache();
        }

        let run_order = self.cached_run_order.read();
        let signature = self.cached_signature.read();

        let run_order = run_order.as_ref().unwrap();
        let signature = signature.as_ref().unwrap();
        if signature.inputs.len() != graph_input_idents.len() {
            return Err(GraphCompileError::IncorrectInputParams {
                expected: signature.inputs.len(),
                found: graph_input_idents.len(),
            });
        }

        let mut output_slot_idents = HashMap::with_capacity(graph_input_idents.len());
        for (slot_id, ident) in signature.inputs.keys().zip(graph_input_idents) {
            output_slot_idents.insert(*slot_id, ident);
        }

        let mut code = String::new();
        for node_id in run_order.clone() {
            let node = self.nodes.get(&node_id).unwrap();

            let context = GraphNodeCodeGenContext {
                inputs: &node.inputs,
                outputs: &node.outputs,
                graph_slots: &self.slots,
                output_slot_idents: &mut output_slot_idents,
                ident_generator: &mut ident_generator,
                resources: &self.resources,
                type_registry: &self.type_registry,
                texture_usage,
            };

            match node.data.generate_code(context) {
                Ok(node_code) => code.push_str(&node_code),
                Err(err) => {
                    return Err(GraphCompileError::NodeCodeGenError(
                        ContextualGraphNodeCodeGenError {
                            node_id,
                            node_title: node.data.name().to_string(),
                            err,
                            code: code.clone(),
                        },
                    ));
                }
            }
        }

        let graph_output_idents = signature
            .outputs
            .keys()
            .filter_map(|input_slot_id| {
                let input_slot = self.slots.inputs.get(input_slot_id)?;
                if let Some(connected_output_id) = input_slot.connected {
                    output_slot_idents.get(&connected_output_id).cloned()
                } else {
                    input_slot.data.to_code()
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            graph_output_idents.len(),
            signature.outputs.len(),
            "This should never fail."
        );

        Ok((graph_output_idents, code))
    }

    pub fn run(
        &self,
        data: &Data,
        graph_input_values: Vec<GraphLiteral>,
    ) -> Result<Vec<GraphLiteral>, GraphRunError> {
        if self.cached_run_order.read().is_none() {
            self.update_run_order_cache();
        }
        if self.cached_signature.read().is_none() {
            self.update_signature_cache();
        }

        let run_order = self.cached_run_order.read();
        let signature = self.cached_signature.read();

        let run_order = run_order.as_ref().unwrap();
        let signature = signature.as_ref().unwrap();
        if signature.inputs.len() != graph_input_values.len() {
            return Err(GraphRunError::IncorrectInputParams {
                expected: signature.inputs.len(),
                found: graph_input_values.len(),
            });
        }

        let mut output_storage = HashMap::new();
        for (slot_id, value) in signature.inputs.keys().zip(graph_input_values) {
            output_storage.insert(*slot_id, value);
        }

        for node_id in run_order.iter() {
            let node = self.nodes.get(node_id).unwrap();

            let context = GraphNodeRunContext {
                data,
                inputs: &node.inputs,
                outputs: &node.outputs,
                graph_slots: &self.slots,
                output_storage: &mut output_storage,
                resources: &self.resources,
                type_registry: &self.type_registry,
            };

            match node.data.run(context) {
                Ok(()) => {}
                Err(err) => {
                    // log::error!(
                    //     "Error running node {:?} ({:?}): {:?}",
                    //     node_id,
                    //     node.data.name(),
                    //     err
                    // );
                    // return Err(GraphRunError::NodeRunError(ContextualGraphNodeRunError {
                    //     node_id: *node_id,
                    //     node_title: node.data.name().to_string(),
                    //     err,
                    // }));
                    // TODO Some nodes are only ran on GPU and only available on GPU.
                }
            }
        }

        let graph_output_values = signature
            .outputs
            .keys()
            .filter_map(|input_slot_id| {
                let input_slot = self.slots.inputs.get(input_slot_id)?;
                if let Some(connected_output_id) = input_slot.connected {
                    output_storage.get(&connected_output_id).cloned()
                } else {
                    Some(input_slot.data.clone())
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            graph_output_values.len(),
            signature.outputs.len(),
            "This should never fail."
        );

        Ok(graph_output_values)
    }

    pub fn resources(&self) -> &GraphResources<Data> {
        &self.resources
    }

    pub fn type_registry(&self) -> &Arc<GraphTypeRegistry> {
        &self.type_registry
    }
}

fn create_input_slots(
    slots: &mut GraphSlots,
    node_id: GraphNodeId,
    raw_inputs: Vec<GraphDefaultInputSlot>,
) -> Vec<GraphInputSlotId> {
    let mut inputs = Vec::with_capacity(raw_inputs.len());
    for slot in raw_inputs {
        let slot_id = GraphInputSlotId::new(Uuid::new_v4());
        slots.inputs.insert(
            slot_id,
            GraphInputSlotData {
                node_id,
                data: slot.value,
                connected: None,
            },
        );
        inputs.push(slot_id);
    }
    inputs
}

fn create_output_slots(
    slots: &mut GraphSlots,
    node_id: GraphNodeId,
    raw_outputs: Vec<GraphDefaultOutputSlot>,
) -> Vec<GraphOutputSlotId> {
    let mut outputs = Vec::with_capacity(raw_outputs.len());
    for slot in raw_outputs {
        let slot_id = GraphOutputSlotId::new(Uuid::new_v4());
        slots.outputs.insert(
            slot_id,
            GraphOutputSlotData {
                node_id,
                data_ty: slot.ty,
                connected: HashSet::new(),
            },
        );
        outputs.push(slot_id);
    }
    outputs
}

fn disconnect_all_inputs(slots: &mut GraphSlots, input_slot_ids: &[GraphInputSlotId]) {
    for input_slot_id in input_slot_ids {
        if let Some(input_slot) = slots.inputs.get_mut(input_slot_id)
            && let Some(output_slot) = input_slot
                .connected
                .and_then(|output_id| slots.outputs.get_mut(&output_id))
        {
            input_slot.connected = None;
            output_slot.connected.remove(&input_slot_id);
        }
    }
}

fn disconnect_all_outputs(slots: &mut GraphSlots, output_slot_ids: &[GraphOutputSlotId]) {
    for output_slot_id in output_slot_ids {
        if let Some(output_slot) = slots.outputs.get_mut(output_slot_id) {
            for input_slot_id in &output_slot.connected {
                if let Some(input_slot) = slots.inputs.get_mut(input_slot_id) {
                    input_slot.connected = None;
                }
            }
            output_slot.connected.clear();
        }
    }
}

fn delete_all_inputs(slots: &mut GraphSlots, input_slot_ids: &[GraphInputSlotId]) {
    for input_slot_id in input_slot_ids {
        if let Some(input_slot) = slots.inputs.remove(input_slot_id)
            && let Some(output_slot) = input_slot
                .connected
                .and_then(|output_id| slots.outputs.get_mut(&output_id))
        {
            output_slot.connected.remove(&input_slot_id);
        }
    }
}

fn delete_all_outputs(slots: &mut GraphSlots, output_slot_ids: &[GraphOutputSlotId]) {
    for output_slot_id in output_slot_ids {
        if let Some(output_slot) = slots.outputs.remove(output_slot_id) {
            for input_slot_id in output_slot.connected {
                if let Some(input_slot) = slots.inputs.get_mut(&input_slot_id) {
                    input_slot.connected = None;
                }
            }
        }
    }
}

#[derive(Default, Clone)]
pub struct GraphResources<Data: GraphData> {
    pub textures: Arc<GraphTextureStorage>,
    pub functions: Arc<GraphFunctionStorage<Data>>,
    pub external_vars: Arc<GraphExternalVariableStorage>,
}

#[derive(Default, Clone)]
pub struct GraphSignature {
    pub inputs: IndexMap<GraphOutputSlotId, GraphVariable>,
    pub outputs: IndexMap<GraphInputSlotId, GraphVariable>,
}

#[derive(Debug, thiserror::Error)]
pub enum GraphCompileError {
    #[error("{0}")]
    NodeCodeGenError(ContextualGraphNodeCodeGenError),
    #[error("Expected {expected} input(s), but found {found}")]
    IncorrectInputParams { expected: usize, found: usize },
    #[error(transparent)]
    CustomError(anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum GraphRunError {
    #[error("{0}")]
    NodeRunError(ContextualGraphNodeRunError),
    #[error("Expected {expected} input(s), but found {found}")]
    IncorrectInputParams { expected: usize, found: usize },
    #[error("{0}")]
    CustomError(anyhow::Error),
}

#[derive(Default)]
pub struct GraphVarIdentGenerator {
    suffix: String,
    output_counter: usize,
}

impl GraphVarIdentGenerator {
    pub fn new(suffix: String) -> Self {
        Self {
            suffix,
            output_counter: 0,
        }
    }

    pub fn next_output(&mut self) -> String {
        let ident = format!("output_{}_{}", self.output_counter, self.suffix);
        self.output_counter += 1;
        ident
    }
}

pub trait GraphData: Send + Sync + 'static {}
