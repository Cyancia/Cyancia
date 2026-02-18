use std::{collections::HashMap, io::Read, sync::Arc};

use crate::{
    asset::{Asset, ErasedAsset},
    store::AssetRegistry,
};

pub struct AssetSerializerRegistry {
    serializers: HashMap<&'static str, Arc<dyn ErasedAssetSerializer>>,
}

impl AssetSerializerRegistry {
    pub fn new() -> Self {
        Self {
            serializers: HashMap::new(),
        }
    }

    pub fn register<L: AssetSerializer + Default>(&mut self) {
        let loader = Arc::new(L::default());
        self.serializers.insert(L::file_extension(), loader.clone());
    }

    pub fn get(&self, ext: &str) -> Option<Arc<dyn ErasedAssetSerializer>> {
        self.serializers.get(ext).cloned()
    }
}

pub trait AssetSerializer: Send + Sync + 'static {
    type Asset: Asset;
    type Error: std::error::Error + Send + Sync + 'static;
    fn file_extension() -> &'static str;
    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error>;
    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error>;
}

pub trait ErasedAssetSerializer: Send + Sync + 'static {
    fn file_extension(&self) -> &'static str;
    fn asset_type_name(&self) -> &'static str;
    fn read(
        &self,
        reader: &mut dyn Read,
    ) -> Result<Box<dyn ErasedAsset>, Box<dyn std::error::Error + Send + Sync + 'static>>;
    fn write(
        &self,
        asset: &dyn ErasedAsset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>>;
}

impl<T: AssetSerializer> ErasedAssetSerializer for T {
    fn file_extension(&self) -> &'static str {
        <Self as AssetSerializer>::file_extension()
    }

    fn asset_type_name(&self) -> &'static str {
        <<Self as AssetSerializer>::Asset>::TYPE_NAME
    }

    fn read(
        &self,
        reader: &mut dyn Read,
    ) -> Result<Box<dyn ErasedAsset>, Box<dyn std::error::Error + Send + Sync + 'static>> {
        match <Self as AssetSerializer>::read(self, reader) {
            Ok(a) => Ok(Box::new(a)),
            Err(e) => Err(Box::new(e)),
        }
    }

    fn write(
        &self,
        asset: &dyn ErasedAsset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let asset = asset
            .as_any()
            .downcast_ref::<<Self as AssetSerializer>::Asset>()
            .ok_or_else(|| {
                format!(
                    "Asset type mismatch for serializer {}",
                    <Self as AssetSerializer>::file_extension()
                )
            })?;
        match <Self as AssetSerializer>::write(self, asset, writer) {
            Ok(()) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    }
}
