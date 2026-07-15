use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use cyancia_utils::wrapper;
use dyn_clone::DynClone;
use gpui::{AnyElement, App, Rgba, Window};
use parse_display::Display;
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::graph::{
    node::GraphNodeId,
    variable::{GraphLiteral, GraphLiteralValue},
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
        let input_node = self.inputs.get(input_id)?;
        let connected_id = input_node.connected.as_ref()?;
        self.outputs.get(connected_id)
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
    fn color(&self, cx: &App) -> Rgba;
    fn name(&self) -> &'static str;
    fn default_literal(&self) -> Self::AssociatedLiteralType;
    fn wgsl_type(&self) -> Option<&'static str>;
    fn try_write_into_shader_buffer(
        &self,
        literal: &Self::AssociatedLiteralType,
    ) -> Option<Vec<u8>>;
    fn literal_to_code(&self, data: &Self::AssociatedLiteralType) -> Option<String>;
    fn render_inline(
        &self,
        literal: &Self::AssociatedLiteralType,
        ctx: GraphInlineLiteralRenderContext<'_>,
    ) -> AnyElement;
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

pub trait ErasedGraphValueType: Send + Sync + 'static + DynClone {
    fn color(&self, cx: &App) -> Rgba;
    fn name(&self) -> &'static str;
    fn default_literal(&self) -> Box<dyn GraphLiteralValue>;
    fn wgsl_type(&self) -> Option<&'static str>;
    fn try_write_into_shader_buffer(&self, literal: &dyn GraphLiteralValue) -> Option<Vec<u8>>;
    fn literal_to_code(&self, data: &dyn GraphLiteralValue) -> Option<String>;
    fn render_inline(
        &self,
        literal: &dyn GraphLiteralValue,
        ctx: GraphInlineLiteralRenderContext<'_>,
    ) -> AnyElement;
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
    fn color(&self, cx: &App) -> Rgba {
        self.color(cx)
    }

    fn name(&self) -> &'static str {
        self.name()
    }

    fn default_literal(&self) -> Box<dyn GraphLiteralValue> {
        Box::new(self.default_literal())
    }

    fn wgsl_type(&self) -> Option<&'static str> {
        self.wgsl_type()
    }

    fn try_write_into_shader_buffer(&self, literal: &dyn GraphLiteralValue) -> Option<Vec<u8>> {
        let literal = literal
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.try_write_into_shader_buffer(literal)
    }

    fn literal_to_code(&self, data: &dyn GraphLiteralValue) -> Option<String> {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.literal_to_code(literal)
    }

    fn render_inline(
        &self,
        literal: &dyn GraphLiteralValue,
        ctx: GraphInlineLiteralRenderContext<'_>,
    ) -> AnyElement {
        let literal = literal
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.render_inline(literal, ctx)
    }

    fn serialize_literal(
        &self,
        data: &dyn GraphLiteralValue,
    ) -> Result<toml::Value, toml::ser::Error> {
        let literal = data
            .downcast_ref::<T::AssociatedLiteralType>()
            .expect("Failed to downcast literal.");
        self.serialize_literal(literal)
    }

    fn deserialize_literal<'a>(
        &self,
        deserializer: toml::Value,
    ) -> Result<Box<dyn GraphLiteralValue>, <toml::Value as Deserializer<'a>>::Error> {
        let literal = self.deserialize_literal(deserializer)?;
        Ok(Box::new(literal))
    }
}

pub struct GraphInlineLiteralRenderContext<'a> {
    pub slot_id: GraphInputSlotId,
    pub window: &'a mut Window,
    pub cx: &'a mut App,
    pub on_update: Rc<dyn Fn(Box<dyn GraphLiteralValue>, &mut App)>,
}
