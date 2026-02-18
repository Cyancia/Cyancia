use std::{
    any::{Any, TypeId},
    collections::{HashMap, hash_map::Entry},
    path::Path,
    sync::Arc,
};

use crate::{
    asset::{Asset, AssetHandle, AssetUrl},
    bundle::{AssetBundle, BundleId, BundleMetadata, CachedAssetBundle},
    error::Result,
    id::AssetId,
    index_db::AssetIndexDb,
    loader::{AssetSerializer, ErasedAssetSerializer},
};

pub struct AssetRegistry {
    bundles: HashMap<BundleId, Arc<CachedAssetBundle>>,
    index_db: Arc<AssetIndexDb>,
}

impl AssetRegistry {
    pub async fn new(root: impl AsRef<Path>) -> Result<Self> {
        // TODO: Scan bundles
        let bundles = HashMap::new();
        let index_db = AssetIndexDb::connect(&format!(
            "sqlite:{}",
            root.as_ref().join("index.sqlite3").display()
        ))
        .await?;

        Ok(Self {
            bundles,
            index_db: Arc::new(index_db),
        })
    }

    pub fn add_bundle<B: AssetBundle>(&mut self, bundle: B) {
        self.bundles.insert(
            BundleId::new(bundle.metadata().filename.clone()),
            Arc::new(bundle),
        );
    }

    pub fn handle<T: Asset>(&self, bundle_id: BundleId, path: &str) -> Option<AssetHandle<T>> {
        let bundle = self.bundles.get(&bundle_id)?;

        Some(AssetHandle::new(
            AssetUrl::new(bundle_id, path.to_string().into()),
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
                    AssetUrl::new(meta.bundle_id.clone(), meta.relative_path.into()),
                    self.bundles.get(&meta.bundle_id)?.clone(),
                    self.index_db.clone(),
                ))
            })
            .collect::<_>();

        Some(handles)
    }
}
