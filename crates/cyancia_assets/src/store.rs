use std::{
    any::{Any, TypeId},
    collections::{HashMap, hash_map::Entry},
    fs::metadata,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::DateTime;

use crate::{
    asset::{Asset, AssetHandle, AssetId, AssetMetadata, AssetUrl},
    bundle::{
        AssetBundle, AssetBundleCache, AssetBundleMetadata, BundleId, ErasedAssetBundle,
        modified_bundle_absolute_path, scan_bundle_assets,
    },
    error::{AssetError, AssetResult},
    index_db::{AssetIndexDb, BundleStatus},
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

    pub async fn add_bundle<B: AssetBundle>(&mut self, bundle: B) -> AssetResult<()> {
        let bundle = Arc::new(bundle) as Arc<dyn ErasedAssetBundle>;
        let mut bundle_meta = bundle.metadata().map_err(AssetError::BundleError)?;
        let modified = modified_bundle_absolute_path(&self.root, &bundle_meta.bundle_id);
        if modified.exists() {
            let t = DateTime::from(metadata(modified)?.modified()?);
            if t > bundle_meta.last_modified {
                bundle_meta.last_modified = t;
            }
        }

        let state = self.index_db.upsert_bundle(&bundle_meta).await?;
        let manifest = match state {
            BundleStatus::UpToDate => {
                self.index_db
                    .get_assets_by_bundle(&bundle_meta.bundle_id)
                    .await?
            }
            BundleStatus::Outdated => {
                let manifest = scan_bundle_assets(&self.root, bundle.as_ref(), &self.serializers)?;
                self.index_db
                    .upsert_assets(&bundle_meta.bundle_id, &manifest)
                    .await?;
                manifest
            }
        };

        let cache = AssetBundleCache::new(
            self.root.clone(),
            bundle,
            manifest
                .into_iter()
                .map(|a| (a.asset_id, a.relative_path.into()))
                .collect(),
            self.serializers.clone(),
        )?;

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
        let metadata = self.index_db.get_assets_by_type(T::TYPE_NAME).await.ok()?;

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
