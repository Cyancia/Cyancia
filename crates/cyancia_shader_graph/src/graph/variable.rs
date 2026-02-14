use std::{any::Any, collections::HashMap};

use crate::graph::slot::{ErasedGraphLiteralUpdateMessage, ErasedGraphValueType, GraphValueType};

#[derive(Default)]
pub struct GraphValueTypeStorage {
    types: HashMap<&'static str, Box<dyn ErasedGraphValueType>>,
}

impl GraphValueTypeStorage {
    pub fn register<T: GraphValueType + Default>(&mut self) {
        let ty = T::default();
        self.types.insert(ty.name(), Box::new(ty));
    }

    pub fn register_non_default<T: GraphValueType>(&mut self, ty: T) {
        self.types.insert(ty.name(), Box::new(ty));
    }

    pub fn get(&self, name: &str) -> Option<&Box<dyn ErasedGraphValueType>> {
        self.types.get(name)
    }

    pub fn all(&self) -> &HashMap<&'static str, Box<dyn ErasedGraphValueType>> {
        &self.types
    }

    pub fn merge(&mut self, other: Self) {
        for (name, ty) in other.types {
            self.types.insert(name, ty);
        }
    }
}

#[derive(Default)]
pub struct GraphTypeCastersStorage {
    casters: HashMap<&'static str, HashMap<&'static str, Box<dyn ErasedGraphVariableCaster>>>,
}

impl GraphTypeCastersStorage {
    pub fn register<T: GraphVariableCaster + Default>(&mut self) {
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

    pub fn all(
        &self,
    ) -> &HashMap<&'static str, HashMap<&'static str, Box<dyn ErasedGraphVariableCaster>>> {
        &self.casters
    }

    pub fn merge(&mut self, other: Self) {
        for (from_name, to_map) in other.casters {
            let entry = self.casters.entry(from_name).or_default();
            for (to_name, caster) in to_map {
                entry.insert(to_name, caster);
            }
        }
    }
}

pub trait GraphVariableCaster: Send + Sync + 'static {
    type FromType: GraphValueType + Default;
    type ToType: GraphValueType + Default;
    fn cast(&self, variable: &String) -> String;
}

pub trait ErasedGraphVariableCaster: Send + Sync + 'static {
    fn cast(&self, variable: &String) -> String;
}

impl<T: GraphVariableCaster> ErasedGraphVariableCaster for T {
    fn cast(&self, variable: &String) -> String {
        self.cast(variable)
    }
}

pub struct GraphLiteral {
    value: Box<dyn Any + Send + Sync>,
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

    pub(crate) fn new_boxed(
        value: Box<dyn Any + Send + Sync>,
        ty: Box<dyn ErasedGraphValueType>,
    ) -> Self {
        Self { value, ty }
    }

    pub fn as_ref<T: 'static>(&self) -> &T {
        self.value
            .downcast_ref::<T>()
            .expect("Failed to downcast Literal")
    }

    pub fn as_mut<T: 'static>(&mut self) -> &mut T {
        self.value
            .downcast_mut::<T>()
            .expect("Failed to downcast Literal")
    }

    pub fn ty(&self) -> &dyn ErasedGraphValueType {
        self.ty.as_ref()
    }

    pub fn value(&self) -> &Box<dyn Any + Send + Sync> {
        &self.value
    }

    pub fn set<T: 'static>(&mut self, value: T) {
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
}

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

    pub fn ty(&self) -> &dyn ErasedGraphValueType {
        self.ty.as_ref()
    }
}
