use std::collections::{HashMap, HashSet};

use anyhow::Result;
use downcast_rs::Downcast;
use dyn_clone::DynClone;
use iced_core::Color;
use lapiz_utils::wrapper;
use parse_display::Display;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use uuid::Uuid;
use wgpu::QueueWriteBufferView;

use crate::{
    GraphElement,
    graph::{
        node::GraphNodeId,
        variable::{GraphLiteral, GraphLiteralValue},
    },
};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub GraphInputSlotId : Uuid
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
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
        let input = self.inputs.get(input_id)?;
        self.outputs.get(input.connected.as_ref()?)
    }
}

pub struct GraphDefaultInputSlot {
    pub name: String,
    pub ty: Box<dyn ErasedGraphValueType>,
}

impl GraphDefaultInputSlot {
    pub fn new<T: GraphValueType + Default>(name: String) -> Self {
        Self {
            name,
            ty: Box::new(T::default()),
        }
    }

    pub fn new_boxed(name: String, ty: Box<dyn ErasedGraphValueType>) -> Self {
        Self { name, ty }
    }
}

pub struct GraphInputSlotData {
    pub node_id: GraphNodeId,
    pub name: String,
    pub data: GraphLiteral,
    pub connected: Option<GraphOutputSlotId>,
}

pub struct GraphDefaultOutputSlot {
    pub name: String,
    pub ty: Box<dyn ErasedGraphValueType>,
}

impl GraphDefaultOutputSlot {
    pub fn new<T: GraphValueType + Default>(name: String) -> Self {
        Self {
            name,
            ty: Box::new(T::default()),
        }
    }

    pub fn new_non_default<T: GraphValueType>(name: String, ty: T) -> Self {
        Self {
            name,
            ty: Box::new(ty),
        }
    }

    pub fn new_boxed(name: String, ty: Box<dyn ErasedGraphValueType>) -> Self {
        Self { name, ty }
    }
}

pub struct GraphOutputSlotData {
    pub node_id: GraphNodeId,
    pub name: String,
    pub data_ty: Box<dyn ErasedGraphValueType>,
    pub connected: HashSet<GraphInputSlotId>,
}

pub trait GraphValueType: Send + Sync + 'static + DynClone {
    type AssociatedLiteralType: GraphLiteralValue + Serialize + DeserializeOwned;
    type Message: GraphLiteralUpdateMessage;

    fn color(&self, is_dark: bool) -> Color;
    fn name(&self) -> &'static str;
    fn default_literal(&self) -> Self::AssociatedLiteralType;
    fn wgsl_type(&self) -> Option<(&'static str, u64)>;
    fn try_write_into_shader_buffer(
        &self,
        literal: &Self::AssociatedLiteralType,
        writer: &mut QueueWriteBufferView,
    ) -> Result<()>;
    fn view_literal(
        &self,
        data: &Self::AssociatedLiteralType,
    ) -> GraphElement<'static, Self::Message>;
    fn update_literal(&self, data: &mut Self::AssociatedLiteralType, message: Self::Message);
    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String>;

    fn serialize_literal(
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
            id: self.id,
        }
    }
}

pub trait ErasedGraphValueType: Send + Sync + 'static + DynClone {
    fn color(&self, is_dark: bool) -> Color;
    fn name(&self) -> &'static str;
    fn default_literal(&self) -> Box<dyn GraphLiteralValue>;
    fn wgsl_type(&self) -> Option<(&'static str, u64)>;
    fn try_write_into_shader_buffer(
        &self,
        literal: &dyn GraphLiteralValue,
        writer: &mut QueueWriteBufferView,
    ) -> Result<()>;
    fn view_literal(
        &self,
        slot_id: GraphInputSlotId,
        data: &dyn GraphLiteralValue,
    ) -> GraphElement<'static, ErasedGraphLiteralUpdateMessage>;
    fn update_literal(
        &self,
        data: &mut dyn GraphLiteralValue,
        message: ErasedGraphLiteralUpdateMessage,
    );
    fn literal_to_code(&self, data: &dyn GraphLiteralValue) -> Option<String>;
    fn serialize_literal(
        &self,
        data: &dyn GraphLiteralValue,
    ) -> Result<toml::Value, toml::ser::Error>;
    fn deserialize_literal<'a>(
        &self,
        deserializer: toml::Value,
    ) -> Result<Box<dyn GraphLiteralValue>, <toml::Value as Deserializer<'a>>::Error>;
}

dyn_clone::clone_trait_object!(ErasedGraphValueType);

impl<T: GraphValueType> ErasedGraphValueType for T {
    fn color(&self, is_dark: bool) -> Color {
        self.color(is_dark)
    }

    fn name(&self) -> &'static str {
        self.name()
    }

    fn default_literal(&self) -> Box<dyn GraphLiteralValue> {
        Box::new(self.default_literal())
    }

    fn wgsl_type(&self) -> Option<(&'static str, u64)> {
        self.wgsl_type()
    }

    fn try_write_into_shader_buffer(
        &self,
        literal: &dyn GraphLiteralValue,
        writer: &mut QueueWriteBufferView,
    ) -> Result<()> {
        self.try_write_into_shader_buffer(
            literal
                .downcast_ref::<T::AssociatedLiteralType>()
                .expect("failed to downcast graph literal"),
            writer,
        )
    }

    fn view_literal(
        &self,
        slot_id: GraphInputSlotId,
        data: &dyn GraphLiteralValue,
    ) -> GraphElement<'static, ErasedGraphLiteralUpdateMessage> {
        self.view_literal(
            data.downcast_ref::<T::AssociatedLiteralType>()
                .expect("failed to downcast graph literal"),
        )
        .map(move |message| ErasedGraphLiteralUpdateMessage {
            inner: Box::new(message),
            id: slot_id,
        })
    }

    fn update_literal(
        &self,
        data: &mut dyn GraphLiteralValue,
        message: ErasedGraphLiteralUpdateMessage,
    ) {
        let data = data
            .downcast_mut::<T::AssociatedLiteralType>()
            .expect("failed to downcast graph literal");
        let message = match message.inner.downcast::<T::Message>() {
            Ok(message) => message,
            Err(_) => panic!("failed to downcast graph literal message"),
        };
        self.update_literal(data, *message);
    }

    fn literal_to_code(&self, data: &dyn GraphLiteralValue) -> Option<String> {
        self.literal_to_code(
            data.downcast_ref::<T::AssociatedLiteralType>()
                .expect("failed to downcast graph literal"),
        )
    }

    fn serialize_literal(
        &self,
        data: &dyn GraphLiteralValue,
    ) -> Result<toml::Value, toml::ser::Error> {
        self.serialize_literal(
            data.downcast_ref::<T::AssociatedLiteralType>()
                .expect("failed to downcast graph literal"),
        )
    }

    fn deserialize_literal<'a>(
        &self,
        deserializer: toml::Value,
    ) -> Result<Box<dyn GraphLiteralValue>, <toml::Value as Deserializer<'a>>::Error> {
        Ok(Box::new(self.deserialize_literal(deserializer)?))
    }
}
