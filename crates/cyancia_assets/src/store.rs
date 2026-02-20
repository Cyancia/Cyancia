use std::{
    any::{Any, TypeId},
    collections::{HashMap, hash_map::Entry},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    asset::{Asset, AssetHandle, AssetId, AssetMetadata, AssetUrl},
    bundle::{AssetBundle, AssetBundleCache, AssetBundleMetadata, BundleId, ErasedAssetBundle},
    error::AssetResult,
    index_db::AssetIndexDb,
    loader::{AssetSerializer, AssetSerializerRegistry, ErasedAssetSerializer},
};

pub struct AssetRegistry {
    root: PathBuf,
    bundles: HashMap<BundleId, Arc<AssetBundleCache>>,
    serializers: Arc<AssetSerializerRegistry>,
    index_db: Arc<AssetIndexDb>,
}

impl AssetRegistry {
    pub async fn new(
        root: impl AsRef<Path>,
        serializers: Arc<AssetSerializerRegistry>,
    ) -> AssetResult<Self> {
        let bundles = HashMap::new();
        let index_db = AssetIndexDb::connect("index.sqlite3").await?;

        Ok(Self {
            root: root.as_ref().to_path_buf(),
            bundles,
            index_db: Arc::new(index_db),
            serializers,
        })
    }

    pub async fn add_bundle<B: AssetBundle>(
        &mut self,
        filename: String,
        bundle: B,
    ) -> AssetResult<()> {
        let (cache, metadata) = AssetBundleCache::new(
            self.root.clone(),
            filename,
            Arc::new(bundle),
            self.serializers.clone(),
        )?;

        let _ = self.index_db.upsert_bundle(cache.metadata()).await;

        for meta in metadata {
            let _ = self.index_db.upsert_asset(&meta).await;
        }

        self.bundles
            .insert(cache.metadata().bundle_id, Arc::new(cache));
        Ok(())
    }

    pub fn handle<T: Asset>(
        &self,
        bundle_id: BundleId,
        asset_id: AssetId,
    ) -> Option<AssetHandle<T>> {
        let bundle = self.bundles.get(&bundle_id)?;

        Some(AssetHandle::new(
            asset_id,
            bundle.clone(),
            self.index_db.clone(),
        ))
    }

    pub async fn all_handles_of<T: Asset>(&self) -> Option<Vec<AssetHandle<T>>> {
        let metadata = self.index_db.all_by_type(T::TYPE_NAME).await.ok()?;

        let handles = metadata
            .into_iter()
            .filter_map(|meta| {
                Some(AssetHandle::new(
                    meta.asset_id,
                    self.bundles.get(&meta.bundle_id)?.clone(),
                    self.index_db.clone(),
                ))
            })
            .collect::<_>();

        Some(handles)
    }

    pub fn serializers(&self) -> &AssetSerializerRegistry {
        &self.serializers
    }
}
