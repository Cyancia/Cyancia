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
}

impl AssetDirectory {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().into(),
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
        let folder_name = self
            .root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        let last_modified = DateTime::from(metadata(&self.root)?.modified()?);

        Ok(AssetBundleMetadata {
            bundle_id: BundleId::new(Uuid::from_u128(xxhash_rust::xxh3::xxh3_128(
                folder_name.as_bytes(),
            ))),
            name: folder_name.to_string(),
            last_modified,
        })
    }

    fn manifest(&self) -> Result<HashMap<AssetId, PathBuf>, DataDirectoryError> {
        Ok(toml::from_str(&read_to_string(
            self.root.join("manifest.toml"),
        )?)?)
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
