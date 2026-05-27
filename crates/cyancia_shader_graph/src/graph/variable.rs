use std::{
    any::Any,
    collections::{BTreeMap, HashMap},
};

use downcast_rs::Downcast;
use dyn_clone::DynClone;
use gpui::AnyElement;

use crate::graph::{
    GraphData,
    slot::{ErasedGraphValueType, GraphInlineLiteralRenderContext, GraphValueType},
};

#[derive(Default, Clone)]
pub struct GraphTypeRegistry {
    types: BTreeMap<&'static str, Box<dyn ErasedGraphValueType>>,
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

    pub fn try_wgsl_cast(
        &self,
        from_type: &dyn ErasedGraphValueType,
        to_type: &dyn ErasedGraphValueType,
        identifier: &String,
    ) -> Option<String> {
        Some(
            self.casters
                .get(from_type.name())?
                .get(to_type.name())?
                .wgsl_cast(identifier),
        )
    }

    pub fn try_cast(
        &self,
        from_type: &dyn ErasedGraphValueType,
        to_type: &dyn ErasedGraphValueType,
        value: &Box<dyn GraphLiteralValue>,
    ) -> Option<Box<dyn GraphLiteralValue>> {
        Some(
            self.casters
                .get(from_type.name())?
                .get(to_type.name())?
                .cast(value),
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

    pub fn all_types(&self) -> &BTreeMap<&'static str, Box<dyn ErasedGraphValueType>> {
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
    fn wgsl_cast(&self, variable: &String) -> String;
    fn cast(
        &self,
        value: &<Self::FromType as GraphValueType>::AssociatedLiteralType,
    ) -> <Self::ToType as GraphValueType>::AssociatedLiteralType;
}

pub trait ErasedGraphVariableCaster: Send + Sync + 'static + DynClone {
    fn wgsl_cast(&self, variable: &String) -> String;
    fn cast(&self, value: &Box<dyn GraphLiteralValue>) -> Box<dyn GraphLiteralValue>;
}

dyn_clone::clone_trait_object!(ErasedGraphVariableCaster);

impl<T: GraphVariableCaster> ErasedGraphVariableCaster for T {
    fn wgsl_cast(&self, variable: &String) -> String {
        self.wgsl_cast(variable)
    }

    fn cast(&self, value: &Box<dyn GraphLiteralValue>) -> Box<dyn GraphLiteralValue> {
        let from_value = value
            .downcast_ref::<<T::FromType as GraphValueType>::AssociatedLiteralType>()
            .expect("Failed to downcast value for casting");
        let to_value = self.cast(from_value);
        Box::new(to_value)
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

    pub fn downcast<T: GraphLiteralValue>(self) -> T {
        match self.value.downcast::<T>() {
            Ok(ok) => *ok,
            Err(_) => {
                panic!("Failed to downcast Literal")
            }
        }
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

    pub fn set_boxed(&mut self, value: Box<dyn GraphLiteralValue>) {
        if value.as_ref().type_id() == self.value.as_ref().type_id() {
            self.value = value;
        } else {
            log::error!("Setting a Literal with a different type");
        }
    }

    pub fn to_code(&self) -> Option<String> {
        self.ty.literal_to_code(&self.value)
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
