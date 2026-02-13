use iced_core::{Color, Element, color};
use iced_widget::{Column, column, pick_list, space, text_input};
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        GraphDynamicInstancesStorage,
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext,
        },
        slot::{
            ErasedGraphLiteralUpdateMessage, ErasedGraphValueType, GraphDefaultInputSlot,
            GraphDefaultOutputSlot,
        },
        variable::GraphLiteral,
    },
    save::GraphSerializable,
};

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
        state: &Self::State,
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
        // Will be handled by parent SubGraphNode
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
        state: &Self::State,
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
        // Will be handled by parent SubGraphNode
        Ok(Default::default())
    }
}
