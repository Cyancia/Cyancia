use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::DateTime;
use filetime::FileTime;
use uuid::Uuid;

use crate::{
    asset::{AssetMetadata, AssetPhysicalLocation, ErasedAsset},
    bundle::{AssetBundle, BundleId},
    loader::AssetSerializerRegistry,
};

pub struct DataDirectory {
    root: PathBuf,
}

pub const DATA_DIRECTORY_BUNDLE_ID: BundleId = BundleId::new(Uuid::from_u128(0));

impl DataDirectory {
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
    #[error("Missing serializer for extension: {0}")]
    MissingSerializer(String),
    #[error("Missing extension for path: {0}")]
    MissingExtension(PathBuf),
    #[error("Serializer error: {0}")]
    SerializerError(Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl AssetBundle for DataDirectory {
    type Error = DataDirectoryError;

    fn id(&self) -> BundleId {
        DATA_DIRECTORY_BUNDLE_ID
    }

    fn hash(&self) -> String {
        todo!()
    }

    fn is_read_only() -> bool {
        false
    }

    fn read(
        &self,
        serializers: &AssetSerializerRegistry,
    ) -> Result<HashMap<String, Arc<dyn ErasedAsset>>, Self::Error> {
        let mut assets = HashMap::new();
        scan_all_assets(&self.root, serializers, &mut assets)?;
        Ok(assets)
    }

    fn write(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) -> Result<(), Self::Error> {
        let abs_path = self.root.join(path);
        if let Some(parent) = abs_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let mut file = File::create(&abs_path)?;
        let Some(ext) = abs_path.extension().and_then(|e| e.to_str()) else {
            return Err(DataDirectoryError::MissingExtension(abs_path));
        };

        let serializer = serializers
            .get(ext)
            .ok_or_else(|| DataDirectoryError::MissingSerializer(ext.to_string()))?;
        serializer
            .write(asset, &mut file)
            .map_err(DataDirectoryError::SerializerError)
    }
}

fn scan_all_assets(
    root: &Path,
    serializers: &AssetSerializerRegistry,
    assets: &mut HashMap<String, Arc<dyn ErasedAsset>>,
) -> Result<(), DataDirectoryError> {
    let entries = std::fs::read_dir(root)?;

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_dir() {
            scan_all_assets(&path, serializers, assets)?;
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .ok_or_else(|| DataDirectoryError::MissingExtension(path.clone()))?;
            let mut file = File::open(&path)?;
            let serializer = serializers
                .get(ext)
                .ok_or_else(|| DataDirectoryError::MissingSerializer(ext.to_string()))?;

            let asset = serializer
                .read(&mut file)
                .map_err(DataDirectoryError::SerializerError)?;
            let relative_path = path
                .strip_prefix(root)
                .unwrap()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string();
            assets.insert(relative_path, asset.into());
        }
    }

    Ok(())
}
