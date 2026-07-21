use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::Utc;
use gpui::Global;

use crate::{
    asset::{Asset, AssetHandle, AssetId, AssetMetadata, ErasedAsset, UntypedAssetId},
    bundle::{
        AssetBundle, AssetBundleCache, BundleId, BundleManifest, BundleSnapshot, ErasedAssetBundle,
        read_asset_tags_file, read_tag_file, scan_bundle_assets,
    },
    error::{AssetError, AssetResult},
    index_db::{AssetFilter, AssetIndexDb, TagFilter, UntypedAssetFilter},
    loader::AssetSerializerRegistry,
    tag::{AssetTags, Tag, TagFile, TagId},
};

pub struct AssetRegistry {
    root: PathBuf,
    bundles: HashMap<BundleId, Arc<AssetBundleCache>>,
    serializers: Arc<AssetSerializerRegistry>,
    index_db: Arc<AssetIndexDb>,
}

impl Global for AssetRegistry {}

impl AssetRegistry {
    pub fn new(
        root: impl AsRef<Path>,
        serializers: Arc<AssetSerializerRegistry>,
    ) -> AssetResult<Self> {
        let root = root.as_ref();
        let bundles = HashMap::new();
        let index_db = AssetIndexDb::connect(root.join("index.sqlite3"))?;

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

    pub fn add_asset<T: Asset>(
        &self,
        bundle_id: BundleId,
        path: impl AsRef<Path>,
        asset: Arc<T>,
    ) -> AssetResult<AssetId<T>> {
        let bundle = self
            .bundles
            .get(&bundle_id)
            .ok_or_else(|| AssetError::BundleNotFound(bundle_id))?;
        let asset_id = bundle.add_asset(&path, asset.clone())?;
        self.index_db.add_asset(&AssetMetadata {
            asset_id,
            ty: asset.type_name().to_string(),
            bundle_id,
            relative_path: path.as_ref().to_path_buf().to_string_lossy().to_string(),
            revision: 0,
            // TODO: This can be different from that in fs. But this field is only used for tags to determine if they are outdated.
            //       Probably fix it?
            last_modified: Utc::now(),
            in_memory: false,
        })?;
        Ok(asset_id.into_typed())
    }

    pub fn add_erased_bundles(
        &mut self,
        bundles: impl IntoIterator<Item = Arc<dyn ErasedAssetBundle>>,
    ) -> AssetResult<()> {
        let snapshots = bundles
            .into_iter()
            .map(|bundle| self.scan_bundle(bundle))
            .collect::<AssetResult<Vec<_>>>()?;
        if snapshots.is_empty() {
            return Ok(());
        }

        self.index_db.sync_bundles(&snapshots)?;

        for snapshot in snapshots {
            self.bundles.insert(
                snapshot.metadata.bundle_id,
                Arc::new(AssetBundleCache::new(
                    self.root.clone(),
                    snapshot,
                    self.serializers.clone(),
                )),
            );
        }
        Ok(())
    }

    fn scan_bundle(&self, bundle: Arc<dyn ErasedAssetBundle>) -> AssetResult<BundleSnapshot> {
        let metadata = bundle.metadata().map_err(AssetError::BundleError)?;
        let manifest = bundle.manifest().map_err(AssetError::BundleError)?;
        let assets =
            scan_bundle_assets(&self.root, metadata.clone(), &manifest, &self.serializers)?;
        let tags = scan_tags(bundle.as_ref(), &manifest)?;
        let asset_tags = scan_asset_tags(
            self.root.as_path(),
            &metadata.bundle_id,
            bundle.as_ref(),
            &manifest,
            &assets,
        )?;

        Ok(BundleSnapshot {
            bundle,
            metadata,
            manifest,
            assets,
            tags,
            asset_tags,
        })
    }

    pub fn add_bundle<B: AssetBundle>(&mut self, bundle: B) -> AssetResult<()> {
        self.add_erased_bundles([Arc::new(bundle) as _])
    }

    pub fn handle<T: Asset>(&self, asset_id: AssetId<T>) -> AssetResult<AssetHandle<T>> {
        let bundle_id = self.index_db.get_asset(&asset_id.into_untyped())?.bundle_id;
        let bundle = self
            .bundles
            .get(&bundle_id)
            .ok_or_else(|| AssetError::BundleNotFound(bundle_id))?;

        Ok(AssetHandle::new(
            asset_id,
            bundle.clone(),
            self.index_db.clone(),
        ))
    }

    pub fn all_handles_of<T: Asset>(&self) -> AssetResult<Vec<AssetHandle<T>>> {
        Ok(
            self.metadata_to_handles(self.index_db.get_assets(UntypedAssetFilter {
                ty: Some(T::TYPE_NAME.to_string()),
                ..Default::default()
            })?),
        )
    }

    pub fn all_handles_of_filtered<T: Asset>(
        &self,
        filter: AssetFilter<T>,
    ) -> AssetResult<Vec<AssetHandle<T>>> {
        Ok(self.metadata_to_handles(self.index_db.get_assets(filter.into_untyped())?))
    }

    pub fn all_tags(&self) -> AssetResult<Vec<Tag>> {
        self.index_db.get_tags(TagFilter::default())
    }

    pub fn all_tags_of<T: Asset>(&self) -> AssetResult<Vec<Tag>> {
        self.index_db.get_tags(TagFilter {
            asset_ty: Some(Some(T::TYPE_NAME.to_string())),
            ..Default::default()
        })
    }

    pub fn all_tags_filtered(&self, filter: TagFilter) -> AssetResult<Vec<Tag>> {
        self.index_db.get_tags(filter)
    }

    pub fn add_tag(
        &self,
        bundle_id: &BundleId,
        path: impl AsRef<Path>,
        tag: Tag,
    ) -> AssetResult<()> {
        let bundle = self
            .bundles
            .get(bundle_id)
            .ok_or_else(|| AssetError::BundleNotFound(*bundle_id))?;
        bundle.add_tag(path, TagFile::from(tag.clone()))?;
        self.index_db.add_tag(tag)
    }

    pub fn remove_tag(&self, tag_id: &TagId) -> AssetResult<()> {
        self.index_db.delete_tag(tag_id)
    }

    pub fn serializers(&self) -> &AssetSerializerRegistry {
        &self.serializers
    }

    fn metadata_to_handles<T: Asset>(&self, metadata: Vec<AssetMetadata>) -> Vec<AssetHandle<T>> {
        metadata
            .into_iter()
            .filter_map(|meta| {
                Some(AssetHandle::new(
                    meta.asset_id.into_typed(),
                    self.bundles.get(&meta.bundle_id)?.clone(),
                    self.index_db.clone(),
                ))
            })
            .collect()
    }
}

fn scan_tags(
    bundle: &dyn ErasedAssetBundle,
    manifest: &BundleManifest,
) -> AssetResult<Vec<TagFile>> {
    manifest
        .tags
        .values()
        .map(|path| read_tag_file(path, bundle))
        .collect()
}

fn scan_asset_tags(
    assets_root: &Path,
    bundle_id: &BundleId,
    bundle: &dyn ErasedAssetBundle,
    manifest: &BundleManifest,
    assets: &[AssetMetadata],
) -> AssetResult<HashMap<UntypedAssetId, AssetTags>> {
    let mut asset_tags = HashMap::new();

    for asset in assets {
        let asset_path = manifest
            .assets
            .get(&asset.asset_id)
            .ok_or_else(|| AssetError::AssetPathNotFound(asset.asset_id))?;
        let tags = read_asset_tags_file(assets_root, asset_path, bundle_id, bundle)?;
        asset_tags.insert(asset.asset_id, tags.unwrap_or_default());
    }

    Ok(asset_tags)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        io::{Error as IoError, Read, Write},
    };

    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

    use super::*;
    use crate::{
        bundle::directory::AssetDirectory,
        loader::{AssetRegistryBuilder, AssetSerializer},
        tag::AssetTags,
    };

    #[derive(Debug, Serialize, Deserialize)]
    struct TestAsset {
        value: u32,
    }

    impl Asset for TestAsset {
        const TYPE_NAME: &'static str = "store_test_asset";
    }

    #[derive(Default)]
    struct TestAssetSerializer;

    impl AssetSerializer for TestAssetSerializer {
        type Asset = TestAsset;
        type Error = IoError;

        fn file_extension() -> &'static str {
            "storetest"
        }

        fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
            let mut value = String::new();
            reader.read_to_string(&mut value)?;
            Ok(TestAsset {
                value: value.trim().parse().map_err(IoError::other)?,
            })
        }

        fn write(&self, asset: &Self::Asset, writer: &mut dyn Write) -> Result<(), Self::Error> {
            write!(writer, "{}", asset.value)
        }
    }

    #[test]
    fn batch_sync_resolves_cross_bundle_tags_independent_of_order() -> AssetResult<()> {
        let root = temp_root("cross-bundle");
        let assets_root = root.join("assets-bundle");
        let tags_root = root.join("tags-bundle");
        std::fs::create_dir_all(&assets_root)?;
        std::fs::create_dir_all(&tags_root)?;

        let tag = TagFile::new(
            "Test tag".to_string(),
            Some(TestAsset::TYPE_NAME.to_string()),
        );
        std::fs::write(assets_root.join("sample.storetest"), "42")?;
        std::fs::write(
            assets_root.join("sample.storetest.tags"),
            toml::to_string(&AssetTags {
                tags: BTreeSet::from([tag.id.clone()]),
            })?,
        )?;
        std::fs::write(tags_root.join("test.ctag"), toml::to_string(&tag)?)?;

        let assets_bundle = AssetDirectory::new(&assets_root);
        let assets_bundle_id = AssetBundle::metadata(&assets_bundle)
            .map_err(|error| AssetError::BundleError(Box::new(error)))?
            .bundle_id;
        let mut builder = registry_builder(&root);
        builder.add_bundle(Arc::new(assets_bundle));
        builder.add_bundle(Arc::new(AssetDirectory::new(&tags_root)));
        let registry = builder.try_build()?;

        let handles = registry.all_handles_of::<TestAsset>()?;
        assert_eq!(handles.len(), 1);
        let handle = &handles[0];
        assert_eq!(handle.read_tags()?, BTreeSet::from([tag.id.clone()]));

        handle.remove_tag(&tag.id)?;
        assert!(handle.read_tags()?.is_empty());
        assert!(
            registry
                .all_handles_of_filtered::<TestAsset>(AssetFilter::new().with_tag(tag.id.clone()))?
                .is_empty()
        );
        assert!(handle.remove_tag(&tag.id).is_err());

        handle.add_tag(&tag.id)?;
        assert_eq!(handle.read_tags()?, BTreeSet::from([tag.id.clone()]));
        assert_eq!(
            registry
                .all_handles_of_filtered::<TestAsset>(AssetFilter::new().with_tag(tag.id.clone()))?
                .len(),
            1
        );
        assert!(handle.add_tag(&tag.id).is_err());

        let added_tag = Tag {
            id: TagId::new(Uuid::new_v4()),
            name: "Added at runtime".to_string(),
            asset_ty: None,
        };
        registry.add_tag(&assets_bundle_id, "runtime/added.ctag", added_tag.clone())?;
        assert!(assets_root.join("runtime/added.ctag").is_file());
        let stored_tag = registry.index_db().get_tag(added_tag.id.clone())?;
        assert_eq!(stored_tag.name, added_tag.name);
        assert_eq!(stored_tag.asset_ty, added_tag.asset_ty);

        drop(handles);
        drop(registry);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn corrupt_sidecar_does_not_advance_bundle_metadata() -> AssetResult<()> {
        let root = temp_root("corrupt-sidecar");
        let bundle_root = root.join("asset-bundle");
        std::fs::create_dir_all(&bundle_root)?;
        std::fs::write(bundle_root.join("sample.storetest"), "9")?;
        let bundle = AssetDirectory::new(&bundle_root);
        let bundle_id = AssetBundle::metadata(&bundle)
            .map_err(|error| AssetError::BundleError(Box::new(error)))?
            .bundle_id;

        let mut builder = registry_builder(&root);
        builder.add_bundle(Arc::new(bundle));
        let registry = builder.try_build()?;
        let previous_metadata = registry.index_db().get_bundle(&bundle_id)?;
        drop(registry);

        std::fs::write(bundle_root.join("sample.storetest.tags"), "tags = [")?;
        let mut builder = registry_builder(&root);
        builder.add_bundle(Arc::new(AssetDirectory::new(&bundle_root)));
        assert!(builder.try_build().is_err());

        let index = AssetIndexDb::connect(root.join("index.sqlite3"))?;
        assert_eq!(
            index.get_bundle(&bundle_id)?.last_modified,
            previous_metadata.last_modified
        );
        drop(index);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn registry_builder(root: &Path) -> AssetRegistryBuilder {
        let mut builder = AssetRegistryBuilder::default();
        builder.set_root(root.to_path_buf());
        builder.add_serializer::<TestAssetSerializer>();
        builder
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("cyancia-{name}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
}
