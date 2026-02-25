use std::{
    any::Any,
    collections::{HashMap, HashSet},
};

use cyancia_utils::wrapper;
use downcast_rs::Downcast;
use dyn_clone::DynClone;
use iced_core::{Color, Element};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        node::{GraphNodeData, GraphNodeId},
        variable::GraphLiteral,
    },
};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub GraphInputSlotId : Uuid
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub GraphOutputSlotId : Uuid
}

#[derive(Default)]
pub struct GraphSlots {
    pub(crate) inputs: HashMap<GraphInputSlotId, GraphInputSlotData>,
    pub(crate) outputs: HashMap<GraphOutputSlotId, GraphOutputSlotData>,
}

impl GraphSlots {
    pub fn get_input(&self, id: &GraphInputSlotId) -> Option<&GraphInputSlotData> {
        self.inputs.get(id)
    }

    pub fn get_output(&self, id: &GraphOutputSlotId) -> Option<&GraphOutputSlotData> {
        self.outputs.get(id)
    }

    pub fn get_connected(&self, input_id: &GraphInputSlotId) -> Option<&GraphOutputSlotData> {
        let input_node = self.inputs.get(input_id)?;
        let connected_id = input_node.connected.as_ref()?;
        self.outputs.get(connected_id)
    }
}

pub struct GraphDefaultInputSlot {
    pub value: GraphLiteral,
}

impl GraphDefaultInputSlot {
    pub fn new<T: GraphValueType + Default>(value: T::AssociatedLiteralType) -> Self {
        Self {
            value: GraphLiteral::new::<T>(value),
        }
    }

    pub fn new_boxed_default(ty: Box<dyn ErasedGraphValueType>) -> Self {
        Self {
            value: GraphLiteral::new_boxed(ty.default_literal(), ty),
        }
    }

    pub fn new_non_default<T: GraphValueType>(value: T::AssociatedLiteralType, ty: T) -> Self {
        Self {
            value: GraphLiteral::new_non_default::<T>(value, ty),
        }
    }
}

pub struct GraphInputSlotData {
    pub node_id: GraphNodeId,
    pub data: GraphLiteral,
    pub connected: Option<GraphOutputSlotId>,
}

pub struct GraphDefaultOutputSlot {
    pub ty: Box<dyn ErasedGraphValueType>,
}

impl GraphDefaultOutputSlot {
    pub fn new<T: GraphValueType + Default>() -> Self {
        Self {
            ty: Box::new(T::default()),
        }
    }

    pub fn new_non_default<T: GraphValueType>(ty: T) -> Self {
        Self { ty: Box::new(ty) }
    }

    pub fn new_boxed(ty: Box<dyn ErasedGraphValueType>) -> Self {
        Self { ty }
    }
}

pub struct GraphOutputSlotData {
    pub node_id: GraphNodeId,
    pub data_ty: Box<dyn ErasedGraphValueType>,
    pub connected: HashSet<GraphInputSlotId>,
}

pub trait GraphValueType: Send + Sync + 'static + DynClone {
    type AssociatedLiteralType: Send + Sync + 'static + Serialize + DeserializeOwned;
    type Message: GraphLiteralUpdateMessage;
    fn color(&self) -> Color;
    fn name(&self) -> &'static str;
    fn default_literal(&self) -> Self::AssociatedLiteralType;
    fn wgsl_type(&self) -> Option<&'static str>;
    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer>;
    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message);
    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String>;
    fn serialize_literal<'a>(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> Result<toml::Value, toml::ser::Error> {
        toml::Value::try_from(data)
    }
    fn deserialize_literal<'a>(
        &self,
        deserializer: toml::Value,
    ) -> Result<Self::AssociatedLiteralType, <toml::Value as Deserializer<'a>>::Error> {
        Self::AssociatedLiteralType::deserialize(deserializer)
    }
}

pub trait GraphLiteralUpdateMessage: DynClone + Send + Sync + 'static + Downcast {}

impl<T: DynClone + Send + Sync + 'static> GraphLiteralUpdateMessage for T {}
downcast_rs::impl_downcast!(GraphLiteralUpdateMessage);

pub struct ErasedGraphLiteralUpdateMessage {
    pub inner: Box<dyn GraphLiteralUpdateMessage>,
    pub id: GraphInputSlotId,
}

impl std::fmt::Debug for ErasedGraphLiteralUpdateMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErasedGraphLiteralUpdateMessage")
            .field("id", &self.id)
            .finish()
    }
}

impl Clone for ErasedGraphLiteralUpdateMessage {
    fn clone(&self) -> Self {
        Self {
            inner: dyn_clone::clone_box(&*self.inner),
            id: self.id.clone(),
        }
    }
}

pub trait ErasedGraphValueType: Send + Sync + 'static + DynClone {
    fn color(&self) -> Color;
    fn name(&self) -> &'static str;
    fn default_literal(&self) -> Box<dyn Any + Send + Sync>;
    fn wgsl_type(&self) -> Option<&'static str>;
    fn view_literal(
        &self,
        slot_id: GraphInputSlotId,
        data: &Box<dyn Any + Send + Sync>,
    ) -> Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer>;
    fn update_literal(
        &self,
        data: &mut Box<dyn Any + Send + Sync>,
        message: ErasedGraphLiteralUpdateMessage,
    );
    fn literal_to_code(&self, data: &Box<dyn Any + Send + Sync>) -> Option<String>;
    fn serialize_literal<'a>(
        &self,
        data: &Box<dyn Any + Send + Sync>,
    ) -> Result<toml::Value, toml::ser::Error>;
    fn deserialize_literal<'a>(
        &self,
        deserializer: toml::Value,
    ) -> Result<Box<dyn Any + Send + Sync>, <toml::Value as Deserializer<'a>>::Error>;
}

impl<T: GraphValueType> ErasedGraphValueType for T {
    fn color(&self) -> Color {
        self.color()
    }

    fn name(&self) -> &'static str {
        self.name()
    }

    fn default_literal(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(self.default_literal())
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        self.wgsl_type()
    }

    fn view_literal(
        &self,
        slot_id: GraphInputSlotId,
        data: &Box<dyn Any + Send + Sync>,
    ) -> Element<'static, ErasedGraphLiteralUpdateMessage, GraphTheme, GraphRenderer> {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.view_literal(literal)
            .map(move |msg| ErasedGraphLiteralUpdateMessage {
                inner: Box::new(msg),
                id: slot_id,
            })
    }

    fn update_literal(
        &self,
        data: &mut Box<dyn Any + Send + Sync>,
        message: ErasedGraphLiteralUpdateMessage,
    ) {
        let literal = data
            .downcast_mut::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        let msg = match message.inner.downcast::<T::Message>() {
            Ok(m) => m,
            Err(_) => {
                unreachable!("Failed to downcast literal update message.");
            }
        };
        self.update_literal(literal, *msg);
    }

    fn literal_to_code(&self, data: &Box<dyn Any + Send + Sync>) -> Option<String> {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.literal_to_code(literal)
    }

    fn serialize_literal<'a>(
        &self,
        data: &Box<dyn Any + Send + Sync>,
    ) -> Result<toml::Value, toml::ser::Error> {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.serialize_literal(literal)
    }

    fn deserialize_literal<'a>(
        &self,
        deserializer: toml::Value,
    ) -> Result<Box<dyn Any + Send + Sync>, <toml::Value as Deserializer<'a>>::Error> {
        let literal = self.deserialize_literal(deserializer)?;
        Ok(Box::new(literal))
    }
}
