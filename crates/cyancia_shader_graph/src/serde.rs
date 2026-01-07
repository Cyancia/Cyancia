use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    sync::Arc,
};

use cyancia_id::Id;
use serde::{Deserialize, Serialize};

use crate::{
    ErasedGraphNodeCreator, ErasedGraphValueType, Graph, GraphDynamicInstancesStorage,
    GraphFunctionSignature, GraphInputSlotData, GraphNodeData, GraphOutputSlotData, GraphSlots,
    GraphTypeCastersStorage, GraphVarIdentGenerator, GraphVariable,
};

#[derive(Debug, thiserror::Error)]
pub enum GraphDeserializeError {
    #[error("Value type not found: {0}")]
    TypeNotFound(String),
    #[error("Node type not found: {0}")]
    NodeNotFound(String),
    #[error("Unmatched input slot count on node {node:?}: expected {expected}, found {found}")]
    UnmatchedInputSlotCount {
        node: Id<GraphNodeData>,
        expected: usize,
        found: usize,
    },
    #[error("Unmatched output slot count on node {node:?}: expected {expected}, found {found}")]
    UnmatchedOutputSlotCount {
        node: Id<GraphNodeData>,
        expected: usize,
        found: usize,
    },
    #[error("Missing input slot on node {0:?}: {1:?}")]
    MissingInputSlot(Id<GraphNodeData>, Id<GraphInputSlotData>),
    #[error("Missing output slot on node {0:?}: {1:?}")]
    MissingOutputSlot(Id<GraphNodeData>, Id<GraphOutputSlotData>),
    #[error("Input slot {1:?} on node {0:?} is connected to missing output slot {2:?}")]
    MissingConnectedSlot(
        Id<GraphNodeData>,
        Id<GraphInputSlotData>,
        Id<GraphOutputSlotData>,
    ),
}

#[derive(Serialize, Deserialize)]
pub struct SerializableGraph {
    pub nodes: Vec<SerializableNodeData>,
    pub inputs: HashMap<Id<GraphInputSlotData>, SerializableInputSlotData>,
    pub outputs: HashMap<Id<GraphOutputSlotData>, SerializableOutputSlotData>,
    pub signature: SerializableGraphFunctionSignature,
    pub ident_generator: SerializableGraphIdentGenerator,
}

impl SerializableGraph {
    pub fn from_graph(graph: &Graph) -> Self {
        let nodes = graph
            .nodes
            .iter()
            .map(|(node_id, node)| SerializableNodeData {
                id: *node_id,
                position: node.position.into(),
                inputs: node.inputs.clone(),
                outputs: node.outputs.clone(),
                data: GraphNodeTypeId {
                    name: node.data.name().to_string(),
                },
            })
            .collect();

        let inputs = graph
            .slots
            .inputs
            .iter()
            .map(|(id, slot)| {
                (
                    *id,
                    SerializableInputSlotData {
                        connected: slot.connected,
                    },
                )
            })
            .collect();

        let outputs = graph
            .slots
            .outputs
            .iter()
            .map(|(id, slot)| {
                (
                    *id,
                    SerializableOutputSlotData {
                        variable_name: slot.data.identifier().to_string(),
                    },
                )
            })
            .collect();

        let signature = SerializableGraphFunctionSignature {
            name: graph.signature.name.clone(),
            ret_type: GraphValueTypeId {
                name: graph.signature.ret_type.name().to_string(),
            },
            params: graph
                .signature
                .params
                .iter()
                .map(|param| SerializableGraphVariable {
                    identifier: param.identifier().to_string(),
                    ty: GraphValueTypeId {
                        name: param.ty().name().to_string(),
                    },
                })
                .collect(),
        };

        let ident_generator = SerializableGraphIdentGenerator {
            counter: graph.ident_generator.counter,
        };

        Self {
            nodes,
            inputs,
            outputs,
            signature,
            ident_generator,
        }
    }

    pub fn into_graph(
        self,
        storage: Arc<GraphDynamicInstancesStorage>,
    ) -> (Option<Graph>, Vec<GraphDeserializeError>) {
        let mut errors = Vec::new();

        let mut signature_params = Vec::with_capacity(self.signature.params.len());
        for param in self.signature.params {
            match storage.types.get(&param.ty.name) {
                Some(t) => {
                    signature_params.push(GraphVariable {
                        identifier: param.identifier.clone(),
                        ty: dyn_clone::clone_box(&**t),
                    });
                }
                None => {
                    return (
                        None,
                        with_error(
                            errors,
                            GraphDeserializeError::TypeNotFound(param.ty.name.clone()),
                        ),
                    );
                }
            }
        }

        let signature = GraphFunctionSignature {
            name: self.signature.name,
            ret_type: match storage.types.get(&self.signature.ret_type.name) {
                Some(t) => dyn_clone::clone_box(&**t),
                None => {
                    return (
                        None,
                        with_error(
                            errors,
                            GraphDeserializeError::TypeNotFound(
                                self.signature.ret_type.name.clone(),
                            ),
                        ),
                    );
                }
            },
            params: signature_params,
        };
        let mut inputs = HashMap::with_capacity(self.inputs.len());
        let mut outputs = HashMap::with_capacity(self.outputs.len());

        let mut nodes = HashMap::with_capacity(self.nodes.len());
        'node_loop: for node in self.nodes {
            let Some(creator) = storage.creators.get(&node.data.name) else {
                errors.push(GraphDeserializeError::NodeNotFound(node.data.name.clone()));
                continue;
            };

            let data = creator.create();
            let raw_inputs = data.create_inputs();
            if raw_inputs.len() != node.inputs.len() {
                errors.push(GraphDeserializeError::UnmatchedInputSlotCount {
                    node: node.id,
                    expected: raw_inputs.len(),
                    found: node.inputs.len(),
                });
                continue;
            }
            let mut node_inputs = HashMap::with_capacity(raw_inputs.len());
            for (index, default) in raw_inputs.into_iter().enumerate() {
                let id = node.inputs[index];
                let Some(slot) = self.inputs.get(&id) else {
                    errors.push(GraphDeserializeError::MissingInputSlot(node.id, id));
                    continue 'node_loop;
                };

                node_inputs.insert(
                    id,
                    GraphInputSlotData {
                        node_id: node.id,
                        name: default.name,
                        data: default.value,
                        connected: slot.connected,
                        slot_type: default.slot_type,
                    },
                );
            }

            let raw_outputs = data.create_outputs();
            if raw_outputs.len() != node.outputs.len() {
                errors.push(GraphDeserializeError::UnmatchedOutputSlotCount {
                    node: node.id,
                    expected: raw_outputs.len(),
                    found: node.outputs.len(),
                });
                continue;
            }
            let mut node_outputs = HashMap::with_capacity(raw_outputs.len());
            for (index, default) in raw_outputs.into_iter().enumerate() {
                let id = node.outputs[index];
                let Some(slot) = self.outputs.get(&id) else {
                    errors.push(GraphDeserializeError::MissingOutputSlot(node.id, id));
                    continue 'node_loop;
                };

                node_outputs.insert(
                    id,
                    GraphOutputSlotData {
                        node_id: node.id,
                        name: default.name,
                        data: GraphVariable {
                            identifier: slot.variable_name.clone(),
                            ty: default.ty,
                        },
                        // Will be done later
                        connected: HashSet::new(),
                    },
                );
            }

            nodes.insert(
                node.id,
                GraphNodeData {
                    position: node.position.into(),
                    data,
                    inputs: node.inputs.clone(),
                    outputs: node.outputs.clone(),
                },
            );
            inputs.extend(node_inputs);
            outputs.extend(node_outputs);
        }

        for (input_id, input) in &mut inputs {
            if let Some(connected_id) = input.connected {
                match outputs.entry(connected_id) {
                    Entry::Occupied(mut e) => {
                        e.get_mut().connected.insert(*input_id);
                    }
                    Entry::Vacant(_) => {
                        errors.push(GraphDeserializeError::MissingConnectedSlot(
                            input.node_id,
                            *input_id,
                            connected_id,
                        ));
                        input.connected = None;
                    }
                }
            }
        }

        let ident_generator = GraphVarIdentGenerator {
            counter: self.ident_generator.counter,
        };

        let graph = Graph {
            nodes,
            slots: GraphSlots { inputs, outputs },
            storage,
            ident_generator,
            signature,
            cached_run_order: None,
        };

        (Some(graph), errors)
    }
}

fn with_error(
    mut errors: Vec<GraphDeserializeError>,
    error: GraphDeserializeError,
) -> Vec<GraphDeserializeError> {
    errors.push(error);
    errors
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct GraphValueTypeId {
    pub name: String,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct GraphNodeTypeId {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableNodeData {
    pub id: Id<GraphNodeData>,
    pub data: GraphNodeTypeId,
    pub position: [f32; 2],
    pub inputs: Vec<Id<GraphInputSlotData>>,
    pub outputs: Vec<Id<GraphOutputSlotData>>,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableInputSlotData {
    pub connected: Option<Id<GraphOutputSlotData>>,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableOutputSlotData {
    pub variable_name: String,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableGraphFunctionSignature {
    pub name: String,
    pub ret_type: GraphValueTypeId,
    pub params: Vec<SerializableGraphVariable>,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableGraphVariable {
    pub identifier: String,
    pub ty: GraphValueTypeId,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableGraphIdentGenerator {
    pub counter: usize,
}
