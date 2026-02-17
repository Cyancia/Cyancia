use std::{
    collections::HashMap,
    sync::Arc,
};

use atomicow::CowArc;
use cyancia_utils::wrapper;
use parking_lot::RwLock;
use sqlx::{Decode, Encode, Sqlite, prelude::Type, types::Uuid};

use crate::{
    asset::{Asset, AssetMetadata, ErasedAsset, UntypedAssetHandle},
    id::UntypedAssetId,
    loader::AssetSerializerRegistry,
};

pub mod data_directory;

wrapper! {
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Type)]
    #[sqlx(transparent)]
    pub BundleId: String
}

pub struct BundleMetadata {
    pub filename: String,
    pub content_hash: String,
}

pub struct CachedAssetBundle {
    metadata: BundleMetadata,
    cached_asset: RwLock<HashMap<String, Arc<dyn ErasedAsset>>>,
    serializers: Arc<AssetSerializerRegistry>,
    bundle: Arc<dyn AssetBundle>,
}

impl CachedAssetBundle {
    pub fn new(
        metadata: BundleMetadata,
        bundle: Arc<dyn AssetBundle>,
        serializers: Arc<AssetSerializerRegistry>,
    ) -> Self {
        Self {
            metadata,
            cached_asset: RwLock::new(HashMap::new()),
            serializers,
            bundle,
        }
    }

    pub fn metadata(&self) -> &BundleMetadata {
        &self.metadata
    }

    pub fn read_by_path(&self, path: &str) -> Option<Arc<dyn ErasedAsset>> {
        if let Some(asset) = self.cached_asset.read().get(path) {
            return Some(asset.clone());
        }

        let asset = self.bundle.read_by_path(path, &self.serializers)?;
        self.cached_asset
            .write()
            .insert(path.to_string(), asset.clone());
        Some(asset)
    }

    pub fn update_by_path(&self, path: String, asset: Arc<dyn ErasedAsset>) {
        self.cached_asset.write().insert(path, asset);
    }

    pub fn write_by_path(&self, path: &str) {
        if let Some(asset) = self.cached_asset.read().get(path) {
            self.bundle
                .write_by_path(path, asset.as_ref(), &self.serializers);
        }
    }

    pub fn write_all(&self) {
        for (path, asset) in self.cached_asset.read().iter() {
            self.bundle
                .write_by_path(path, asset.as_ref(), &self.serializers);
        }
    }
}

pub trait AssetBundle: Send + Sync {
    fn hash(&self) -> String;
    fn all_assets(&self, serializers: &AssetSerializerRegistry) -> Vec<AssetMetadata>;

    fn read_by_path(
        &self,
        path: &str,
        serializers: &AssetSerializerRegistry,
    ) -> Option<Arc<dyn ErasedAsset>>;
    fn write_by_path(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    );
}
