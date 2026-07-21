use std::{
    collections::{HashMap, HashSet},
    error::Error,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::Global;

use crate::{
    asset::{Asset, ErasedAsset},
    bundle::ErasedAssetBundle,
    error::{AssetErrorKind, AssetResult},
    store::AssetRegistry,
};

#[derive(Default)]
pub struct AssetRegistryBuilder {
    root: PathBuf,
    bundles: Vec<Arc<dyn ErasedAssetBundle>>,
    serializers: HashMap<&'static str, Box<dyn ErasedAssetSerializer>>,
}

impl Global for AssetRegistryBuilder {}

impl AssetRegistryBuilder {
    pub fn set_root(&mut self, root: PathBuf) {
        self.root = root;
    }

    pub fn add_serializer<L: AssetSerializer + Default>(&mut self) {
        let loader = Box::new(L::default());
        self.serializers.insert(L::file_extension(), loader);
    }

    pub fn add_bundle(&mut self, bundle: Arc<dyn ErasedAssetBundle>) {
        self.bundles.push(bundle);
    }

    pub fn build(self) -> AssetRegistry {
        self.try_build().unwrap()
    }

    pub fn try_build(self) -> AssetResult<AssetRegistry> {
        let mut serializers = AssetSerializerRegistry::default();
        for (ext, loader) in self.serializers {
            serializers.serializers.insert(ext, Arc::from(loader));
        }
        let mut registry = AssetRegistry::new(&self.root, serializers.into())?;
        registry.add_erased_bundles(self.bundles)?;
        let loaded_bundle_ids = registry
            .bundles()
            .map(|bundle| bundle.metadata().bundle_id)
            .collect::<HashSet<_>>();
        registry
            .index_db()
            .remove_unloaded_bundles(&loaded_bundle_ids)?;
        Ok(registry)
    }
}

#[derive(Default)]
pub struct AssetSerializerRegistry {
    serializers: HashMap<&'static str, Arc<dyn ErasedAssetSerializer>>,
}

impl Global for AssetSerializerRegistry {}

impl AssetSerializerRegistry {
    pub fn get(&self, ext: &str) -> Option<Arc<dyn ErasedAssetSerializer>> {
        self.serializers.get(ext).cloned()
    }

    pub fn get_for_path(
        &self,
        path: impl AsRef<Path>,
    ) -> AssetResult<Arc<dyn ErasedAssetSerializer>> {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| AssetErrorKind::MissingExtension(path.to_path_buf()))?;
        Ok(self
            .get(ext)
            .ok_or_else(|| AssetErrorKind::SerializerNotFound(ext.to_string()))?)
    }
}

pub trait AssetSerializer: Send + Sync + 'static {
    type Asset: Asset;
    type Error: Error + Send + Sync + 'static;
    fn file_extension() -> &'static str;
    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error>;
    fn write(&self, asset: &Self::Asset, writer: &mut dyn Write) -> Result<(), Self::Error>;
}

pub trait ErasedAssetSerializer: Send + Sync + 'static {
    fn file_extension(&self) -> &'static str;
    fn asset_type_name(&self) -> &'static str;
    fn read(
        &self,
        reader: &mut dyn Read,
    ) -> Result<Box<dyn ErasedAsset>, Box<dyn Error + Send + Sync + 'static>>;
    fn write(
        &self,
        asset: &dyn ErasedAsset,
        writer: &mut dyn Write,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>>;
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
    ) -> Result<Box<dyn ErasedAsset>, Box<dyn Error + Send + Sync + 'static>> {
        match <Self as AssetSerializer>::read(self, reader) {
            Ok(a) => Ok(Box::new(a)),
            Err(e) => Err(Box::new(e)),
        }
    }

    fn write(
        &self,
        asset: &dyn ErasedAsset,
        writer: &mut dyn Write,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
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
