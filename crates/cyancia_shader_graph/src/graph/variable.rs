use std::{any::Any, collections::HashMap};

use downcast_rs::Downcast;
use dyn_clone::DynClone;

use crate::graph::slot::{ErasedGraphLiteralUpdateMessage, ErasedGraphValueType, GraphValueType};

#[derive(Default, Clone)]
pub struct GraphTypeRegistry {
    types: HashMap<&'static str, Box<dyn ErasedGraphValueType>>,
    casters: HashMap<&'static str, HashMap<&'static str, Box<dyn ErasedGraphVariableCaster>>>,
}

impl GraphTypeRegistry {
    pub fn register_type<T: GraphValueType + Default>(&mut self) {
        let ty = T::default();
        self.types.insert(ty.name(), Box::new(ty));
    }

    pub fn get_type(&self, name: &str) -> Option<&Box<dyn ErasedGraphValueType>> {
        self.types.get(name)
    }

    pub fn register_caster<T: GraphVariableCaster + Default>(&mut self) {
        let from = T::FromType::default();
        let to = T::ToType::default();
        let from_name = <T::FromType as GraphValueType>::name(&from);
        let to_name = <T::ToType as GraphValueType>::name(&to);
        let caster: Box<dyn ErasedGraphVariableCaster> = Box::new(T::default());
        self.casters
            .entry(from_name)
            .or_default()
            .insert(to_name, caster);
    }

    pub fn try_cast(
        &self,
        from_type: &dyn ErasedGraphValueType,
        to_type: &dyn ErasedGraphValueType,
        identifier: &String,
    ) -> Option<String> {
        Some(
            self.casters
                .get(from_type.name())?
                .get(to_type.name())?
                .cast(identifier),
        )
    }

    pub fn can_cast(&self, from: &dyn ErasedGraphValueType, to: &dyn ErasedGraphValueType) -> bool {
        let from_name = from.name();
        let to_name = to.name();
        self.casters
            .get(from_name)
            .and_then(|map| map.get(to_name))
            .is_some()
    }

    pub fn all_types(&self) -> &HashMap<&'static str, Box<dyn ErasedGraphValueType>> {
        &self.types
    }

    pub fn all_casters(
        &self,
    ) -> &HashMap<&'static str, HashMap<&'static str, Box<dyn ErasedGraphVariableCaster>>> {
        &self.casters
    }

    pub fn merge(&mut self, other: GraphTypeRegistry) {
        self.types.extend(other.types);
        for (from, casters) in other.casters {
            self.casters.entry(from).or_default().extend(casters);
        }
    }
}

pub trait GraphVariableCaster: Send + Sync + 'static + Clone {
    type FromType: GraphValueType + Default;
    type ToType: GraphValueType + Default;
    fn cast(&self, variable: &String) -> String;
}

pub trait ErasedGraphVariableCaster: Send + Sync + 'static + DynClone {
    fn cast(&self, variable: &String) -> String;
}

dyn_clone::clone_trait_object!(ErasedGraphVariableCaster);

impl<T: GraphVariableCaster> ErasedGraphVariableCaster for T {
    fn cast(&self, variable: &String) -> String {
        self.cast(variable)
    }
}

pub trait GraphLiteralValue: DynClone + Send + Sync + 'static + Downcast {}

downcast_rs::impl_downcast!(GraphLiteralValue);
dyn_clone::clone_trait_object!(GraphLiteralValue);

impl<T: Send + Sync + 'static + DynClone> GraphLiteralValue for T {}

#[derive(Clone)]
pub struct GraphLiteral {
    value: Box<dyn GraphLiteralValue>,
    ty: Box<dyn ErasedGraphValueType>,
}

impl GraphLiteral {
    pub fn new<T: GraphValueType + Default>(value: T::AssociatedLiteralType) -> Self {
        Self {
            value: Box::new(value),
            ty: Box::new(T::default()) as Box<dyn ErasedGraphValueType>,
        }
    }

    pub fn new_non_default<T: GraphValueType>(value: T::AssociatedLiteralType, ty: T) -> Self {
        Self {
            value: Box::new(value),
            ty: Box::new(ty),
        }
    }

    pub fn new_boxed(value: Box<dyn GraphLiteralValue>, ty: Box<dyn ErasedGraphValueType>) -> Self {
        Self { value, ty }
    }

    pub fn as_ref<T: GraphLiteralValue>(&self) -> &T {
        self.value
            .downcast_ref::<T>()
            .expect("Failed to downcast Literal")
    }

    pub fn as_mut<T: GraphLiteralValue>(&mut self) -> &mut T {
        self.value
            .downcast_mut::<T>()
            .expect("Failed to downcast Literal")
    }

    pub fn try_as_ref<T: GraphLiteralValue>(&self) -> Option<&T> {
        self.value.downcast_ref::<T>()
    }

    pub fn try_as_mut<T: GraphLiteralValue>(&mut self) -> Option<&mut T> {
        self.value.downcast_mut::<T>()
    }

    pub fn ty(&self) -> &Box<dyn ErasedGraphValueType> {
        &self.ty
    }

    pub fn value(&self) -> &Box<dyn GraphLiteralValue> {
        &self.value
    }

    pub fn set<T: GraphLiteralValue>(&mut self, value: T) {
        if let Some(x) = self.value.downcast_mut() {
            *x = value;
        } else {
            log::error!("Setting a Literal with a different type");
        }
    }

    pub fn to_code(&self) -> Option<String> {
        self.ty.literal_to_code(&self.value)
    }

    pub fn update(&mut self, message: ErasedGraphLiteralUpdateMessage) {
        self.ty.update_literal(&mut self.value, message);
    }

    pub fn try_write_into_shader_buffer(&self) -> Option<Vec<u8>> {
        self.ty.try_write_into_shader_buffer(&self.value)
    }
}

#[derive(Clone)]
pub struct GraphVariable {
    identifier: String,
    ty: Box<dyn ErasedGraphValueType>,
}

impl GraphVariable {
    pub fn new<T: GraphValueType + Default>(identifier: String) -> Self {
        Self {
            identifier,
            ty: Box::new(T::default()) as Box<dyn ErasedGraphValueType>,
        }
    }

    pub fn new_boxed(identifier: String, ty: Box<dyn ErasedGraphValueType>) -> Self {
        Self { identifier, ty }
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn ty(&self) -> &Box<dyn ErasedGraphValueType> {
        &self.ty
    }
}
