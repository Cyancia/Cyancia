use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    sync::Arc,
};

use anyhow::Ok;
use cyancia_id::Id;
use serde::{Deserialize, Serialize, de::IntoDeserializer};
use toml::ser::Buffer;

use crate::{
    GraphSerializer,
    graph::{
        Graph, GraphDynamicInstancesStorage, GraphSignature,
        node::{GraphNodeData, StatefulGraphNode},
        slot::{GraphInputSlotData, GraphOutputSlotData, GraphSlots},
        variable::{GraphLiteral, GraphVarIdentGenerator, GraphVariable},
    },
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
    #[error("Failed to deserialize literal data: {0}")]
    LiteralDeserializeError(toml::de::Error),
    #[error("Failed to deserialize node state: {0}")]
    NodeStateDeserializeError(toml::de::Error),
    #[error("Deserialization error: {0}")]
    DeserializerError(toml::de::Error),
}

impl Graph {
    pub fn to_toml(&self) -> Result<String, anyhow::Error> {
        let mut buf = Buffer::new();
        self.serialize(&mut buf)?;
        Ok(buf.to_string())
    }

    pub fn from_toml(
        storage: Arc<GraphDynamicInstancesStorage>,
        s: &str,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let deserializer = match toml::Deserializer::parse(s) {
            Result::Ok(x) => x,
            Err(e) => {
                return (None, vec![GraphDeserializeError::DeserializerError(e)]);
            }
        };
        Self::deserialize(storage, deserializer)
    }

    pub fn serialize<'a>(&self, buf: &mut Buffer) -> Result<(), anyhow::Error> {
        let nodes = self
            .nodes
            .iter()
            .try_fold(Vec::new(), |mut acc, (node_id, node)| {
                acc.push(SerializableNodeData {
                    id: *node_id,
                    position: node.position.into(),
                    inputs: node.inputs.clone(),
                    outputs: node.outputs.clone(),
                    data: GraphNodeTypeId {
                        name: node.data.name().to_string(),
                    },
                    state: node.data.serialize_state()?,
                });

                Ok(acc)
            })?;

        let inputs =
            self.slots
                .inputs
                .iter()
                .try_fold(HashMap::default(), |mut acc, (id, slot)| {
                    acc.insert(
                        *id,
                        SerializableInputSlotData {
                            connected: slot.connected,
                            data: slot.data.ty().serialize_literal(slot.data.value())?,
                        },
                    );

                    Result::<_, anyhow::Error>::Ok(acc)
                })?;

        let outputs = self
            .slots
            .outputs
            .iter()
            .map(|(id, _)| (*id, SerializableOutputSlotData {}))
            .collect();

        let ident_generator = SerializableGraphIdentGenerator {
            counter: self.ident_generator.counter(),
        };

        let signature = SerializableGraphSignature {
            name: self.signature.name().to_string(),
            inputs: self
                .signature
                .inputs()
                .iter()
                .map(|var| SerializableGraphVariable {
                    identifier: var.identifier().to_string(),
                    ty: GraphValueTypeId {
                        name: var.ty().name().to_string(),
                    },
                })
                .collect(),
            outputs: self
                .signature
                .outputs()
                .iter()
                .map(|var| SerializableGraphVariable {
                    identifier: var.identifier().to_string(),
                    ty: GraphValueTypeId {
                        name: var.ty().name().to_string(),
                    },
                })
                .collect(),
        };

        let sg = SerializableGraph {
            nodes,
            inputs,
            outputs,
            signature,
            ident_generator,
        };

        let serializer = GraphSerializer::new(buf);
        sg.serialize(serializer)?;
        Ok(())
    }

    pub fn deserialize<'de>(
        storage: Arc<GraphDynamicInstancesStorage>,
        deserializer: toml::Deserializer<'de>,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let mut rs = (None, Vec::new());
        let errs = &mut rs.1;
        let SerializableGraph {
            nodes,
            inputs,
            outputs,
            signature,
            ident_generator,
        } = match SerializableGraph::deserialize(deserializer) {
            Result::Ok(x) => x,
            Err(e) => {
                errs.push(GraphDeserializeError::DeserializerError(e));
                return rs;
            }
        };

        let sig_inputs = signature
            .inputs
            .iter()
            .try_fold(Vec::new(), |mut acc, var| {
                let type_name = &var.ty.name;
                let value_type_obj = match storage.types.get(type_name) {
                    Some(t) => t,
                    None => {
                        errs.push(GraphDeserializeError::TypeNotFound(type_name.clone()));
                        return Result::<_, GraphDeserializeError>::Err(
                            GraphDeserializeError::TypeNotFound(type_name.clone()),
                        );
                    }
                };

                acc.push(GraphVariable::new_boxed(
                    var.identifier.clone(),
                    dyn_clone::clone_box(&**value_type_obj),
                ));

                Result::<_, GraphDeserializeError>::Ok(acc)
            });
        let Result::Ok(sig_inputs) = sig_inputs else {
            return rs;
        };

        let sig_outputs = signature
            .outputs
            .iter()
            .try_fold(Vec::new(), |mut acc, var| {
                let type_name = &var.ty.name;
                let value_type_obj = match storage.types.get(type_name) {
                    Some(t) => t,
                    None => {
                        errs.push(GraphDeserializeError::TypeNotFound(type_name.clone()));
                        return Result::<_, GraphDeserializeError>::Err(
                            GraphDeserializeError::TypeNotFound(type_name.clone()),
                        );
                    }
                };

                acc.push(GraphVariable::new_boxed(
                    var.identifier.clone(),
                    dyn_clone::clone_box(&**value_type_obj),
                ));

                Result::<_, GraphDeserializeError>::Ok(acc)
            });
        let Result::Ok(sig_outputs) = sig_outputs else {
            return rs;
        };

        let signature = GraphSignature::new_full(signature.name, sig_inputs, sig_outputs);

        let mut graph_nodes = HashMap::with_capacity(nodes.len());
        let mut graph_inputs = HashMap::with_capacity(inputs.len());
        let mut graph_outputs = HashMap::with_capacity(outputs.len());
        let mut ident_generator = GraphVarIdentGenerator::default();

        'node_loop: for node in nodes {
            let Some(node_inst) = storage.nodes.get_cloned(&node.data.name) else {
                errs.push(GraphDeserializeError::NodeNotFound(node.data.name.clone()));
                continue;
            };

            let raw_inputs = node_inst.create_inputs();
            if raw_inputs.len() != node.inputs.len() {
                errs.push(GraphDeserializeError::UnmatchedInputSlotCount {
                    node: node.id,
                    expected: raw_inputs.len(),
                    found: node.inputs.len(),
                });
                continue;
            }
            let mut node_inputs = HashMap::with_capacity(raw_inputs.len());

            for (index, default) in raw_inputs.into_iter().enumerate() {
                let id = node.inputs[index];
                let Some(slot) = inputs.get(&id) else {
                    errs.push(GraphDeserializeError::MissingInputSlot(node.id, id));
                    continue 'node_loop;
                };

                let type_name = default.value.ty().name();
                let value_type_obj = match storage.types.get(type_name) {
                    Some(t) => t,
                    None => {
                        errs.push(GraphDeserializeError::TypeNotFound(type_name.to_string()));
                        continue 'node_loop;
                    }
                };

                let literal_value = match value_type_obj.deserialize_literal(slot.data.clone()) {
                    Result::Ok(val) => val,
                    Err(e) => {
                        errs.push(GraphDeserializeError::LiteralDeserializeError(e));
                        continue 'node_loop;
                    }
                };

                node_inputs.insert(
                    id,
                    GraphInputSlotData {
                        node_id: node.id,
                        data: GraphLiteral::new_boxed(
                            literal_value,
                            dyn_clone::clone_box(&**value_type_obj),
                        ),
                        connected: slot.connected,
                    },
                );
            }

            let raw_outputs = node_inst.create_outputs();
            if raw_outputs.len() != node.outputs.len() {
                errs.push(GraphDeserializeError::UnmatchedOutputSlotCount {
                    node: node.id,
                    expected: raw_outputs.len(),
                    found: node.outputs.len(),
                });
                continue;
            }
            let mut node_outputs = HashMap::with_capacity(raw_outputs.len());

            for (index, default) in raw_outputs.into_iter().enumerate() {
                let id = node.outputs[index];
                let Some(slot) = outputs.get(&id) else {
                    errs.push(GraphDeserializeError::MissingOutputSlot(node.id, id));
                    continue 'node_loop;
                };

                node_outputs.insert(
                    id,
                    GraphOutputSlotData {
                        node_id: node.id,
                        data: GraphVariable::new_boxed(ident_generator.next_output(), default.ty),
                        connected: HashSet::new(),
                    },
                );
            }

            let mut data = StatefulGraphNode::new(node_inst);
            match data.deserialize_state(node.state) {
                Result::Ok(_) => {}
                Err(e) => {
                    errs.push(GraphDeserializeError::NodeStateDeserializeError(e));
                    continue 'node_loop;
                }
            }

            graph_nodes.insert(
                node.id,
                GraphNodeData {
                    position: node.position.into(),
                    data,
                    inputs: node.inputs,
                    outputs: node.outputs,
                },
            );
            graph_inputs.extend(node_inputs);
            graph_outputs.extend(node_outputs);
        }

        for (input_id, input) in &mut graph_inputs {
            if let Some(connected_id) = input.connected {
                match graph_outputs.entry(connected_id) {
                    Entry::Occupied(mut e) => {
                        e.get_mut().connected.insert(*input_id);
                    }
                    Entry::Vacant(_) => {
                        errs.push(GraphDeserializeError::MissingConnectedSlot(
                            input.node_id,
                            *input_id,
                            connected_id,
                        ));
                        input.connected = None;
                    }
                }
            }
        }

        rs.0 = Some(Graph {
            nodes: graph_nodes,
            slots: GraphSlots {
                inputs: graph_inputs,
                outputs: graph_outputs,
            },
            storage,
            signature,
            ident_generator,
            cached_run_order: None,
        });

        return rs;
    }
}

#[derive(Serialize, Deserialize)]
pub struct SerializableGraph {
    pub nodes: Vec<SerializableNodeData>,
    pub inputs: HashMap<Id<GraphInputSlotData>, SerializableInputSlotData>,
    pub outputs: HashMap<Id<GraphOutputSlotData>, SerializableOutputSlotData>,
    pub signature: SerializableGraphSignature,
    pub ident_generator: SerializableGraphIdentGenerator,
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
    pub inputs: Arc<[Id<GraphInputSlotData>]>,
    pub outputs: Arc<[Id<GraphOutputSlotData>]>,
    pub state: toml::Value,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableInputSlotData {
    pub connected: Option<Id<GraphOutputSlotData>>,
    pub data: toml::Value,
}

#[derive(Serialize, Deserialize)]
pub struct SerializableOutputSlotData {}

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

#[derive(Serialize, Deserialize)]
pub struct SerializableGraphSignature {
    pub name: String,
    pub inputs: Vec<SerializableGraphVariable>,
    pub outputs: Vec<SerializableGraphVariable>,
}
