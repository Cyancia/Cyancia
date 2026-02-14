use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};

use iced_core::{Color, Element, color};
use iced_widget::{Column, column, pick_list, space, text_input};
use parking_lot::{RwLock, RwLockReadGuard};
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    editor::slot::{input_slot, output_slot},
    graph::{
        Graph, GraphDynamicInstancesStorage, GraphFunctionsStorage, GraphVarIdentGenerator,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext, GraphNodeUpdateSignatureContext,
        },
        slot::{
            ErasedGraphLiteralUpdateMessage, ErasedGraphValueType, GraphDefaultInputSlot,
            GraphDefaultOutputSlot,
        },
    },
    save::GraphSerializable,
};

pub fn functioning() -> GraphDynamicInstancesStorage {
    let mut storage = GraphDynamicInstancesStorage::default();

    storage.nodes.register::<GraphInputNode>();
    storage.nodes.register::<GraphOutputNode>();

    storage
}

static UNIQUE_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Clone)]
pub struct GraphFunctionNode {
    pub storage: Arc<GraphFunctionsStorage>,
}

impl GraphFunctionNode {
    pub fn new(storage: Arc<GraphFunctionsStorage>) -> Self {
        Self { storage }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphFunctionId {
    pub name: String,
}

impl ToString for GraphFunctionId {
    fn to_string(&self) -> String {
        self.name.clone()
    }
}

#[derive(Serialize, Deserialize)]
pub struct GraphFunctionNodeState {
    pub id: Option<GraphFunctionId>,
}

#[derive(Clone)]
pub enum GraphFunctionNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    FunctionChanged(GraphFunctionId),
}

impl GraphNode for GraphFunctionNode {
    type State = GraphFunctionNodeState;

    type Message = GraphFunctionNodeMessage;

    fn name(&self) -> &'static str {
        "Function"
    }

    fn default_state(&self) -> Self::State {
        GraphFunctionNodeState { id: None }
    }

    fn header_color(&self) -> Color {
        color!(0xb379f2)
    }

    fn create_inputs(&self, state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        let Some(graph) = state.id.as_ref().and_then(|id| self.storage.get(id)) else {
            return Vec::new();
        };

        let mut graph = graph.write();
        if graph.signature().is_none() {
            graph.update_signature_cache();
        }
        graph
            .signature()
            .unwrap()
            .inputs
            .iter()
            .map(|(slot, var)| {
                GraphDefaultInputSlot::new_boxed_default(dyn_clone::clone_box(&*var.ty()))
            })
            .collect()
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
        let Some(graph) = state.id.as_ref().and_then(|id| self.storage.get(id)) else {
            return Vec::new();
        };

        let mut graph = graph.write();
        if graph.signature().is_none() {
            graph.update_signature_cache();
        }
        graph
            .signature()
            .unwrap()
            .outputs
            .iter()
            .map(|(slot, var)| GraphDefaultOutputSlot::new_boxed(dyn_clone::clone_box(&*var.ty())))
            .collect()
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let column = column![pick_list(
            self.storage.all().keys().cloned().collect::<Vec<_>>(),
            state.id.clone(),
            GraphFunctionNodeMessage::FunctionChanged,
        )];
        let Some(graph) = state.id.as_ref().and_then(|id| self.storage.get(id)) else {
            return column.into();
        };

        let mut graph = graph.write();
        if graph.signature().is_none() {
            graph.update_signature_cache();
        }
        let signature = graph.signature().unwrap();
        let slots = ctx
            .all_inputs()
            .zip(signature.inputs.values())
            .map(|((id, slot), var)| {
                input_slot(*id, var.identifier().to_string(), slot)
                    .map(GraphFunctionNodeMessage::LiteralUpdate)
            })
            .collect::<Vec<_>>();

        column.extend(slots).into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let Some(graph) = state.id.as_ref().and_then(|id| self.storage.get(id)) else {
            return space().into();
        };

        let mut graph = graph.write();
        if graph.signature().is_none() {
            graph.update_signature_cache();
        }
        let signature = graph.signature().unwrap();
        let slots = ctx
            .all_outputs()
            .zip(signature.outputs.values())
            .map(|((id, slot), var)| output_slot(*id, var.identifier().to_string(), slot))
            .collect::<Vec<_>>();
        Column::with_children(slots).into()
    }

    fn update(&self, state: &mut Self::State, message: Self::Message, ctx: GraphNodeUpdateContext) {
        match message {
            GraphFunctionNodeMessage::LiteralUpdate(_) => unreachable!(),
            GraphFunctionNodeMessage::FunctionChanged(id) => {
                state.id = Some(id);
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let Some(id) = state.id.as_ref() else {
            return Ok(Default::default());
        };
        let Some(graph) = self.storage.get(id) else {
            return Ok(Default::default());
        };

        let input_idents = (0..ctx.inputs.len()).try_fold(
            Vec::with_capacity(ctx.inputs.len()),
            |mut acc, i| {
                acc.push(ctx.get_input(i)?);
                Ok::<_, GraphNodeCodeGenError>(acc)
            },
        )?;

        let mut graph = graph.write();
        let (output_idents, code) = graph
            .compile(
                input_idents,
                GraphVarIdentGenerator::new(format!(
                    "{}_{}",
                    id.name,
                    UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
                )),
            )
            .map_err(|e| GraphNodeCodeGenError::Custom(e.into()))?;

        for (slot_id, output_ident) in ctx.outputs.iter().zip(output_idents) {
            ctx.output_slot_idents.insert(*slot_id, output_ident);
        }

        Ok(code)
    }
}

#[derive(Default, Clone)]
pub struct GraphInputNode;

#[derive(Default)]
pub struct GraphInputNodeState {
    pub name: String,
    pub ty: Option<Box<dyn ErasedGraphValueType>>,
}

#[derive(Serialize, Deserialize)]
struct SerializableGraphInputNodeState {
    pub name: String,
    pub ty_name: Option<String>,
}

impl GraphSerializable for GraphInputNodeState {
    fn to_toml(&self) -> Result<toml::Value, toml::ser::Error> {
        toml::Value::try_from(SerializableGraphInputNodeState {
            name: self.name.clone(),
            ty_name: self.ty.as_ref().map(|t| t.name().to_string()),
        })
    }

    fn from_toml(
        value: toml::Value,
        storage: &GraphDynamicInstancesStorage,
    ) -> Result<Self, toml::de::Error> {
        let de = SerializableGraphInputNodeState::deserialize(value)?;
        let ty = de.ty_name.map(|ty| {
            storage.types.get(&ty).ok_or_else(|| {
                <toml::de::Error as serde::de::Error>::custom(format!(
                    "Type '{}' not found in storage",
                    ty
                ))
            })
        });
        let ty = match ty {
            Some(ty) => Some(ty?),
            None => None,
        };

        Ok(GraphInputNodeState {
            name: de.name,
            ty: ty.map(|t| dyn_clone::clone_box(&**t)),
        })
    }
}

#[derive(Clone)]
pub enum GraphInputNodeMessage {
    VarNameChanged(String),
    TypeChanged(&'static str),
}

impl GraphNode for GraphInputNode {
    type State = GraphInputNodeState;

    type Message = GraphInputNodeMessage;

    fn name(&self) -> &'static str {
        "Graph Input"
    }

    fn default_state(&self) -> Self::State {
        GraphInputNodeState::default()
    }

    fn header_color(&self) -> Color {
        color!(0x79f2c1)
    }

    fn create_inputs(&self, state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
        dbg!(state.ty.is_some());
        match &state.ty {
            Some(ty) => vec![
                // Comment that prevents ugly formatting
                GraphDefaultOutputSlot::new_boxed(dyn_clone::clone_box(&**ty)),
            ],
            None => vec![],
        }
    }

    fn update_signature(&self, state: &Self::State, mut ctx: GraphNodeUpdateSignatureContext) {
        ctx.require_output_slot_as_graph_input(0, state.name.clone());
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        column![
            text_input("Variable Name", &state.name)
                .on_input(GraphInputNodeMessage::VarNameChanged),
            pick_list(
                ctx.storage()
                    .types
                    .all()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                state.ty.as_ref().map(|t| t.name()),
                GraphInputNodeMessage::TypeChanged
            )
        ]
        .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(&self, state: &mut Self::State, message: Self::Message, ctx: GraphNodeUpdateContext) {
        match message {
            GraphInputNodeMessage::VarNameChanged(name) => state.name = name,
            GraphInputNodeMessage::TypeChanged(ty_name) => {
                state.ty = ctx
                    .storage()
                    .types
                    .get(ty_name)
                    .map(|t| dyn_clone::clone_box(&**t));
            }
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(Default::default())
    }
}

#[derive(Default, Clone)]
pub struct GraphOutputNode;

#[derive(Default)]
pub struct GraphOutputNodeState {
    pub name: String,
    pub ty: Option<Box<dyn ErasedGraphValueType>>,
}

#[derive(Serialize, Deserialize)]
struct SerializableGraphOutputNodeState {
    pub name: String,
    pub ty_name: Option<String>,
}

impl GraphSerializable for GraphOutputNodeState {
    fn to_toml(&self) -> Result<toml::Value, toml::ser::Error> {
        toml::Value::try_from(SerializableGraphOutputNodeState {
            name: self.name.clone(),
            ty_name: self.ty.as_ref().map(|t| t.name().to_string()),
        })
    }

    fn from_toml(
        value: toml::Value,
        storage: &GraphDynamicInstancesStorage,
    ) -> Result<Self, toml::de::Error> {
        let de = SerializableGraphOutputNodeState::deserialize(value)?;
        let ty = de.ty_name.map(|ty| {
            storage.types.get(&ty).ok_or_else(|| {
                <toml::de::Error as serde::de::Error>::custom(format!(
                    "Type '{}' not found in storage",
                    ty
                ))
            })
        });
        let ty = match ty {
            Some(ty) => Some(ty?),
            None => None,
        };

        Ok(GraphOutputNodeState {
            name: de.name,
            ty: ty.map(|t| dyn_clone::clone_box(&**t)),
        })
    }
}

#[derive(Clone)]
pub enum GraphOutputNodeMessage {
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
    VarNameChanged(String),
    TypeChanged(&'static str),
}

impl GraphNode for GraphOutputNode {
    type State = GraphOutputNodeState;

    type Message = GraphOutputNodeMessage;

    fn name(&self) -> &'static str {
        "Graph Output"
    }

    fn default_state(&self) -> Self::State {
        GraphOutputNodeState::default()
    }

    fn header_color(&self) -> Color {
        color!(0x79f2c1)
    }

    fn create_inputs(&self, state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        match &state.ty {
            Some(ty) => vec![
                // Comment that prevents ugly formatting
                GraphDefaultInputSlot::new_boxed_default(dyn_clone::clone_box(&**ty)),
            ],
            None => vec![],
        }
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
        vec![]
    }

    fn update_signature(&self, state: &Self::State, mut ctx: GraphNodeUpdateSignatureContext) {
        ctx.require_input_slot_as_graph_output(0, state.name.clone());
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        column![
            Element::new(
                text_input("Variable Name", &state.name)
                    .on_input(GraphOutputNodeMessage::VarNameChanged),
            ),
            Element::new(pick_list(
                ctx.storage()
                    .types
                    .all()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>(),
                state.ty.as_ref().map(|t| t.name()),
                GraphOutputNodeMessage::TypeChanged,
            )),
        ]
        .extend(ctx.view_all_inputs(&["Value"], GraphOutputNodeMessage::LiteralUpdate))
        .into()
    }

    fn view_outputs(
        &self,
        _state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(&self, state: &mut Self::State, message: Self::Message, ctx: GraphNodeUpdateContext) {
        match message {
            GraphOutputNodeMessage::VarNameChanged(name) => state.name = name,
            GraphOutputNodeMessage::TypeChanged(ty_name) => {
                state.ty = ctx
                    .storage()
                    .types
                    .get(ty_name)
                    .map(|t| dyn_clone::clone_box(&**t));
            }
            GraphOutputNodeMessage::LiteralUpdate(_) => unreachable!(),
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        Ok(Default::default())
    }
}
