use std::{
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    sync::Arc,
};

use cyancia_id::Id;
use iced_core::Point;

use crate::graph::{
    node::{
        ContextualGraphNodeCodeGenError, ErasedGraphNode, ErasedGraphNodeMessage, GraphNode,
        GraphNodeCodeGenContext, GraphNodeCreatorStorage, GraphNodeData, StatefulGraphNode,
    },
    slot::{
        ErasedGraphValueType, GraphInputSlotData, GraphOutputSlotData, GraphSlots, GraphValueType,
    },
    variable::{
        GraphTypeCastersStorage, GraphValueTypeStorage, GraphVarIdentGenerator, GraphVariable,
    },
};

pub mod node;
pub mod slot;
pub mod variable;

#[derive(Debug, thiserror::Error)]
pub enum GraphCompileError {
    #[error("Invalid function signature")]
    InvalidFunctionSignature,
    #[error("{0}")]
    NodeCodeGenError(ContextualGraphNodeCodeGenError),
    #[error(transparent)]
    CustomError(anyhow::Error),
}

pub struct Graph {
    pub(crate) nodes: HashMap<Id<GraphNodeData>, GraphNodeData>,
    pub(crate) slots: GraphSlots,
    pub(crate) ident_generator: GraphVarIdentGenerator,
    pub(crate) signature: GraphFunctionSignature,
    pub(crate) storage: Arc<GraphDynamicInstancesStorage>,
    pub(crate) cached_run_order: Option<Vec<Id<GraphNodeData>>>,
}

impl Graph {
    pub fn new(
        signature: GraphFunctionSignature,
        storage: Arc<GraphDynamicInstancesStorage>,
    ) -> Self {
        Self {
            nodes: HashMap::new(),
            slots: GraphSlots::default(),
            ident_generator: GraphVarIdentGenerator::default(),
            signature,
            storage,
            cached_run_order: None,
        }
    }

    pub fn add_boxed_node(
        &mut self,
        position: Point,
        node: Box<dyn ErasedGraphNode>,
    ) -> Id<GraphNodeData> {
        let node_id = Id::random();
        let raw_inputs = node.create_inputs();
        let mut inputs = Vec::with_capacity(raw_inputs.len());
        for slot in raw_inputs {
            let slot_id = Id::random();
            self.slots.inputs.insert(
                slot_id,
                GraphInputSlotData {
                    node_id,
                    name: slot.name,
                    data: slot.value,
                    connected: None,
                },
            );
            inputs.push(slot_id);
        }

        let raw_outputs = node.create_outputs();
        let mut outputs = Vec::with_capacity(raw_outputs.len());
        for slot in raw_outputs {
            let slot_id = Id::random();
            self.slots.outputs.insert(
                slot_id,
                GraphOutputSlotData {
                    node_id,
                    name: slot.name,
                    data: GraphVariable::new_boxed(self.ident_generator.next_output(), slot.ty),
                    connected: HashSet::new(),
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
                data: StatefulGraphNode::new(node),
            },
        );
        self.invalidate_cache();
        node_id
    }

    pub fn add_node<T: GraphNode>(&mut self, position: Point, node: T) -> Id<GraphNodeData> {
        self.add_boxed_node(position, Box::new(node))
    }

    pub fn delete_node(&mut self, id: &Id<GraphNodeData>) {
        if let Some(node) = self.nodes.remove(id) {
            for input_slot_id in node.inputs {
                if let Some(input_slot) = self.slots.inputs.remove(&input_slot_id) {
                    if let Some(connected) = input_slot
                        .connected
                        .and_then(|id| self.slots.outputs.get_mut(&id))
                    {
                        connected.connected.remove(&input_slot_id);
                    }
                }
            }
            for output_slot_id in node.outputs {
                if let Some(output_slot) = self.slots.outputs.remove(&output_slot_id) {
                    for connected_id in output_slot.connected {
                        if let Some(connected_slot) = self.slots.inputs.get_mut(&connected_id) {
                            connected_slot.connected = None;
                        }
                    }
                }
            }
            self.invalidate_cache();
        }
    }

    pub fn get_node(&self, id: &Id<GraphNodeData>) -> Option<&GraphNodeData> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &Id<GraphNodeData>) -> Option<&mut GraphNodeData> {
        self.nodes.get_mut(id)
    }

    pub fn connect_slots(&mut self, from: Id<GraphOutputSlotData>, to: Id<GraphInputSlotData>) {
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

    pub fn can_connect_slots(
        &self,
        from: Id<GraphOutputSlotData>,
        to: Id<GraphInputSlotData>,
    ) -> bool {
        let from_slot = self.slots.outputs.get(&from);
        let to_slot = self.slots.inputs.get(&to);

        if let (Some(from), Some(to)) = (from_slot, to_slot) {
            from.data.ty().name() == to.data.ty().name()
                || self.storage.casters.can_cast(from.data.ty(), to.data.ty())
        } else {
            false
        }
    }

    pub fn disconnect_slot(&mut self, to: Id<GraphInputSlotData>) {
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
        from_node: Id<GraphNodeData>,
        from_output_index: usize,
        to_node: Id<GraphNodeData>,
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

    pub fn disconnect_slots_by_index(&mut self, to_node: Id<GraphNodeData>, to_input_index: usize) {
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

    pub fn compile(&mut self) -> Result<String, GraphCompileError> {
        if self.cached_run_order.is_none() {
            self.update_cache();
        }

        let run_order = self.cached_run_order.as_ref().unwrap();
        let mut code = String::new();
        for node_id in run_order.clone() {
            let node = self.nodes.get(&node_id).unwrap();
            let context = GraphNodeCodeGenContext {
                inputs: &node.inputs,
                outputs: &node.outputs,
                graph_slots: &mut self.slots,
                casters: &self.storage.casters,
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
        self.signature
            .compile(code)
            .ok_or(GraphCompileError::InvalidFunctionSignature)
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
        let mut ready_nodes = self
            .nodes
            .iter()
            .filter(|(_, node)| node.outputs.len() == 0)
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
        self.cached_run_order = Some(dbg!(run_order));
    }

    pub fn find_loops(&self) -> Vec<Vec<Id<GraphNodeData>>> {
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
        node_id: Id<GraphNodeData>,
        visited: &mut HashSet<Id<GraphNodeData>>,
        stack: &mut Vec<Id<GraphNodeData>>,
        loops: &mut Vec<Vec<Id<GraphNodeData>>>,
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
            for output_slot_id in &node.outputs {
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

    pub fn update_node(&mut self, message: ErasedGraphNodeMessage) {
        if let Some(node) = self.nodes.get_mut(&message.id) {
            node.update(message, &mut self.slots);
        }
    }

    pub fn storage(&self) -> &Arc<GraphDynamicInstancesStorage> {
        &self.storage
    }
}

#[derive(Default)]
pub struct GraphDynamicInstancesStorage {
    pub creators: GraphNodeCreatorStorage,
    pub types: GraphValueTypeStorage,
    pub casters: GraphTypeCastersStorage,
}

pub struct GraphFunctionSignature {
    name: String,
    ret_type: Box<dyn ErasedGraphValueType>,
    params: Vec<GraphVariable>,
}

impl GraphFunctionSignature {
    pub fn new<T: GraphValueType>(name: String, ret_type: T) -> Self {
        Self {
            name,
            ret_type: Box::new(ret_type),
            params: Vec::new(),
        }
    }

    pub fn new_full(
        name: String,
        ret_type: Box<dyn ErasedGraphValueType>,
        params: Vec<GraphVariable>,
    ) -> Self {
        Self {
            name,
            ret_type,
            params,
        }
    }

    pub fn with_param<T: GraphValueType + Default>(mut self, identifier: String) -> Self {
        self.params.push(GraphVariable::new::<T>(identifier));
        self
    }

    pub fn compile(&self, body: String) -> Option<String> {
        let ret_ty = self.ret_type.wgsl_type()?;

        let params = self
            .params
            .iter()
            .filter_map(|param| {
                param
                    .ty()
                    .wgsl_type()
                    .map(|ty| format!("{}: {}", param.identifier(), ty))
            })
            .collect::<Vec<_>>();
        if params.len() != self.params.len() {
            return None;
        }

        Some(format!(
            "fn {}({}) -> {} {{\n{}\n}}",
            self.name,
            params.join(", "),
            ret_ty,
            body
        ))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ret_type(&self) -> &Box<dyn ErasedGraphValueType> {
        &self.ret_type
    }

    pub fn params(&self) -> &Vec<GraphVariable> {
        &self.params
    }
}
