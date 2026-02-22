use std::{
    collections::HashMap,
    fs::{File, metadata},
    io::{Cursor, Read, read_to_string},
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::DateTime;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::{
    asset::{AssetId, AssetMetadata, ErasedAsset},
    bundle::{AssetBundle, AssetBundleMetadata, BundleId},
    loader::{AssetSerializerRegistry, ErasedAssetSerializer},
};

pub struct StandardAssetBundle {
    path: PathBuf,
    archive: RwLock<ZipArchive<File>>,
}

impl StandardAssetBundle {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StandardAssetBundleError> {
        let path = path.as_ref().to_path_buf();
        let archive = ZipArchive::new(File::open(&path)?)?;

        Ok(Self {
            path,
            archive: archive.into(),
        })
    }

    pub async fn scan_bundles(
        root: impl AsRef<Path>,
    ) -> (Vec<Self>, Vec<StandardAssetBundleError>) {
        let mut bundles = Vec::new();
        let mut errors = Vec::new();
        scan_bundles(root, &mut bundles, &mut errors).await;
        (bundles, errors)
    }
}

async fn scan_bundles(
    root: impl AsRef<Path>,
    bundles: &mut Vec<StandardAssetBundle>,
    errors: &mut Vec<StandardAssetBundleError>,
) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) => {
            errors.push(StandardAssetBundleError::Io(e));
            return;
        }
    };

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_file() {
            let ext = path.extension().and_then(|ext| ext.to_str());
            if ext == Some("csb") {
                match StandardAssetBundle::new(&path) {
                    Ok(bundle) => bundles.push(bundle),
                    Err(e) => errors.push(e),
                }
            }
        } else if path.is_dir() {
            Box::pin(scan_bundles(path, bundles, errors)).await;
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StandardAssetBundleError {
    #[error("Unsupported writing to standard asset bundle")]
    UnsupportedWriting,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Missing serializer for extension: {0}")]
    MissingSerializer(String),
    #[error("Serializer error: {0}")]
    SerializerError(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("Toml error: {0}")]
    TomlError(#[from] toml::de::Error),
}

#[derive(Serialize, Deserialize)]
pub struct StandardAssetBundleMetadata {
    pub bundle_id: BundleId,
    pub name: String,
}

impl AssetBundle for StandardAssetBundle {
    const READONLY: bool = true;

    type Error = StandardAssetBundleError;

    fn metadata(&self) -> Result<AssetBundleMetadata, Self::Error> {
        let mut archive = self.archive.write();
        let content = read_to_string(archive.by_name("metadata.toml")?)?;
        let bundle_meta = toml::from_str::<StandardAssetBundleMetadata>(&content)?;
        let last_modified = DateTime::from(metadata(&self.path)?.modified()?);

        Ok(AssetBundleMetadata {
            bundle_id: bundle_meta.bundle_id,
            name: bundle_meta.name,
            last_modified,
        })
    }

    fn manifest(&self) -> Result<HashMap<AssetId, PathBuf>, Self::Error> {
        let mut archive = self.archive.write();
        let content = read_to_string(archive.by_name("manifest.toml")?)?;
        Ok(toml::from_str(&content)?)
    }

    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Self::Error> {
        let mut archive = self.archive.write();
        let mut file = archive.by_name(path.to_str().unwrap_or_default())?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer)?;
        let asset = serializer
            .read(&mut Cursor::new(buffer))
            .map_err(StandardAssetBundleError::SerializerError)?;
        Ok(asset.into())
    }

    fn add(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<AssetId, StandardAssetBundleError> {
        Err(StandardAssetBundleError::UnsupportedWriting)
    }
}
