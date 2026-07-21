use std::{
    collections::HashMap,
    fs::{File, create_dir_all, metadata, read_to_string},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::DateTime;
use uuid::Uuid;

use crate::{
    asset::{ErasedAsset, UntypedAssetId},
    bundle::{AssetBundle, AssetBundleMetadata, BundleId, asset_tags_path},
    loader::ErasedAssetSerializer,
    tag::AssetTags,
};

pub struct AssetDirectory {
    root: PathBuf,
    id: BundleId,
    name: String,
}

impl AssetDirectory {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let id = BundleId::new(Uuid::from_u128(xxhash_rust::xxh3::xxh3_128(
            name.as_bytes(),
        )));

        Self {
            id,
            name,
            root: root.into(),
        }
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

    fn manifest(&self) -> Result<HashMap<UntypedAssetId, PathBuf>, DataDirectoryError> {
        let path = self.root.join("manifest.toml");
        let exists = path.exists();
        if !exists || metadata(&path)?.modified()? != metadata(&self.root)?.modified()? {
            if exists {
                // Or the manifest itself will be scanned.
                std::fs::remove_file(&path)?;
            }

            let manifest = scan_dir(&self.root)?;
            std::fs::write(&path, toml::to_string(&manifest)?)?;
            Ok(manifest)
        } else {
            Ok(toml::from_str(&read_to_string(&path)?)?)
        }
    }

    fn read(
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

    fn add(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, DataDirectoryError> {
        let asset_path = self.root.join(path);
        if let Some(parent) = asset_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&asset_path)?;
        serializer
            .write(asset, &mut file)
            .map_err(DataDirectoryError::SerializerError)?;
        let path_str = path.to_string_lossy().to_string();
        let asset_id = asset_id_from_relative_path(&path_str);

        File::options()
            .append(true)
            .open(self.root.join("manifest.toml"))?
            // Only works because it's toml.
            .write_all(format!("{} = \"{}\"", asset_id, path_str).as_bytes())?;
        Ok(asset_id)
    }

    fn read_asset_tags(&self, path: &Path) -> Result<Option<AssetTags>, Self::Error> {
        let path = self.root.join(asset_tags_path(path));
        if !path.exists() {
            return Ok(None);
        }

        Ok(Some(toml::from_str(&read_to_string(path)?)?))
    }

    fn write_asset_tags(&self, path: &Path, tags: &AssetTags) -> Result<(), Self::Error> {
        let path = self.root.join(asset_tags_path(path));
        if let Some(parent) = path.parent() {
            create_dir_all(parent)?;
        }
        File::create(path)?.write_all(toml::to_string(tags)?.as_bytes())?;
        Ok(())
    }
}

fn scan_dir(root: &Path) -> Result<HashMap<UntypedAssetId, PathBuf>, DataDirectoryError> {
    let mut assets = HashMap::new();
    scan_dir_dfs(root, root, &mut assets)?;
    Ok(assets)
}

fn scan_dir_dfs(
    current_path: &Path,
    root: &Path,
    assets: &mut HashMap<UntypedAssetId, PathBuf>,
) -> Result<(), DataDirectoryError> {
    let entries = std::fs::read_dir(current_path)?;
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_dir() {
            scan_dir_dfs(&path, root, assets)?;
        } else {
            let relative_path = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            let asset_id = asset_id_from_relative_path(&relative_path);

            assets.insert(asset_id, relative_path.into());
        }
    }

    Ok(())
}

fn asset_id_from_relative_path(path: &str) -> UntypedAssetId {
    UntypedAssetId::new(Uuid::from_u128(xxhash_rust::xxh3::xxh3_128(
        path.as_bytes(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tag::TagId;

    #[test]
    fn asset_tags_are_read_written_and_overwritten() -> Result<(), DataDirectoryError> {
        let root = std::env::temp_dir().join(format!("cyancia-asset-tags-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root)?;
        let bundle = AssetDirectory::new(&root);
        let asset_path = Path::new("brushes/sample.cbp");
        let first_tag = TagId::new(Uuid::from_u128(1));
        let second_tag = TagId::new(Uuid::from_u128(2));

        assert!(bundle.read_asset_tags(asset_path)?.is_none());

        bundle.write_asset_tags(
            asset_path,
            &AssetTags {
                tags: std::collections::BTreeSet::from([first_tag.clone()]),
            },
        )?;
        assert_eq!(
            bundle.read_asset_tags(asset_path)?.unwrap().tags,
            std::collections::BTreeSet::from([first_tag])
        );
        assert!(root.join("brushes/sample.cbp.tags").is_file());

        bundle.write_asset_tags(
            asset_path,
            &AssetTags {
                tags: std::collections::BTreeSet::from([second_tag.clone()]),
            },
        )?;
        assert_eq!(
            bundle.read_asset_tags(asset_path)?.unwrap().tags,
            std::collections::BTreeSet::from([second_tag])
        );

        bundle.write_asset_tags(asset_path, &AssetTags::default())?;
        assert!(bundle.read_asset_tags(asset_path)?.unwrap().tags.is_empty());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
