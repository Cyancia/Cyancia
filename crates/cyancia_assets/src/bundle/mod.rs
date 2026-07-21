use std::{
    collections::HashMap,
    error::Error,
    fs::{File, create_dir_all, metadata},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use cyancia_utils::wrapper;
use parking_lot::RwLock;
use parse_display::Display;
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    asset::{AssetMetadata, ErasedAsset, UntypedAssetId},
    error::{AssetError, AssetResult},
    loader::{AssetSerializerRegistry, ErasedAssetSerializer},
    tag::{ASSET_TAGS_EXT, AssetTags},
};

pub mod directory;
pub mod standard;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize)]
    #[display("{0}")]
    pub BundleId: Uuid
}

impl rusqlite::types::FromSql for BundleId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(Uuid::column_result(value)?))
    }
}

impl rusqlite::types::ToSql for BundleId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[derive(Serialize, Deserialize)]
pub struct AssetBundleMetadata {
    pub bundle_id: BundleId,
    pub name: String,
    pub last_modified: DateTime<Utc>,
}

pub struct AssetBundleCache {
    assets_root: PathBuf,
    metadata: AssetBundleMetadata,
    bundle: Arc<dyn ErasedAssetBundle>,

    id_to_original_path: RwLock<HashMap<UntypedAssetId, PathBuf>>,
    id_to_path: RwLock<HashMap<UntypedAssetId, PathBuf>>,
    assets: RwLock<HashMap<UntypedAssetId, Arc<dyn ErasedAsset>>>,

    serializers: Arc<AssetSerializerRegistry>,
}

impl AssetBundleCache {
    pub fn new(
        assets_root: impl AsRef<Path>,
        bundle: Arc<dyn ErasedAssetBundle>,
        manifest: HashMap<UntypedAssetId, PathBuf>,
        serializers: Arc<AssetSerializerRegistry>,
    ) -> AssetResult<Self> {
        Ok(Self {
            assets_root: assets_root.as_ref().to_path_buf(),
            metadata: bundle.metadata().map_err(AssetError::BundleError)?,

            id_to_original_path: bundle.manifest().map_err(AssetError::BundleError)?.into(),
            id_to_path: manifest.into(),
            bundle,
            assets: Default::default(),

            serializers,
        })
    }

    pub fn get_cached(&self, id: &UntypedAssetId) -> AssetResult<Arc<dyn ErasedAsset>> {
        let assets = self.assets.read();
        Ok(assets
            .get(id)
            .cloned()
            .ok_or_else(|| AssetError::AssetNotFound(*id))?)
    }

    pub fn update(&self, id: UntypedAssetId, asset: Arc<dyn ErasedAsset>) -> AssetResult<()> {
        self.assets.write().insert(id, asset);
        Ok(())
    }

    pub fn write(&self, id: &UntypedAssetId, revision: u32) -> AssetResult<PathBuf> {
        let assets = self.assets.read();
        let asset = assets
            .get(id)
            .ok_or_else(|| AssetError::AssetNotFound(*id))?;
        let id_to_original_path = self.id_to_original_path.read();
        let path = id_to_original_path
            .get(id)
            .ok_or_else(|| AssetError::AssetPathNotFound(*id))?;
        let serializer = self.serializers.get_for_path(path)?;

        if !self.bundle.is_readonly() && revision == 0 {
            self.bundle
                .add(path, asset.as_ref(), serializer.as_ref())
                .map_err(AssetError::BundleError)?;
            Ok(path.clone())
        } else {
            write_modified_asset(
                &self.assets_root,
                path,
                &self.metadata.bundle_id,
                asset.as_ref(),
                revision,
                serializer.as_ref(),
            )
        }
    }

    pub fn is_readonly(&self) -> bool {
        self.bundle.is_readonly()
    }

    pub fn add(
        &self,
        path: impl AsRef<Path>,
        asset: Arc<dyn ErasedAsset>,
    ) -> AssetResult<UntypedAssetId> {
        let path = path.as_ref().clean();
        let serializer = self.serializers.get_for_path(&path)?;
        let id = self
            .bundle
            .add(&path, asset.as_ref(), serializer.as_ref())
            .map_err(AssetError::BundleError)?;

        self.id_to_path.write().insert(id, path.clone());
        self.id_to_original_path.write().insert(id, path.clone());
        self.assets.write().insert(id, asset);

        Ok(id)
    }

    pub fn metadata(&self) -> &AssetBundleMetadata {
        &self.metadata
    }

    pub fn read_asset_tags(&self, id: &UntypedAssetId) -> AssetResult<AssetTags> {
        let id_to_original_path = self.id_to_original_path.read();
        let path = id_to_original_path
            .get(id)
            .ok_or_else(|| AssetError::AssetPathNotFound(*id))?;

        if self.bundle.is_readonly() {
            let modified_path =
                modified_bundle_absolute_path(&self.assets_root, &self.metadata.bundle_id)
                    .join(path.with_added_extension(ASSET_TAGS_EXT));
            if modified_path.exists() {
                return toml::from_str(&std::fs::read_to_string(modified_path)?)
                    .map_err(Into::into);
            }
        }

        Ok(self
            .bundle
            .read_asset_tags(path)
            .map_err(AssetError::BundleError)?
            .unwrap_or_default())
    }

    pub fn write_asset_tags(&self, id: &UntypedAssetId, tags: &AssetTags) -> AssetResult<()> {
        let id_to_original_path = self.id_to_original_path.read();
        let path = id_to_original_path
            .get(id)
            .ok_or_else(|| AssetError::AssetPathNotFound(*id))?;

        if self.bundle.is_readonly() {
            let modified_path =
                modified_bundle_absolute_path(&self.assets_root, &self.metadata.bundle_id)
                    .join(path.with_added_extension(ASSET_TAGS_EXT));
            if let Some(parent) = modified_path.parent() {
                create_dir_all(parent)?;
            }
            File::create(modified_path)?.write_all(toml::to_string(tags)?.as_bytes())?;
        } else {
            self.bundle
                .write_asset_tags(path, tags)
                .map_err(AssetError::BundleError)?;
        }

        Ok(())
    }

    pub fn read(&self, id: UntypedAssetId, revision: u32) -> AssetResult<Arc<dyn ErasedAsset>> {
        let id_to_path = self.id_to_path.read();
        let path = id_to_path
            .get(&id)
            .ok_or_else(|| AssetError::AssetPathNotFound(id))?;
        let serializer = self.serializers.get_for_path(path)?;

        let asset = if revision == 0 {
            self.bundle
                .read(path, serializer.as_ref())
                .map_err(AssetError::BundleError)?
        } else {
            read_modified_asset(
                &self.assets_root,
                path,
                &self.metadata.bundle_id,
                serializer.as_ref(),
            )?
        };

        self.assets.write().insert(id, asset.clone());
        Ok(asset)
    }

    pub fn absolute_modified_path(&self, path: impl AsRef<Path>) -> PathBuf {
        modified_bundle_absolute_path(&self.assets_root, &self.metadata.bundle_id).join(path)
    }
}

pub fn scan_bundle_assets(
    assets_root: impl AsRef<Path>,
    bundle: &dyn ErasedAssetBundle,
    serializers: &AssetSerializerRegistry,
) -> AssetResult<Vec<AssetMetadata>> {
    let bundle_meta = bundle.metadata().map_err(AssetError::BundleError)?;
    let manifest = bundle.manifest().map_err(AssetError::BundleError)?;
    let mut assets = Vec::with_capacity(manifest.len());

    for (id, path) in &manifest {
        let Ok(serializer) = serializers.get_for_path(path) else {
            log::warn!(
                "Skipped loading asset at {} because of unknown extension.",
                path.display(),
            );
            continue;
        };
        assets.push(AssetMetadata {
            asset_id: *id,
            ty: serializer.asset_type_name().to_string(),
            bundle_id: bundle_meta.bundle_id,
            relative_path: path.to_string_lossy().to_string(),
            revision: 0,
            // TODO: This isn't accurate, but... probably good enough for now?
            last_modified: bundle_meta.last_modified,
            in_memory: false,
        });
    }

    let modified = scan_modified_assets(
        assets_root.as_ref(),
        &bundle_meta.bundle_id,
        &manifest,
        serializers,
    )?;
    assets.extend(modified);

    Ok(assets)
}

fn scan_modified_assets(
    assets_root: &Path,
    bundle_id: &BundleId,
    bundle_manifest: &HashMap<UntypedAssetId, PathBuf>,
    serializers: &AssetSerializerRegistry,
) -> AssetResult<Vec<AssetMetadata>> {
    let mut assets = Vec::new();
    let modified_bundle_path = modified_bundle_absolute_path(assets_root, bundle_id);
    if !modified_bundle_path.exists() {
        return Ok(assets);
    }

    let reversed_manifest = bundle_manifest
        .iter()
        .map(|(id, path)| (path.clone(), *id))
        .collect::<HashMap<_, _>>();

    scan_modified_assets_dfs(
        &modified_bundle_path,
        bundle_id,
        &modified_bundle_path,
        &mut assets,
        serializers,
        &reversed_manifest,
    )?;

    Ok(assets)
}

fn scan_modified_assets_dfs(
    modified_bundle_path: &Path,
    bundle_id: &BundleId,
    current_path: &Path,
    assets: &mut Vec<AssetMetadata>,
    serializers: &AssetSerializerRegistry,
    path_to_id: &HashMap<PathBuf, UntypedAssetId>,
) -> AssetResult<()> {
    for entry in current_path.read_dir()? {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_dir() {
            let _ = scan_modified_assets_dfs(
                modified_bundle_path,
                bundle_id,
                &path,
                assets,
                serializers,
                path_to_id,
            );
        } else if path.is_file() {
            let Some(revision) = parse_revision_from_path(&path) else {
                log::warn!(
                    "Skipped loading modified asset at {} because it does not have a revision.",
                    path.display(),
                );
                continue;
            };

            let Ok(serializer) = serializers.get_for_path(&path) else {
                log::warn!(
                    "Skipped loading modified asset at {} because of unknown extension.",
                    path.display(),
                );
                continue;
            };
            let modified_path = path
                .strip_prefix(modified_bundle_path)
                .unwrap()
                .to_string_lossy()
                .to_string();

            let Some(original_path) = restore_original_path(Path::new(&modified_path)) else {
                continue;
            };
            let Some(asset_id) = path_to_id.get(&original_path) else {
                continue;
            };

            assets.push(AssetMetadata {
                asset_id: *asset_id,
                ty: serializer.asset_type_name().to_string(),
                bundle_id: *bundle_id,
                relative_path: modified_path,
                revision,
                last_modified: metadata(&path)?.modified()?.into(),
                in_memory: false,
            });
        }
    }

    Ok(())
}

fn parse_revision_from_path(path: &Path) -> Option<u32> {
    let filename = path.file_stem()?.to_str()?;
    let parts = filename.split(".rev").collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    parts[1].split('.').next()?.parse().ok()
}

fn restore_original_path(modified_path: &Path) -> Option<PathBuf> {
    let filename = modified_path.file_stem()?.to_str()?;
    let parts = filename.split(".rev").collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    let original_filename = format!(
        "{}.{}",
        parts[0],
        modified_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
    );
    Some(modified_path.with_file_name(original_filename))
}

fn read_modified_asset(
    assets_root: &Path,
    asset_relative_path: &Path,
    bundle_id: &BundleId,
    serializer: &dyn ErasedAssetSerializer,
) -> AssetResult<Arc<dyn ErasedAsset>> {
    let path = modified_bundle_absolute_path(assets_root, bundle_id).join(asset_relative_path);
    let mut file = File::open(path)?;
    Ok(serializer
        .read(&mut file)
        .map_err(AssetError::SerializerError)
        .map(Into::into)?)
}

fn write_modified_asset(
    assets_root: &Path,
    original_relative_path: &Path,
    bundle_id: &BundleId,
    asset: &dyn ErasedAsset,
    revision: u32,
    serializer: &dyn ErasedAssetSerializer,
) -> AssetResult<PathBuf> {
    let new_relative_path = modified_asset_relative_path(original_relative_path, revision);
    let modified_bundle_path = modified_bundle_absolute_path(assets_root, bundle_id);
    let modified_asset_path = modified_bundle_path.join(&new_relative_path);

    if let Some(dir) = &modified_asset_path.parent() {
        create_dir_all(dir)?;
    }
    let mut file = File::create(modified_asset_path)?;
    serializer
        .write(asset, &mut file)
        .map_err(AssetError::SerializerError)?;

    Ok(new_relative_path)
}

fn modified_asset_relative_path(asset_relative_path: &Path, revision: u32) -> PathBuf {
    let ext = asset_relative_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let file_stem = asset_relative_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    asset_relative_path.with_file_name(format!("{}.rev{}.{}", file_stem, revision, ext))
}

pub fn modified_bundle_absolute_path(
    assets_root: impl AsRef<Path>,
    bundle_id: &BundleId,
) -> PathBuf {
    assets_root
        .as_ref()
        .join(format!("{}.modified", bundle_id.0))
}

pub trait AssetBundle: Send + Sync + 'static {
    const READONLY: bool;
    type Error: Error + Sync + Send + 'static;

    fn metadata(&self) -> Result<AssetBundleMetadata, Self::Error>;
    fn manifest(&self) -> Result<HashMap<UntypedAssetId, PathBuf>, Self::Error>;
    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Self::Error>;
    fn add(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, Self::Error>;
    fn read_asset_tags(&self, path: &Path) -> Result<Option<AssetTags>, Self::Error>;
    fn write_asset_tags(&self, path: &Path, tags: &AssetTags) -> Result<(), Self::Error>;
}

pub trait ErasedAssetBundle: Send + Sync + 'static {
    fn is_readonly(&self) -> bool;
    fn metadata(&self) -> Result<AssetBundleMetadata, Box<dyn Error + Send + Sync + 'static>>;
    fn manifest(
        &self,
    ) -> Result<HashMap<UntypedAssetId, PathBuf>, Box<dyn Error + Send + Sync + 'static>>;
    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Box<dyn Error + Send + Sync + 'static>>;
    fn add(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, Box<dyn Error + Send + Sync + 'static>>;
    fn read_asset_tags(
        &self,
        path: &Path,
    ) -> Result<Option<AssetTags>, Box<dyn Error + Send + Sync + 'static>>;
    fn write_asset_tags(
        &self,
        path: &Path,
        tags: &AssetTags,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>>;
}

impl<T: AssetBundle> ErasedAssetBundle for T {
    fn is_readonly(&self) -> bool {
        T::READONLY
    }

    fn metadata(&self) -> Result<AssetBundleMetadata, Box<dyn Error + Send + Sync + 'static>> {
        self.metadata().map_err(Into::into)
    }

    fn manifest(
        &self,
    ) -> Result<HashMap<UntypedAssetId, PathBuf>, Box<dyn Error + Send + Sync + 'static>> {
        self.manifest().map_err(Into::into)
    }

    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Box<dyn Error + Send + Sync + 'static>> {
        self.read(path, serializer).map_err(Into::into)
    }

    fn add(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, Box<dyn Error + Send + Sync + 'static>> {
        self.add(path, asset, serializer).map_err(Into::into)
    }

    fn read_asset_tags(
        &self,
        path: &Path,
    ) -> Result<Option<AssetTags>, Box<dyn Error + Send + Sync + 'static>> {
        self.read_asset_tags(path).map_err(Into::into)
    }

    fn write_asset_tags(
        &self,
        path: &Path,
        tags: &AssetTags,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        self.write_asset_tags(path, tags).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::TagId;

    struct ReadonlyBundle {
        id: BundleId,
        asset_id: UntypedAssetId,
        asset_path: PathBuf,
        tags: AssetTags,
    }

    impl AssetBundle for ReadonlyBundle {
        const READONLY: bool = true;
        type Error = std::io::Error;

        fn metadata(&self) -> Result<AssetBundleMetadata, Self::Error> {
            Ok(AssetBundleMetadata {
                bundle_id: self.id,
                name: "readonly".to_string(),
                last_modified: Utc::now(),
            })
        }

        fn manifest(&self) -> Result<HashMap<UntypedAssetId, PathBuf>, Self::Error> {
            Ok(HashMap::from([(self.asset_id, self.asset_path.clone())]))
        }

        fn read(
            &self,
            _: &Path,
            _: &dyn ErasedAssetSerializer,
        ) -> Result<Arc<dyn ErasedAsset>, Self::Error> {
            Err(std::io::Error::other("not used by this test"))
        }

        fn add(
            &self,
            _: &Path,
            _: &dyn ErasedAsset,
            _: &dyn ErasedAssetSerializer,
        ) -> Result<UntypedAssetId, Self::Error> {
            Err(std::io::Error::other("readonly"))
        }

        fn read_asset_tags(&self, _: &Path) -> Result<Option<AssetTags>, Self::Error> {
            Ok(Some(self.tags.clone()))
        }

        fn write_asset_tags(&self, _: &Path, _: &AssetTags) -> Result<(), Self::Error> {
            Err(std::io::Error::other("readonly"))
        }
    }

    #[test]
    fn readonly_asset_tags_are_overridden_in_modified_directory() -> AssetResult<()> {
        let root = std::env::temp_dir().join(format!("cyancia-readonly-tags-{}", Uuid::new_v4()));
        let bundle_id = BundleId::new(Uuid::from_u128(1));
        let asset_id = UntypedAssetId::new(Uuid::from_u128(2));
        let asset_path = PathBuf::from("brushes/sample.cbp");
        let base_tag = TagId::new(Uuid::from_u128(3));
        let override_tag = TagId::new(Uuid::from_u128(4));
        let bundle = Arc::new(ReadonlyBundle {
            id: bundle_id,
            asset_id,
            asset_path: asset_path.clone(),
            tags: AssetTags {
                tags: std::collections::BTreeSet::from([base_tag.clone()]),
            },
        });
        let cache = AssetBundleCache::new(
            &root,
            bundle,
            HashMap::from([(asset_id, asset_path.clone())]),
            Arc::new(AssetSerializerRegistry::default()),
        )?;

        assert_eq!(
            cache.read_asset_tags(&asset_id)?.tags,
            std::collections::BTreeSet::from([base_tag])
        );

        cache.write_asset_tags(
            &asset_id,
            &AssetTags {
                tags: std::collections::BTreeSet::from([override_tag.clone()]),
            },
        )?;
        assert_eq!(
            cache.read_asset_tags(&asset_id)?.tags,
            std::collections::BTreeSet::from([override_tag])
        );
        assert!(
            modified_bundle_absolute_path(&root, &bundle_id)
                .join(asset_path.with_added_extension(ASSET_TAGS_EXT))
                .is_file()
        );

        cache.write_asset_tags(&asset_id, &AssetTags::default())?;
        assert!(cache.read_asset_tags(&asset_id)?.tags.is_empty());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
