use std::{
    any::{Any, TypeId},
    collections::{HashMap, hash_map::Entry},
    fs::metadata,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};

use crate::{
    asset::{Asset, AssetHandle, AssetId, AssetMetadata, ErasedAsset},
    bundle::{
        AssetBundle, AssetBundleCache, AssetBundleMetadata, BundleId, ErasedAssetBundle,
        modified_bundle_absolute_path, scan_bundle_assets,
    },
    error::{AssetError, AssetResult},
    index_db::{AssetFilter, AssetIndexDb, ItemStatus, UntypedAssetFilter},
    loader::{AssetSerializer, AssetSerializerRegistry, ErasedAssetSerializer},
    tag::{Tag, TagId},
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
        let root = root.as_ref();
        let bundles = HashMap::new();
        let index_db = AssetIndexDb::connect(root.join("index.sqlite3")).await?;

        Ok(Self {
            root: root.to_path_buf(),
            bundles,
            index_db: Arc::new(index_db),
            serializers,
        })
    }

    pub fn index_db(&self) -> &AssetIndexDb {
        &self.index_db
    }

    pub fn bundles(&self) -> impl Iterator<Item = &Arc<AssetBundleCache>> {
        self.bundles.values()
    }

    pub async fn add_asset<T: Asset>(
        &self,
        bundle_id: BundleId,
        path: impl AsRef<Path>,
        asset: Arc<T>,
    ) -> AssetResult<AssetId> {
        let bundle = self
            .bundles
            .get(&bundle_id)
            .ok_or_else(|| AssetError::BundleNotFound(bundle_id))?;
        let asset_id = bundle.add(&path, asset.clone())?;
        self.index_db
            .add_asset(&AssetMetadata {
                asset_id,
                ty: asset.type_name().to_string(),
                bundle_id,
                relative_path: path.as_ref().to_path_buf().to_string_lossy().to_string(),
                revision: 0,
                // TODO: This can be different from that in fs. But this field is only used for tags to determine if they are outdated.
                //       Probably fix it?
                last_modified: Utc::now(),
                in_memory: false,
            })
            .await?;
        Ok(asset_id)
    }

    pub async fn add_bundle<B: AssetBundle>(&mut self, bundle: B) -> AssetResult<()> {
        let bundle = Arc::new(bundle) as Arc<dyn ErasedAssetBundle>;
        let mut bundle_meta = bundle.metadata().map_err(AssetError::BundleError)?;
        let modified = modified_bundle_absolute_path(&self.root, &bundle_meta.bundle_id);
        if modified.exists() {
            let t = DateTime::from(metadata(modified)?.modified()?);
            if t != bundle_meta.last_modified {
                bundle_meta.last_modified = t;
            }
        }

        let status = self.index_db.upsert_bundle(&bundle_meta).await?;
        let manifest = match status {
            ItemStatus::UpToDate => {
                self.index_db
                    .get_assets(UntypedAssetFilter {
                        bundle: Some(bundle_meta.bundle_id),
                        ..Default::default()
                    })
                    .await?
            }
            ItemStatus::Outdated => {
                let manifest = scan_bundle_assets(&self.root, bundle.as_ref(), &self.serializers)?;
                self.index_db
                    .replace_assets(&bundle_meta.bundle_id, &manifest)
                    .await?;
                manifest
            }
        };

        let tags = manifest
            .iter()
            .filter(|a| a.ty == Tag::TYPE_NAME)
            .cloned()
            .collect::<Vec<_>>();

        let cache = AssetBundleCache::new(
            self.root.clone(),
            bundle,
            manifest
                .into_iter()
                .map(|a| (a.asset_id, a.relative_path.into()))
                .collect(),
            self.serializers.clone(),
        )?;
        let cache = Arc::new(cache);

        match status {
            ItemStatus::UpToDate => {}
            ItemStatus::Outdated => {
                for tag in tags {
                    let handle =
                        AssetHandle::<Tag>::new(tag.asset_id, cache.clone(), self.index_db.clone());
                    let tag_asset = handle.get().await?;
                    self.index_db
                        .upsert_tag(&tag_asset, tag.last_modified)
                        .await?;
                }
            }
        }

        self.bundles.insert(cache.metadata().bundle_id, cache);
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

    pub async fn all_handles_of<T: Asset>(&self) -> AssetResult<Vec<AssetHandle<T>>> {
        Ok(self.metadata_to_handles(
            self.index_db
                .get_assets(UntypedAssetFilter {
                    ty: Some(T::TYPE_NAME.to_string()),
                    ..Default::default()
                })
                .await?,
        ))
    }

    pub async fn all_handles_of_filtered<T: Asset>(
        &self,
        filter: AssetFilter<T>,
    ) -> AssetResult<Vec<AssetHandle<T>>> {
        Ok(self.metadata_to_handles(self.index_db.get_assets(filter.into_untyped()).await?))
    }

    pub fn serializers(&self) -> &AssetSerializerRegistry {
        &self.serializers
    }

    fn metadata_to_handles<T: Asset>(&self, metadata: Vec<AssetMetadata>) -> Vec<AssetHandle<T>> {
        metadata
            .into_iter()
            .filter_map(|meta| {
                Some(AssetHandle::new(
                    meta.asset_id,
                    self.bundles.get(&meta.bundle_id)?.clone(),
                    self.index_db.clone(),
                ))
            })
            .collect()
    }
}
