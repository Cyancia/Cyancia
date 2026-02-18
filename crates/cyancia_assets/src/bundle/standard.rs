use std::{
    collections::HashMap,
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use crate::{
    asset::{AssetMetadata, ErasedAsset},
    bundle::{AssetBundle, BundleId},
    loader::AssetSerializerRegistry,
};

pub struct StandardAssetBundle {
    metadata: StandardAssetBundleMetadata,
    path: PathBuf,
}

impl StandardAssetBundle {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StandardAssetBundleError> {
        let path = path.as_ref().to_path_buf();
        let mut archive = ZipArchive::new(File::open(&path)?)?;

        let metadata = {
            let mut file = archive.by_name("metadata.toml")?;
            let mut content = String::new();
            file.read_to_string(&mut content)?;
            toml::from_str::<StandardAssetBundleMetadata>(&content)?
        };

        Ok(Self { metadata, path })
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

#[derive(Serialize, Deserialize, Debug)]
pub struct StandardAssetBundleMetadata {
    pub id: BundleId,
}

impl AssetBundle for StandardAssetBundle {
    type Error = StandardAssetBundleError;

    fn id(&self) -> BundleId {
        self.metadata.id
    }

    fn is_read_only() -> bool {
        true
    }

    fn read(
        &self,
        serializers: &AssetSerializerRegistry,
    ) -> Result<HashMap<String, Arc<dyn ErasedAsset>>, Self::Error> {
        let mut archive = ZipArchive::new(File::open(&self.path)?)?;
        let mut assets = HashMap::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;

            if file.name() == "metadata.toml" {
                continue;
            }

            let mut content = Vec::new();
            file.read_to_end(&mut content)?;

            let extension = Path::new(file.name())
                .extension()
                .and_then(|ext| ext.to_str())
                .ok_or_else(|| {
                    StandardAssetBundleError::MissingSerializer(file.name().to_string())
                })?;

            let serializer = serializers.get(extension).ok_or_else(|| {
                StandardAssetBundleError::MissingSerializer(extension.to_string())
            })?;

            let asset = serializer
                .read(&mut Cursor::new(content))
                .map_err(StandardAssetBundleError::SerializerError)?;

            assets.insert(file.name().to_string(), asset.into());
        }

        Ok(assets)
    }

    fn write(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) -> Result<(), Self::Error> {
        Err(StandardAssetBundleError::UnsupportedWriting)
    }
}
