use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    sync::Arc,
};

use cyancia_id::Id;
use serde::{Deserialize, Serialize};
use toml::ser::Buffer;

use crate::{
    GraphSerializer,
    graph::{
        Graph, GraphDynamicInstancesStorage, GraphSignature,
        node::{GraphNodeData, StatefulGraphNode},
        slot::{GraphInputSlotData, GraphOutputSlotData, GraphSlots},
        variable::{GraphLiteral, GraphVariable},
    },
};

pub trait GraphSerializable: Sized {
    fn to_toml(&self) -> Result<toml::Value, toml::ser::Error>;
    fn from_toml(
        value: toml::Value,
        storage: &GraphDynamicInstancesStorage,
    ) -> Result<Self, toml::de::Error>;
}

impl<'de, T: Serialize + Deserialize<'de>> GraphSerializable for T {
    fn to_toml(&self) -> Result<toml::Value, toml::ser::Error> {
        toml::Value::try_from(self)
    }

    fn from_toml(
        value: toml::Value,
        storage: &GraphDynamicInstancesStorage,
    ) -> Result<Self, toml::de::Error> {
        Self::deserialize(value)
    }
}

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
            Ok(x) => x,
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

                Result::<_, anyhow::Error>::Ok(acc)
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

        let sg = SerializableGraph {
            nodes,
            inputs,
            outputs,
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
        } = match SerializableGraph::deserialize(deserializer) {
            Ok(x) => x,
            Err(e) => {
                errs.push(GraphDeserializeError::DeserializerError(e));
                return rs;
            }
        };

        let mut graph_nodes = HashMap::with_capacity(nodes.len());
        let mut graph_inputs = HashMap::with_capacity(inputs.len());
        let mut graph_outputs = HashMap::with_capacity(outputs.len());

        'node_loop: for ser_node in nodes {
            let Some(node_inst) = storage.nodes.get_cloned(&ser_node.data.name) else {
                errs.push(GraphDeserializeError::NodeNotFound(
                    ser_node.data.name.clone(),
                ));
                continue;
            };

            let mut node = StatefulGraphNode::new(node_inst);
            match node.deserialize_state(ser_node.state, &storage) {
                Ok(_) => {}
                Err(e) => {
                    errs.push(GraphDeserializeError::NodeStateDeserializeError(e));
                    continue 'node_loop;
                }
            }

            let raw_inputs = node.create_inputs();
            if raw_inputs.len() != ser_node.inputs.len() {
                errs.push(GraphDeserializeError::UnmatchedInputSlotCount {
                    node: ser_node.id,
                    expected: raw_inputs.len(),
                    found: ser_node.inputs.len(),
                });
                continue;
            }
            let mut node_inputs = HashMap::with_capacity(raw_inputs.len());

            for (index, default) in raw_inputs.into_iter().enumerate() {
                let id = ser_node.inputs[index];
                let Some(slot) = inputs.get(&id) else {
                    errs.push(GraphDeserializeError::MissingInputSlot(ser_node.id, id));
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
                    Ok(val) => val,
                    Err(e) => {
                        errs.push(GraphDeserializeError::LiteralDeserializeError(e));
                        continue 'node_loop;
                    }
                };

                node_inputs.insert(
                    id,
                    GraphInputSlotData {
                        node_id: ser_node.id,
                        data: GraphLiteral::new_boxed(
                            literal_value,
                            dyn_clone::clone_box(&**value_type_obj),
                        ),
                        connected: slot.connected,
                    },
                );
            }

            let raw_outputs = node.create_outputs();
            if raw_outputs.len() != ser_node.outputs.len() {
                errs.push(GraphDeserializeError::UnmatchedOutputSlotCount {
                    node: ser_node.id,
                    expected: raw_outputs.len(),
                    found: ser_node.outputs.len(),
                });
                continue;
            }
            let mut node_outputs = HashMap::with_capacity(raw_outputs.len());

            for (index, default) in raw_outputs.into_iter().enumerate() {
                let id = ser_node.outputs[index];
                let Some(_slot) = outputs.get(&id) else {
                    errs.push(GraphDeserializeError::MissingOutputSlot(ser_node.id, id));
                    continue 'node_loop;
                };

                node_outputs.insert(
                    id,
                    GraphOutputSlotData {
                        node_id: ser_node.id,
                        data_ty: default.ty,
                        connected: HashSet::new(),
                    },
                );
            }

            graph_nodes.insert(
                ser_node.id,
                GraphNodeData {
                    position: ser_node.position.into(),
                    data: node,
                    inputs: ser_node.inputs,
                    outputs: ser_node.outputs,
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
            cached_run_order: None,
            cached_signature: None,
        });

        return rs;
    }
}

#[derive(Serialize, Deserialize)]
pub struct SerializableGraph {
    pub nodes: Vec<SerializableNodeData>,
    pub inputs: HashMap<Id<GraphInputSlotData>, SerializableInputSlotData>,
    pub outputs: HashMap<Id<GraphOutputSlotData>, SerializableOutputSlotData>,
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
