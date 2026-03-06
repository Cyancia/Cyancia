use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    sync::Arc,
};

use cyancia_assets::{asset::Asset, loader::AssetSerializer};
use serde::{Deserialize, Serialize};
use toml::ser::Buffer;

use crate::{
    GraphSerializer,
    graph::{
        Graph, GraphDynamicInstancesStorage, GraphSignature,
        node::{
            GraphNodeData, GraphNodeId, StatefulGraphNode,
            external::ExternalVariable,
            function::{GraphFunction, GraphFunctionId},
        },
        slot::{
            GraphInputSlotData, GraphInputSlotId, GraphOutputSlotData, GraphOutputSlotId,
            GraphSlots,
        },
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
        node: GraphNodeId,
        expected: usize,
        found: usize,
    },
    #[error("Unmatched output slot count on node {node:?}: expected {expected}, found {found}")]
    UnmatchedOutputSlotCount {
        node: GraphNodeId,
        expected: usize,
        found: usize,
    },
    #[error("Missing input slot on node {0:?}: {1:?}")]
    MissingInputSlot(GraphNodeId, GraphInputSlotId),
    #[error("Missing output slot on node {0:?}: {1:?}")]
    MissingOutputSlot(GraphNodeId, GraphOutputSlotId),
    #[error("Input slot {1:?} on node {0:?} is connected to missing output slot {2:?}")]
    MissingConnectedSlot(GraphNodeId, GraphInputSlotId, GraphOutputSlotId),
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
        let serializer = GraphSerializer::new(buf);
        self.as_serialized()?.serialize(serializer)?;
        Ok(())
    }

    pub fn deserialize<'de>(
        storage: Arc<GraphDynamicInstancesStorage>,
        deserializer: toml::Deserializer<'de>,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let sg = match SerializableGraph::deserialize(deserializer) {
            Ok(x) => x,
            Err(e) => {
                return (None, vec![GraphDeserializeError::DeserializerError(e)]);
            }
        };

        Self::from_serialized(storage, &sg)
    }

    pub fn from_serialized(
        storage: Arc<GraphDynamicInstancesStorage>,
        serialized: &SerializableGraph,
    ) -> (Option<Self>, Vec<GraphDeserializeError>) {
        let SerializableGraph {
            nodes,
            inputs,
            outputs,
        } = serialized;

        let mut rs = (None, Vec::new());
        let errs = &mut rs.1;

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
            match node.deserialize_state(ser_node.state.clone(), &storage) {
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
                    inputs: ser_node.inputs.clone(),
                    outputs: ser_node.outputs.clone(),
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

    pub fn as_serialized(&self) -> Result<SerializableGraph, anyhow::Error> {
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

        Ok(SerializableGraph {
            nodes,
            inputs,
            outputs,
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableGraph {
    pub nodes: Vec<SerializableNodeData>,
    pub inputs: HashMap<GraphInputSlotId, SerializableInputSlotData>,
    pub outputs: HashMap<GraphOutputSlotId, SerializableOutputSlotData>,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct GraphValueTypeId {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub struct GraphNodeTypeId {
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableNodeData {
    pub id: GraphNodeId,
    pub data: GraphNodeTypeId,
    pub position: [f32; 2],
    pub inputs: Arc<[GraphInputSlotId]>,
    pub outputs: Arc<[GraphOutputSlotId]>,
    pub state: toml::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableInputSlotData {
    pub connected: Option<GraphOutputSlotId>,
    pub data: toml::Value,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableOutputSlotData {}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableGraphFunctionSignature {
    pub name: String,
    pub ret_type: GraphValueTypeId,
    pub params: Vec<SerializableGraphVariable>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableGraphVariable {
    pub identifier: String,
    pub ty: GraphValueTypeId,
}

#[derive(Debug, thiserror::Error)]
pub enum SerializableGraphLiteralError {
    #[error("Type not found: {0}")]
    TypeNotFound(String),
    #[error("Failed to deserialize literal data: {0}")]
    LiteralDeserializeError(#[from] toml::de::Error),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableGraphLiteral {
    pub ty: String,
    pub value: toml::Value,
}

impl SerializableGraphLiteral {
    pub fn serialize(literal: &GraphLiteral) -> Result<SerializableGraphLiteral, toml::ser::Error> {
        Ok(SerializableGraphLiteral {
            ty: literal.ty().name().to_string(),
            value: literal.ty().serialize_literal(literal.value())?,
        })
    }

    pub fn deserialize(
        &self,
        storage: &GraphDynamicInstancesStorage,
    ) -> Result<GraphLiteral, SerializableGraphLiteralError> {
        let ty = storage
            .types
            .get(&self.ty)
            .ok_or_else(|| SerializableGraphLiteralError::TypeNotFound(self.ty.clone()))?;

        let literal_value = ty.deserialize_literal(self.value.clone())?;

        Ok(GraphLiteral::new_boxed(literal_value, ty.clone()))
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableExternalVariable {
    pub name: String,
    pub value: SerializableGraphLiteral,
}

impl SerializableExternalVariable {
    pub fn deserialize(
        &self,
        storage: &GraphDynamicInstancesStorage,
    ) -> Result<ExternalVariable, SerializableGraphLiteralError> {
        Ok(ExternalVariable {
            name: self.name.clone(),
            value: self.value.deserialize(storage)?,
        })
    }

    pub fn serialize(
        var: &ExternalVariable,
    ) -> Result<SerializableExternalVariable, toml::ser::Error> {
        Ok(SerializableExternalVariable {
            name: var.name.clone(),
            value: SerializableGraphLiteral {
                ty: var.value.ty().name().to_string(),
                value: var.value.ty().serialize_literal(var.value.value())?,
            },
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SerializableGraphFunction {
    pub id: GraphFunctionId,
    pub name: String,
    pub graph: SerializableGraph,
}

impl SerializableGraphFunction {
    pub fn serialize_func(func: &GraphFunction) -> Result<Self, anyhow::Error> {
        Ok(SerializableGraphFunction {
            id: func.id,
            name: func.name.clone(),
            graph: func.graph.as_serialized()?,
        })
    }

    pub fn deserialize_func(
        &self,
        storage: Arc<GraphDynamicInstancesStorage>,
    ) -> (Option<GraphFunction>, Vec<GraphDeserializeError>) {
        let (maybe_graph, err) = Graph::from_serialized(storage, &self.graph);

        let func = maybe_graph.map(|graph| GraphFunction {
            id: self.id,
            name: self.name.clone(),
            graph,
        });

        (func, err)
    }
}

impl Asset for SerializableGraphFunction {
    const TYPE_NAME: &'static str = "shader_graph_function";
}

#[derive(Default)]
pub struct SerializableGraphFunctionSerializer;

#[derive(Debug, thiserror::Error)]
pub enum SerializableGraphFunctionSerializerError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("Deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
}

impl AssetSerializer for SerializableGraphFunctionSerializer {
    type Asset = SerializableGraphFunction;

    type Error = SerializableGraphFunctionSerializerError;

    fn file_extension() -> &'static str {
        "csf"
    }

    fn read(&self, reader: &mut dyn std::io::Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        Ok(toml::from_str(&buf)?)
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        let serialized = toml::to_string(asset)?;
        writer.write_all(serialized.as_bytes())?;
        Ok(())
    }
}
