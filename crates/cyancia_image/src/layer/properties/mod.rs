use std::{any::TypeId, collections::HashMap};

use downcast_rs::Downcast;
use dyn_clone::DynClone;

mod builtin;

pub use builtin::*;

#[derive(Clone)]
pub struct LayerProperties {
    props: HashMap<TypeId, Box<dyn LayerProperty>>,
}

impl std::fmt::Debug for LayerProperties {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LayerProperties")
            .field("props", &self.props.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl LayerProperties {
    pub fn new<T: HasLayerProperties>() -> Self {
        let props = T::new_properties();
        Self { props: props.props }
    }

    pub fn set<T: LayerProperty>(&mut self, value: T) {
        if let Some(prop) = self.props.get_mut(&TypeId::of::<T>()) {
            *prop = Box::new(value);
        }
    }

    pub fn get<T: LayerProperty>(&self) -> Option<&T> {
        let value = self.props.get(&TypeId::of::<T>())?;
        value.downcast_ref()
    }

    pub fn contains<T: LayerProperty>(&self) -> bool {
        self.props.contains_key(&TypeId::of::<T>())
    }
}

pub trait HasLayerProperties {
    fn new_properties() -> LayerPropertiesDeclaration;
}

#[derive(Default)]
pub struct LayerPropertiesDeclaration {
    props: HashMap<TypeId, Box<dyn LayerProperty>>,
}

impl LayerPropertiesDeclaration {
    pub fn create<T: LayerProperty>(&mut self, value: T) {
        self.props.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn create_default<T: LayerProperty + Default>(&mut self) {
        self.props.insert(TypeId::of::<T>(), Box::new(T::default()));
    }
}

pub trait LayerProperty: DynClone + Downcast + 'static {}
downcast_rs::impl_downcast!(LayerProperty);
dyn_clone::clone_trait_object!(LayerProperty);
