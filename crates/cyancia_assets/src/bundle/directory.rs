use std::{
    ffi::OsStr,
    fs::{File, create_dir_all, metadata, read_to_string},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::DateTime;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    asset::{ErasedAsset, UntypedAssetId},
    bundle::{AssetBundle, AssetBundleMetadata, BundleId, BundleManifest},
    error::AssetResult,
    loader::ErasedAssetSerializer,
    tag::{ASSET_TAGS_EXT, AssetTags, TAG_EXT, TagFile},
};

const METADATA_FILE_NAME: &str = "metadata.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetDirectoryMetadata {
    pub bundle_id: BundleId,
}

pub struct AssetDirectory {
    root: PathBuf,
    id: BundleId,
    name: String,
    manifest: Mutex<BundleManifest>,
}

impl AssetDirectory {
    pub fn new(root: impl AsRef<Path>) -> AssetResult<Self> {
        let root = root.as_ref();
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();

        let metadata_path = root.join(METADATA_FILE_NAME);
        let metadata = if metadata_path.exists() {
            toml::from_str(&read_to_string(&metadata_path)?)?
        } else {
            let metadata = AssetDirectoryMetadata {
                bundle_id: BundleId::new(Uuid::new_v4()),
            };
            File::create_new(&metadata_path)?.write_all(toml::to_string(&metadata)?.as_bytes())?;
            metadata
        };

        let mut manifest = BundleManifest::default();
        scan_dir_dfs(root, root, &metadata.bundle_id, &mut manifest)?;

        Ok(Self {
            id: metadata.bundle_id,
            name,
            root: root.into(),
            manifest: Mutex::new(manifest),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DataDirectoryError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serializer error: {0}")]
    SerializerError(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("Toml serialization error: {0}")]
    TomlSerError(#[from] toml::ser::Error),
    #[error("Toml deserialization error: {0}")]
    TomlDeError(#[from] toml::de::Error),
}

impl AssetBundle for AssetDirectory {
    const READONLY: bool = false;

    type Error = DataDirectoryError;

    fn metadata(&self) -> Result<AssetBundleMetadata, DataDirectoryError> {
        let last_modified = DateTime::from(metadata(&self.root)?.modified()?);

        Ok(AssetBundleMetadata {
            bundle_id: self.id,
            name: self.name.clone(),
            last_modified,
        })
    }

    fn manifest(&self) -> Result<BundleManifest, DataDirectoryError> {
        Ok(self.manifest.lock().clone())
    }

    fn read_asset(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Self::Error> {
        let asset_path = self.root.join(path);
        let mut file = File::open(&asset_path)?;
        let asset = serializer
            .read(&mut file)
            .map_err(DataDirectoryError::SerializerError)?;
        Ok(asset.into())
    }

    fn add_asset(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, DataDirectoryError> {
        let path = path_clean::clean(path);
        let asset_path = self.root.join(&path);
        if let Some(parent) = asset_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&asset_path)?;
        serializer
            .write(asset, &mut file)
            .map_err(DataDirectoryError::SerializerError)?;
        let asset_id = asset_id_from_relative_path(&self.id, &path);
        self.manifest.lock().assets.insert(asset_id, path);
        Ok(asset_id)
    }

    fn read_tag(&self, tag: &Path) -> Result<TagFile, Self::Error> {
        let path = self.root.join(tag);
        Ok(toml::from_str(&read_to_string(path)?)?)
    }

    fn add_tag(&self, path: &Path, tag: &TagFile) -> Result<(), Self::Error> {
        let path = path_clean::clean(path);
        let tag_path = self.root.join(&path);
        if let Some(parent) = tag_path.parent() {
            create_dir_all(parent)?;
        }
        File::create(tag_path)?.write_all(toml::to_string(tag)?.as_bytes())?;

        self.manifest.lock().tags.insert(tag.id, path);

        Ok(())
    }

    fn read_asset_tags(&self, path: &Path) -> Result<Option<AssetTags>, Self::Error> {
        let path = self.root.join(path).with_added_extension(ASSET_TAGS_EXT);
        if !path.exists() {
            return Ok(None);
        }

        Ok(Some(toml::from_str(&read_to_string(path)?)?))
    }

    fn write_asset_tags(&self, path: &Path, tags: &AssetTags) -> Result<(), Self::Error> {
        let path = self.root.join(path).with_added_extension(ASSET_TAGS_EXT);
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        File::create(path)?.write_all(toml::to_string(tags)?.as_bytes())?;
        Ok(())
    }
}

fn scan_dir_dfs(
    current_path: &Path,
    root: &Path,
    bundle_id: &BundleId,
    manifest: &mut BundleManifest,
) -> AssetResult<()> {
    let entries = std::fs::read_dir(current_path)?;
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_dir() {
            scan_dir_dfs(&path, root, bundle_id, manifest)?;
        } else {
            if path.extension() == Some(OsStr::new(ASSET_TAGS_EXT)) {
                continue;
            }

            let relative_path = path.strip_prefix(root).unwrap();
            if relative_path == Path::new(METADATA_FILE_NAME) {
                continue;
            }

            if path.extension() == Some(OsStr::new(TAG_EXT)) {
                let tag = toml::from_slice::<TagFile>(&std::fs::read(&path)?)?;
                manifest.tags.insert(tag.id, relative_path.into());
            } else {
                let asset_id = asset_id_from_relative_path(bundle_id, relative_path);

                manifest.assets.insert(asset_id, relative_path.into());
            }
        }
    }

    Ok(())
}

fn asset_id_from_relative_path(bundle_id: &BundleId, path: &Path) -> UntypedAssetId {
    let path_str = path_clean::clean(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    let path_bytes = path_str.as_bytes();
    let mut key = Vec::with_capacity(bundle_id.as_bytes().len() + path_bytes.len());
    key.extend_from_slice(bundle_id.as_bytes());
    key.extend_from_slice(path_bytes);

    UntypedAssetId::new(Uuid::from_u128(xxhash_rust::xxh3::xxh3_128(&key)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::tag::TagId;

    #[test]
    fn asset_id_is_namespaced_by_bundle_id() {
        let path = Path::new("brushes/sample.cbp");
        let first_bundle = BundleId::new(Uuid::from_u128(1));
        let second_bundle = BundleId::new(Uuid::from_u128(2));

        let first_id = asset_id_from_relative_path(&first_bundle, path);
        assert_eq!(
            first_id,
            asset_id_from_relative_path(&first_bundle, Path::new("./brushes/sample.cbp"))
        );
        assert_ne!(first_id, asset_id_from_relative_path(&second_bundle, path));
    }

    #[test]
    fn asset_tags_are_read_written_and_overwritten() -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!("cyancia-asset-tags-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root)?;
        let bundle = AssetDirectory::new(&root)?;
        let asset_path = Path::new("brushes/sample.cbp");
        let first_tag = TagId::new(Uuid::from_u128(1));
        let second_tag = TagId::new(Uuid::from_u128(2));

        assert!(bundle.read_asset_tags(asset_path)?.is_none());

        bundle.write_asset_tags(
            asset_path,
            &AssetTags {
                tags: BTreeSet::from([first_tag]),
            },
        )?;
        assert_eq!(
            bundle.read_asset_tags(asset_path)?.unwrap().tags,
            BTreeSet::from([first_tag])
        );
        assert!(root.join("brushes/sample.cbp.tags").is_file());
        let manifest = bundle.manifest()?;
        assert!(manifest.assets.is_empty());
        assert!(manifest.tags.is_empty());

        bundle.write_asset_tags(
            asset_path,
            &AssetTags {
                tags: BTreeSet::from([second_tag]),
            },
        )?;
        assert_eq!(
            bundle.read_asset_tags(asset_path)?.unwrap().tags,
            BTreeSet::from([second_tag])
        );

        bundle.write_asset_tags(asset_path, &AssetTags::default())?;
        assert!(bundle.read_asset_tags(asset_path)?.unwrap().tags.is_empty());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
