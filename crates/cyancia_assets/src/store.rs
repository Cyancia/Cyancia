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

    pub fn add_bundle(&mut self, bundle: CachedAssetBundle) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};

    use crate::{
        asset::{AssetMetadata, ErasedAsset},
        loader::AssetSerializerRegistry,
    };

    struct TestAsset {
        value: String,
    }

    impl Asset for TestAsset {
        const TYPE_NAME: &'static str = "test_asset";

        fn hash(&self) -> String {
            self.value.clone()
        }
    }

    struct InMemoryBundle {
        assets: HashMap<String, Arc<dyn ErasedAsset>>,
        write_paths: Arc<Mutex<Vec<String>>>,
    }

    impl AssetBundle for InMemoryBundle {
        fn hash(&self) -> String {
            "bundle-hash".to_string()
        }

        fn all_assets(&self, _serializers: &AssetSerializerRegistry) -> Vec<AssetMetadata> {
            Vec::new()
        }

        fn read_by_path(
            &self,
            path: &str,
            _serializers: &AssetSerializerRegistry,
        ) -> Option<Arc<dyn ErasedAsset>> {
            self.assets.get(path).cloned()
        }

        fn write_by_path(
            &self,
            path: &str,
            _asset: &dyn ErasedAsset,
            _serializers: &AssetSerializerRegistry,
        ) {
            self.write_paths.lock().unwrap().push(path.to_string());
        }
    }

    fn sample_metadata(bundle_id: BundleId, path: &str, hash: &str) -> AssetMetadata {
        AssetMetadata {
            bundle_id,
            asset_type: TestAsset::TYPE_NAME.to_string(),
            relative_path: path.to_string(),
            content_hash: hash.to_string(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn create_cached_bundle(
        bundle_name: &str,
        assets: &[(&str, &str)],
    ) -> (CachedAssetBundle, Arc<Mutex<Vec<String>>>) {
        let mut map: HashMap<String, Arc<dyn ErasedAsset>> = HashMap::new();
        for (path, value) in assets {
            map.insert(
                (*path).to_string(),
                Arc::new(TestAsset {
                    value: (*value).to_string(),
                }),
            );
        }
        let write_paths = Arc::new(Mutex::new(Vec::new()));

        (
            CachedAssetBundle::new(
                BundleMetadata {
                    filename: bundle_name.to_string(),
                    content_hash: "bundle-content-hash".to_string(),
                },
                Arc::new(InMemoryBundle {
                    assets: map,
                    write_paths: write_paths.clone(),
                }),
                Arc::new(AssetSerializerRegistry::new()),
            ),
            write_paths,
        )
    }

    async fn create_registry() -> AssetRegistry {
        let index_db = AssetIndexDb::connect("sqlite::memory:").await.unwrap();
        index_db.initialize_tables().await.unwrap();

        AssetRegistry {
            bundles: HashMap::new(),
            index_db: Arc::new(index_db),
        }
    }

    #[tokio::test]
    async fn test_handle_returns_valid_handle() {
        let mut registry = create_registry().await;
        let bundle_id = BundleId::new("bundle-handle".to_string());

        let (bundle, _write_paths) =
            create_cached_bundle("bundle-handle", &[("hero.asset", "hero")]);
        registry.add_bundle(bundle);

        let handle = registry
            .handle::<TestAsset>(bundle_id.clone(), "hero.asset")
            .expect("expected handle to exist");

        assert_eq!(handle.url().source(), &bundle_id);
        assert_eq!(handle.url().path_str(), "hero.asset");

        let asset = handle.read().expect("expected asset to be readable");
        assert_eq!(asset.value, "hero");
    }

    #[tokio::test]
    async fn test_all_handles_of_returns_matching_handles() {
        let mut registry = create_registry().await;

        let bundle_ok = BundleId::new("bundle-ok".to_string());
        let bundle_missing = BundleId::new("bundle-missing".to_string());

        let (bundle, _write_paths) = create_cached_bundle("bundle-ok", &[("a.asset", "asset-a")]);
        registry.add_bundle(bundle);

        registry
            .index_db
            .upsert_bundle(&bundle_ok, "bundle-hash-ok")
            .await
            .unwrap();
        registry
            .index_db
            .upsert_bundle(&bundle_missing, "bundle-hash-missing")
            .await
            .unwrap();
        registry
            .index_db
            .upsert_asset(sample_metadata(bundle_ok.clone(), "a.asset", "hash-a"))
            .await
            .unwrap();
        registry
            .index_db
            .upsert_asset(sample_metadata(
                bundle_missing.clone(),
                "not-in-registry.asset",
                "hash-missing",
            ))
            .await
            .unwrap();

        let handles = registry
            .all_handles_of::<TestAsset>()
            .await
            .expect("query should succeed");

        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].url().source(), &bundle_ok);
        assert_eq!(handles[0].url().path_str(), "a.asset");

        let asset = handles[0]
            .read()
            .expect("expected mapped asset to be readable");
        assert_eq!(asset.value, "asset-a");
    }

    #[tokio::test]
    async fn test_asset_handle_read_write_update() {
        let mut registry = create_registry().await;
        let bundle_id = BundleId::new("bundle-handle-rwu".to_string());

        let (bundle, write_paths) = create_cached_bundle(
            "bundle-handle-rwu",
            &[("hero.asset", "hero"), ("update.asset", "before")],
        );
        registry.add_bundle(bundle);

        registry
            .index_db
            .upsert_bundle(&bundle_id, "bundle-hash")
            .await
            .unwrap();
        registry
            .index_db
            .upsert_asset(sample_metadata(bundle_id.clone(), "hero.asset", "hero"))
            .await
            .unwrap();
        registry
            .index_db
            .upsert_asset(sample_metadata(bundle_id.clone(), "update.asset", "before"))
            .await
            .unwrap();

        let read_write_handle = registry
            .handle::<TestAsset>(bundle_id.clone(), "hero.asset")
            .expect("expected read/write handle");

        let hero_asset = read_write_handle.read().expect("expected read success");
        assert_eq!(hero_asset.value, "hero");

        read_write_handle.write();
        let paths = write_paths.lock().unwrap();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "hero.asset");
        drop(paths);

        let update_handle = registry
            .handle::<TestAsset>(bundle_id.clone(), "update.asset")
            .expect("expected update handle");
        update_handle
            .update(TestAsset {
                value: "after".to_string(),
            })
            .await;

        let updated = update_handle.read().expect("expected updated asset");
        assert_eq!(updated.value, "after");

        let metadata = update_handle.metadata().await.expect("expected metadata");
        assert_eq!(metadata.content_hash, "after");
    }
}
