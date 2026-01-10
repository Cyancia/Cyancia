use std::{any::TypeId, collections::HashMap, marker::PhantomData, sync::Arc};

use anyhow::anyhow;
use iced_core::{Color, Element, color};
use iced_widget::{column, pick_list};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeCreator,
            GraphNodeUpdateContext, GraphNodeViewContext,
        },
        slot::{
            ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot,
            GraphValueType,
        },
        variable::GraphLiteral,
    },
};

#[derive(Default)]
pub struct ExternalDataStorage {
    contents: RwLock<HashMap<UntypedExternalLiteralId, Arc<GraphLiteral>>>,
    types: RwLock<HashMap<TypeId, Vec<UntypedExternalLiteralId>>>,
}

impl ExternalDataStorage {
    pub fn insert<T: GraphValueType>(&self, id: ExternalLiteralId<T>, value: GraphLiteral) {
        let mut contents = self.contents.write();
        let mut types = self.types.write();
        let id = id.untyped().clone();

        contents.insert(id.clone(), Arc::new(value));
        types.entry(TypeId::of::<T>()).or_default().push(id);
    }

    pub fn get<T: GraphValueType>(&self, id: &ExternalLiteralId<T>) -> Option<Arc<GraphLiteral>> {
        self.contents.read().get(id.as_untyped()).cloned()
    }

    pub fn all_of_type<T: GraphValueType>(&self) -> Vec<ExternalLiteralId<T>> {
        self.types
            .read()
            .get(&TypeId::of::<T>())
            .map(|ids| {
                ids.iter()
                    .map(|id| ExternalLiteralId::new(id.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UntypedExternalLiteralId {
    name: String,
}

impl UntypedExternalLiteralId {
    pub fn typed<T>(self) -> ExternalLiteralId<T> {
        ExternalLiteralId::new(self.name)
    }
}

pub struct ExternalLiteralId<T> {
    id: UntypedExternalLiteralId,
    _marker: PhantomData<T>,
}

impl<T> ExternalLiteralId<T> {
    pub fn new(name: String) -> Self {
        ExternalLiteralId {
            id: UntypedExternalLiteralId { name },
            _marker: PhantomData,
        }
    }

    pub fn untyped(self) -> UntypedExternalLiteralId {
        self.id
    }

    pub fn as_untyped(&self) -> &UntypedExternalLiteralId {
        &self.id
    }
}

impl<T> ToString for ExternalLiteralId<T> {
    fn to_string(&self) -> String {
        self.id.name.clone()
    }
}

impl<T> Serialize for ExternalLiteralId<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.id.serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for ExternalLiteralId<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = UntypedExternalLiteralId::deserialize(deserializer)?;
        Ok(ExternalLiteralId {
            id,
            _marker: PhantomData,
        })
    }
}

impl<T> Clone for ExternalLiteralId<T> {
    fn clone(&self) -> Self {
        ExternalLiteralId {
            id: self.id.clone(),
            _marker: PhantomData,
        }
    }
}

impl<T> PartialEq for ExternalLiteralId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for ExternalLiteralId<T> {}

pub struct ExternalLiteralType<T> {
    storage: Arc<ExternalDataStorage>,
    marker: PhantomData<T>,
}

impl<T> Clone for ExternalLiteralType<T> {
    fn clone(&self) -> Self {
        ExternalLiteralType {
            storage: self.storage.clone(),
            marker: PhantomData,
        }
    }
}

pub struct ExternalNodeCreator<T> {
    storage: Arc<ExternalDataStorage>,
    _marker: PhantomData<T>,
}

impl<T: GraphValueType> ExternalNodeCreator<T> {
    pub fn new(storage: Arc<ExternalDataStorage>) -> Self {
        ExternalNodeCreator {
            storage,
            _marker: PhantomData,
        }
    }
}

impl<T: GraphValueType + Default> GraphNodeCreator for ExternalNodeCreator<T> {
    type NodeType = ExternalNode<T>;

    fn create(&self) -> Self::NodeType {
        ExternalNode {
            storage: self.storage.clone(),
            _marker: PhantomData,
        }
    }
}

pub struct ExternalNode<T> {
    storage: Arc<ExternalDataStorage>,
    _marker: PhantomData<T>,
}

pub enum ExternalNodeMessage<T> {
    IdChanged(ExternalLiteralId<T>),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl<T> Clone for ExternalNodeMessage<T> {
    fn clone(&self) -> Self {
        match self {
            ExternalNodeMessage::IdChanged(id) => ExternalNodeMessage::IdChanged(id.clone()),
            ExternalNodeMessage::LiteralUpdate(m) => ExternalNodeMessage::LiteralUpdate(m.clone()),
        }
    }
}

impl<T: GraphValueType + Default> GraphNode for ExternalNode<T> {
    type State = Option<ExternalLiteralId<T>>;

    type Message = ExternalNodeMessage<T>;

    fn name(&self) -> &'static str {
        "External"
    }

    fn header_color(&self) -> Color {
        color!(0x79c9f2)
    }

    fn create_inputs(&self) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self) -> Vec<GraphDefaultOutputSlot> {
        vec![GraphDefaultOutputSlot::new::<T>("Value")]
    }

    fn generate_code(
        &self,
        state: &Self::State,
        ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let id = state
            .as_ref()
            .ok_or(anyhow!("No external literal selected"))?;
        let literal = self
            .storage
            .get::<T>(id)
            .ok_or(anyhow!("External literal not found"))?;
        let code = literal
            .to_code()
            .ok_or(anyhow!("Cannot convert literal to code"))?;
        let output = ctx.get_output(0)?;
        Ok(format!("let {} = {};\n", output, code))
    }

    fn default_state(&self) -> Self::State {
        None
    }

    fn view_body(
        &self,
        state: &Self::State,
        ctx: GraphNodeViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = column![];

        column = column.push(pick_list(
            self.storage.all_of_type::<T>(),
            state.clone(),
            |id| ExternalNodeMessage::IdChanged(id),
        ));

        column
            .extend(
                ctx.view_all_inputs()
                    .into_iter()
                    .map(|e| e.map(|m| ExternalNodeMessage::LiteralUpdate(m))),
            )
            .into()
    }

    fn update_body(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            ExternalNodeMessage::IdChanged(id) => *state = Some(id),
            ExternalNodeMessage::LiteralUpdate(m) => {
                ctx.update_literal(m);
            }
        }
    }
}
