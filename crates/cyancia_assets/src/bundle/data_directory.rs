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

#[cfg(test)]
mod tests {
    use std::{io::Read, path::PathBuf};

    use super::*;
    use crate::{asset::Asset, loader::AssetSerializer};

    #[derive(Default)]
    struct TestTextAssetSerializer;

    struct TestTextAsset {
        value: String,
    }

    impl Asset for TestTextAsset {
        const TYPE_NAME: &'static str = "test_text_asset";

        fn hash(&self) -> String {
            self.value.clone()
        }
    }

    impl AssetSerializer for TestTextAssetSerializer {
        type Asset = TestTextAsset;
        type Error = std::io::Error;

        fn file_extension() -> &'static str {
            "tasset"
        }

        fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
            let mut value = String::new();
            reader.read_to_string(&mut value)?;
            Ok(TestTextAsset { value })
        }

        fn write(
            &self,
            asset: &Self::Asset,
            writer: &mut dyn std::io::Write,
        ) -> Result<(), Self::Error> {
            writer.write_all(asset.value.as_bytes())
        }
    }

    fn serializers() -> AssetSerializerRegistry {
        let mut serializers = AssetSerializerRegistry::new();
        serializers.register::<TestTextAssetSerializer>();
        serializers
    }

    fn sample_assets_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test_assets")
            .join("sample")
    }

    #[test]
    fn test_all_assets() {
        let serializers = serializers();
        let bundle = CyanciaDataDirectory {
            root: sample_assets_root(),
        };

        let mut assets = bundle.all_assets(&serializers);
        assets.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

        assert_eq!(assets.len(), 2);
        assert_eq!(assets[0].relative_path, "a.tasset");
        assert_eq!(assets[0].content_hash, "alpha");
        assert_eq!(assets[0].asset_type, TestTextAsset::TYPE_NAME);
        assert_eq!(assets[1].relative_path, "b.tasset");
        assert_eq!(assets[1].content_hash, "beta");
        assert_eq!(assets[1].asset_type, TestTextAsset::TYPE_NAME);
    }

    #[test]
    fn test_read_by_path() {
        let serializers = serializers();
        let bundle = CyanciaDataDirectory {
            root: sample_assets_root(),
        };

        let asset = bundle
            .read_by_path("a.tasset", &serializers)
            .and_then(|a| a.downcast_arc::<TestTextAsset>().ok())
            .expect("expected readable test asset");

        assert_eq!(asset.value, "alpha");
    }

    #[test]
    fn test_write_by_path() {
        let serializers = serializers();
        let runtime_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("test_assets")
            .join(format!("runtime_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&runtime_root).unwrap();

        let bundle = CyanciaDataDirectory {
            root: runtime_root.clone(),
        };

        bundle.write_by_path(
            "nested/new.tasset",
            &TestTextAsset {
                value: "written".to_string(),
            },
            &serializers,
        );

        let file_content = std::fs::read_to_string(runtime_root.join("nested/new.tasset")).unwrap();
        assert_eq!(file_content, "written");

        let read_back = bundle
            .read_by_path("nested/new.tasset", &serializers)
            .and_then(|a| a.downcast_arc::<TestTextAsset>().ok())
            .expect("expected read back asset");
        assert_eq!(read_back.value, "written");

        std::fs::remove_dir_all(runtime_root).unwrap();
    }
}
