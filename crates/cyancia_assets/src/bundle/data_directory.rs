use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::DateTime;
use filetime::FileTime;
use uuid::Uuid;

use crate::{
    asset::{AssetMetadata, ErasedAsset},
    bundle::{AssetBundle, BundleId},
    loader::AssetSerializerRegistry,
};

pub struct CyanciaDataDirectory {
    root: PathBuf,
}

impl AssetBundle for CyanciaDataDirectory {
    fn hash(&self) -> String {
        String::new() // TODO: Implement hash for data directory.
    }

    fn all_assets(&self, serializers: &AssetSerializerRegistry) -> Vec<AssetMetadata> {
        let mut metadata = Vec::new();
        scan_all_assets(&self.root, serializers, &mut metadata);
        metadata
    }

    fn read_by_path(
        &self,
        path: &str,
        serializers: &AssetSerializerRegistry,
    ) -> Option<Arc<dyn ErasedAsset>> {
        let abs_path = self.root.join(path);
        if !abs_path.exists() {
            return None;
        }
        let ext = abs_path.extension()?.to_str()?;
        let mut file = File::open(&abs_path).ok()?;

        Some(serializers.get(ext)?.read(&mut file).ok()?.into())
    }

    fn write_by_path(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) {
        let abs_path = self.root.join(path);
        if let Some(parent) = abs_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let Ok(mut file) = File::create(&abs_path) else {
            return;
        };
        let Some(ext) = abs_path.extension().and_then(|e| e.to_str()) else {
            return;
        };

        if let Some(serializer) = serializers.get(ext) {
            serializer.write(asset, &mut file).ok();
        }
    }
}

fn scan_all_assets(
    root: &Path,
    serializers: &AssetSerializerRegistry,
    metadata: &mut Vec<AssetMetadata>,
) {
    let Ok(entires) = root.read_dir() else {
        return;
    };

    for entry in entires {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_dir() {
            scan_all_assets(&path, serializers, metadata);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && let Some(serializer) = serializers.get(ext)
            && let Ok(mut file) = File::open(&path)
            && let Ok(file_metadata) = file.metadata()
            && let Ok(content) = serializer.read(&mut file)
        {
            let relative_path = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();

            let updated_at = DateTime::from_timestamp(
                FileTime::from_last_modification_time(&file_metadata).unix_seconds(),
                0,
            )
            .unwrap();

            metadata.push(AssetMetadata {
                bundle_id: BundleId::new("DataDirectory".to_string()),
                asset_type: serializer.asset_type_name().to_string(),
                relative_path,
                content_hash: content.hash(),
                updated_at,
            });
        }
    }
}
