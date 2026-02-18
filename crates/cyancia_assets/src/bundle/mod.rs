use std::{collections::HashMap, error::Error, path::PathBuf, sync::Arc};

use anyhow::Result;
use atomicow::CowArc;
use cyancia_utils::wrapper;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Sqlite, prelude::Type, types::Uuid};

use crate::{
    asset::{Asset, AssetMetadata, ErasedAsset, UntypedAssetHandle},
    id::UntypedAssetId,
    index_db::AssetIndexDb,
    loader::AssetSerializerRegistry,
};

pub mod data_directory;
pub mod standard;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type, Serialize, Deserialize)]
    #[sqlx(transparent)]
    pub BundleId: Uuid
}

pub struct BundleMetadata {
    pub bundle_id: BundleId,
    pub filename: String,
    pub content_hash: String,
    pub readonly: bool,
}

pub struct AssetBundleCache {
    metadata: BundleMetadata,
    cached_asset: RwLock<HashMap<String, Arc<dyn ErasedAsset>>>,
    serializers: Arc<AssetSerializerRegistry>,
    bundle: Arc<dyn ErasedAssetBundle>,
}

impl AssetBundleCache {
    pub fn new(
        metadata: BundleMetadata,
        bundle: Arc<dyn ErasedAssetBundle>,
        serializers: Arc<AssetSerializerRegistry>,
    ) -> Result<Self> {
        Ok(Self {
            metadata,
            cached_asset: RwLock::new(bundle.read(serializers.as_ref())?),
            serializers,
            bundle,
        })
    }

    pub fn metadata(&self) -> &BundleMetadata {
        &self.metadata
    }

    pub fn read(&self, path: &str) -> Result<Arc<dyn ErasedAsset>> {
        self.cached_asset
            .read()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Asset not found in cache: {}", path))
    }

    pub fn update(&self, path: String, asset: Arc<dyn ErasedAsset>) -> Result<()> {
        self.cached_asset.write().insert(path, asset);
        Ok(())
    }

    pub fn write(&self, path: &str) -> Result<()> {
        let cache = self.cached_asset.read();
        let asset = cache
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Asset not found in cache: {}", path))?;
        self.bundle.write(path, asset.as_ref(), &self.serializers)?;

        Ok(())
    }
}

pub trait AssetBundle: Send + Sync + 'static {
    type Error: Error + Sync + Send + 'static;

    fn id(&self) -> BundleId;
    fn hash(&self) -> String;
    fn is_read_only() -> bool;

    fn read(
        &self,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<HashMap<String, Arc<dyn ErasedAsset>>, Self::Error>;
    fn write(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<(), Self::Error>;
}

pub trait ErasedAssetBundle: Send + Sync + 'static {
    fn id(&self) -> BundleId;
    fn hash(&self) -> String;
    fn is_read_only(&self) -> bool;
    fn read(
        &self,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<HashMap<String, Arc<dyn ErasedAsset>>, anyhow::Error>;
    fn write(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<(), anyhow::Error>;
}

impl<T: AssetBundle> ErasedAssetBundle for T {
    fn id(&self) -> BundleId {
        self.id()
    }

    fn hash(&self) -> String {
        self.hash()
    }

    fn is_read_only(&self) -> bool {
        T::is_read_only()
    }

    fn read(
        &self,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<HashMap<String, Arc<dyn ErasedAsset>>, anyhow::Error> {
        self.read(serializers).map_err(Into::into)
    }

    fn write(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<(), anyhow::Error> {
        self.write(path, asset, serializers).map_err(Into::into)
    }
}
