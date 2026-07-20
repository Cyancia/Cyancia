use std::collections::HashMap;

use anyhow::{Result, anyhow};
use downcast_rs::Downcast;
use dyn_clone::DynClone;

mod builtin;

pub use builtin::*;

use crate::layer::Layer;

#[derive(Clone)]
pub struct LayerProperties {
    props: HashMap<&'static str, Box<dyn LayerProperty>>,
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

    pub fn new_decoded(data: EncodedLayerProperties, layer: &dyn Layer) -> Result<Self> {
        let props = layer.decode_properties(data)?;
        Ok(Self { props: props.props })
    }

    pub fn set<T: LayerProperty>(&mut self, value: T) {
        if let Some(prop) = self.props.get_mut(T::ident()) {
            *prop = Box::new(value);
        }
    }

    pub fn get<T: LayerProperty>(&self) -> Option<&T> {
        let value = self.props.get(T::ident())?;
        value.downcast_ref()
    }

    pub fn contains<T: LayerProperty>(&self) -> bool {
        self.props.contains_key(T::ident())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &dyn LayerProperty)> {
        self.props.iter().map(|(k, v)| (*k, v.as_ref()))
    }
}

pub struct EncodedLayerProperties {
    props: HashMap<String, Vec<u8>>,
}

impl EncodedLayerProperties {
    pub fn new(props: &[u8]) -> Result<Self> {
        Ok(Self {
            props: rmp_serde::from_slice(props)?,
        })
    }

    pub fn decode<T: LayerProperty>(
        &mut self,
        decl: &mut LayerPropertiesDeclaration,
    ) -> Result<()> {
        let data = self
            .props
            .remove(T::ident())
            .ok_or_else(|| anyhow!("Missing property: {}", T::ident()))?;
        decl.create(T::decode(&data)?);
        Ok(())
    }
}

pub trait HasLayerProperties {
    fn new_properties() -> LayerPropertiesDeclaration;
    fn decode_properties(data: EncodedLayerProperties) -> Result<LayerPropertiesDeclaration>;
}

pub trait HasLayerPropertiesDyn {
    fn new_properties(&self) -> LayerPropertiesDeclaration;
    fn decode_properties(&self, data: EncodedLayerProperties)
    -> Result<LayerPropertiesDeclaration>;
}

impl<T: HasLayerProperties> HasLayerPropertiesDyn for T {
    fn new_properties(&self) -> LayerPropertiesDeclaration {
        T::new_properties()
    }

    fn decode_properties(
        &self,
        data: EncodedLayerProperties,
    ) -> Result<LayerPropertiesDeclaration> {
        T::decode_properties(data)
    }
}

#[derive(Default)]
pub struct LayerPropertiesDeclaration {
    props: HashMap<&'static str, Box<dyn LayerProperty>>,
}

impl LayerPropertiesDeclaration {
    pub fn create<T: LayerProperty>(&mut self, value: T) {
        self.props.insert(T::ident(), Box::new(value));
    }

    pub fn create_default<T: LayerProperty + Default>(&mut self) {
        self.props.insert(T::ident(), Box::new(T::default()));
    }
}

pub trait LayerProperty: DynClone + Downcast + 'static {
    fn ident() -> &'static str
    where
        Self: Sized;
    fn encode(&self) -> Result<Vec<u8>>;
    fn decode(data: &[u8]) -> Result<Self>
    where
        Self: Sized;
}
downcast_rs::impl_downcast!(LayerProperty);
dyn_clone::clone_trait_object!(LayerProperty);
