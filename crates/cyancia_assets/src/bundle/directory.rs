use std::{
    collections::HashMap,
    fs::{File, metadata, read_to_string},
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Local, TimeZone, Utc};
use uuid::Uuid;

use crate::{
    asset::{AssetId, AssetMetadata, AssetPhysicalLocation, ErasedAsset},
    bundle::{AssetBundle, AssetBundleMetadata, BundleId},
    loader::{AssetSerializerRegistry, ErasedAssetSerializer},
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
    #[error("Toml error: {0}")]
    TomlError(#[from] toml::de::Error),
}

impl AssetBundle for AssetDirectory {
    type Error = DataDirectoryError;

    fn metadata(&self) -> Result<AssetBundleMetadata, DataDirectoryError> {
        let last_modified = DateTime::from(metadata(&self.root)?.modified()?);

        Ok(AssetBundleMetadata {
            bundle_id: self.id,
            name: self.name.clone(),
            last_modified,
        })
    }

    fn manifest(&self) -> Result<HashMap<AssetId, PathBuf>, DataDirectoryError> {
        let path = self.root.join("manifest.toml");
        let exists = path.exists();
        if !exists || metadata(&path)?.modified()? != metadata(&self.root)?.modified()? {
            if exists {
                // Or the manifest itself will be scanned.
                std::fs::remove_file(&path)?;
            }

            let manifest = scan_dir(&self.root, &self.id)?;
            let manifest_str = toml::to_string_pretty(&manifest).unwrap();
            std::fs::write(&path, manifest_str)?;
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
}

fn scan_dir(
    root: &Path,
    bundle_id: &BundleId,
) -> Result<HashMap<AssetId, PathBuf>, DataDirectoryError> {
    let mut assets = HashMap::new();
    scan_dir_dfs(root, root, bundle_id, &mut assets)?;
    Ok(assets)
}

fn scan_dir_dfs(
    current_path: &Path,
    root: &Path,
    bundle_id: &BundleId,
    assets: &mut HashMap<AssetId, PathBuf>,
) -> Result<(), DataDirectoryError> {
    let entries = std::fs::read_dir(current_path)?;
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_dir() {
            let _ = scan_dir_dfs(&path, root, bundle_id, assets)?;
        } else {
            let relative_path = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            let asset_id = AssetId::new(Uuid::from_u128(xxhash_rust::xxh3::xxh3_128(
                relative_path.as_bytes(),
            )));

            assets.insert(asset_id, relative_path.into());
        }
    }

    Ok(())
}
