use std::{
    collections::HashMap,
    fs,
    io::read_to_string,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Once},
};

use cyancia_assets::{
    asset::{Asset, UntypedAssetId},
    bundle::{
        AssetBundle, directory::AssetDirectory, modified_bundle_absolute_path,
        standard::StandardAssetBundle,
    },
    index_db::AssetFilter,
    loader::{AssetSerializer, AssetSerializerRegistry, AssetSerializerRegistryBuilder},
    store::AssetRegistry,
    tag::{Tag, TagSerializer},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct TestAsset {
    name: String,
    value: i32,
}

impl Asset for TestAsset {
    const TYPE_NAME: &'static str = "test_asset";
}

#[derive(Default)]
struct TestAssetSerializer;

#[derive(Debug, thiserror::Error)]
enum TestAssetSerializerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Toml serialization error: {0}")]
    TomlSerError(#[from] toml::ser::Error),
    #[error("Toml deserialization error: {0}")]
    TomlDeError(#[from] toml::de::Error),
}

impl AssetSerializer for TestAssetSerializer {
    type Asset = TestAsset;

    type Error = TestAssetSerializerError;

    fn file_extension() -> &'static str {
        "toml"
    }

    fn read(&self, reader: &mut dyn std::io::Read) -> Result<Self::Asset, Self::Error> {
        Ok(toml::from_str(&read_to_string(reader)?)?)
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        let content = toml::to_string(asset)?;
        writer.write_all(content.as_bytes())?;
        Ok(())
    }
}

static LOGGER_INIT: Once = Once::new();

fn init_logger() {
    LOGGER_INIT.call_once(|| {
        let _ = env_logger::builder().is_test(true).try_init();
    });
}

struct CwdGuard {
    original: PathBuf,
}

impl CwdGuard {
    fn enter(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let original = std::env::current_dir()?;
        std::env::set_current_dir(path)?;
        Ok(Self { original })
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original);
    }
}

#[derive(Clone)]
struct FileSnapshot {
    path: PathBuf,
    content: Option<Vec<u8>>,
}

#[derive(Clone)]
struct DirSnapshot {
    path: PathBuf,
    files: Option<Vec<(PathBuf, Vec<u8>)>>,
}

#[derive(Default)]
struct FsRestoreGuard {
    files: Vec<FileSnapshot>,
    dirs: Vec<DirSnapshot>,
}

impl FsRestoreGuard {
    fn snapshot_file(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref().to_path_buf();
        let content = if path.exists() {
            Some(fs::read(&path)?)
        } else {
            None
        };
        self.files.push(FileSnapshot { path, content });
        Ok(())
    }

    fn snapshot_dir(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let path = path.as_ref().to_path_buf();
        let files = if path.exists() {
            let mut entries = Vec::new();
            collect_dir_files(&path, &path, &mut entries)?;
            Some(entries)
        } else {
            None
        };
        self.dirs.push(DirSnapshot { path, files });
        Ok(())
    }

    fn restore(&self) -> std::io::Result<()> {
        for file in self.files.iter().rev() {
            remove_path_if_exists(&file.path)?;
            if let Some(content) = &file.content {
                if let Some(parent) = file.path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&file.path, content)?;
            }
        }

        for dir in self.dirs.iter().rev() {
            remove_path_if_exists(&dir.path)?;
            if let Some(files) = &dir.files {
                fs::create_dir_all(&dir.path)?;
                for (rel_path, content) in files {
                    let abs = dir.path.join(rel_path);
                    if let Some(parent) = abs.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(abs, content)?;
                }
            }
        }

        Ok(())
    }
}

impl Drop for FsRestoreGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn collect_dir_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(PathBuf, Vec<u8>)>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_dir_files(root, &path, out)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            out.push((rel, fs::read(path)?));
        }
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn parse_id(id: &str) -> UntypedAssetId {
    UntypedAssetId::new(Uuid::from_str(id).unwrap())
}

fn parse_test_asset(path: impl AsRef<Path>) -> TestAsset {
    toml::from_str::<TestAsset>(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn test() {
    init_logger();

    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let _cwd = CwdGuard::enter(&crate_root).unwrap();

    let assets_root = crate_root.join("assets");
    let local_assets_root = assets_root.join("local_assets");
    let local_manifest_path = local_assets_root.join("manifest.toml");
    let local_tag_path = local_assets_root.join("my_tag.ctag");
    let local_test1_path = local_assets_root.join("test1.toml");
    let local_test_hello_path = local_assets_root.join("test_hello.toml");
    let bundle_path = assets_root.join("test_bundle.csb");

    let local_bundle = AssetDirectory::new(&local_assets_root);
    let local_bundle_id = local_bundle.metadata().unwrap().bundle_id;
    let local_modified_dir = modified_bundle_absolute_path(&assets_root, &local_bundle_id);

    let mut restore_guard = FsRestoreGuard::default();
    restore_guard.snapshot_file(&local_manifest_path).unwrap();
    restore_guard.snapshot_file(&local_tag_path).unwrap();
    restore_guard.snapshot_file(&local_test1_path).unwrap();
    restore_guard.snapshot_file(&local_test_hello_path).unwrap();
    restore_guard.snapshot_dir(&local_modified_dir).unwrap();
    restore_guard
        .snapshot_file(&local_assets_root.join("added_asset.toml"))
        .unwrap();

    if local_manifest_path.exists() {
        fs::remove_file(&local_manifest_path).unwrap();
    }

    let mut serializers = AssetSerializerRegistryBuilder::default();
    serializers.add_serializer::<TestAssetSerializer>();
    serializers.add_serializer::<TagSerializer>();

    let mut registry =
        AssetRegistry::new(&assets_root, serializers.consume_and_build().into()).unwrap();

    registry.add_bundle(local_bundle).unwrap();
    registry
        .add_bundle(StandardAssetBundle::new(&bundle_path).unwrap())
        .unwrap();

    let test_asset_handles = registry.all_handles_of::<TestAsset>().unwrap();
    assert_eq!(test_asset_handles.len(), 3);

    let mut test_assets_by_name = HashMap::new();
    let mut test_asset_locations = HashMap::new();
    for handle in &test_asset_handles {
        let asset = handle.get().unwrap();
        test_asset_locations.insert(
            asset.name.clone(),
            (handle.bundle().metadata().bundle_id, handle.id()),
        );
        test_assets_by_name.insert(asset.name.clone(), asset.value);
    }

    assert_eq!(test_assets_by_name.get("Test Asset"), Some(&42));
    assert_eq!(test_assets_by_name.get("Hello World"), Some(&12345));
    assert_eq!(test_assets_by_name.get("Test Asset 2"), Some(&84));

    let test_asset_handle = registry
        .handle::<TestAsset>(test_asset_locations.get("Test Asset").unwrap().1)
        .unwrap();

    test_asset_handle
        .update(TestAsset {
            name: "Test Asset".to_string(),
            value: 420,
        })
        .unwrap();
    assert_eq!(
        test_asset_handle.get().unwrap().as_ref(),
        &TestAsset {
            name: "Test Asset".to_string(),
            value: 420,
        }
    );

    test_asset_handle
        .update(TestAsset {
            name: "Test Asset".to_string(),
            value: 421,
        })
        .unwrap();
    assert_eq!(
        test_asset_handle.get().unwrap().as_ref(),
        &TestAsset {
            name: "Test Asset".to_string(),
            value: 421,
        }
    );

    test_asset_handle.write().unwrap();
    let written_meta = test_asset_handle.metadata().unwrap();
    assert_eq!(written_meta.revision, 2);
    assert!(!written_meta.in_memory);

    let written_file = modified_bundle_absolute_path(&assets_root, &local_bundle_id)
        .join(&written_meta.relative_path);
    assert!(written_file.exists());
    assert_eq!(
        parse_test_asset(&written_file),
        TestAsset {
            name: "Test Asset".to_string(),
            value: 421,
        }
    );

    let tag_handles = registry.all_handles_of::<Tag>().unwrap();
    assert_eq!(tag_handles.len(), 1);

    let my_tag = tag_handles[0].get().unwrap();
    assert_eq!(my_tag.name(), "My Tag");
    assert_eq!(my_tag.assets().len(), 1);

    let tagged_assets = registry
        .all_handles_of_filtered::<TestAsset>(AssetFilter::new().with_tag(my_tag.id().clone()))
        .unwrap();
    assert_eq!(tagged_assets.len(), 1);
    assert_eq!(tagged_assets[0].get().unwrap().name, "Test Asset");

    let new_asset_id = registry
        .add_asset::<TestAsset>(
            local_bundle_id,
            "added_asset.toml",
            Arc::new(TestAsset {
                name: "Added Asset".to_string(),
                value: 999,
            }),
        )
        .unwrap();

    let new_asset_handle = registry.handle::<TestAsset>(new_asset_id).unwrap();
    assert_eq!(
        new_asset_handle.get().unwrap().as_ref(),
        &TestAsset {
            name: "Added Asset".to_string(),
            value: 999,
        }
    );

    new_asset_handle
        .update(TestAsset {
            name: "Added Asset".to_string(),
            value: 1000,
        })
        .unwrap();
    assert_eq!(
        new_asset_handle.get().unwrap().as_ref(),
        &TestAsset {
            name: "Added Asset".to_string(),
            value: 1000,
        }
    );

    new_asset_handle.write().unwrap();
    let new_asset_meta = new_asset_handle.metadata().unwrap();
    assert_eq!(new_asset_meta.revision, 1);
    assert!(!new_asset_meta.in_memory);
    let new_asset_written_file = modified_bundle_absolute_path(&assets_root, &local_bundle_id)
        .join(&new_asset_meta.relative_path);
    assert!(new_asset_written_file.exists());
    assert_eq!(
        parse_test_asset(&new_asset_written_file),
        TestAsset {
            name: "Added Asset".to_string(),
            value: 1000,
        }
    );

    drop(registry);

    fs::remove_file(&local_test_hello_path).unwrap();
    remove_path_if_exists(&local_modified_dir).unwrap();

    let mut offline_tag: Tag =
        toml::from_str(&fs::read_to_string(&local_tag_path).unwrap()).unwrap();
    let test_bundle_asset_id = parse_id("5a4d778c-08fa-445b-af22-a13afca8e492");
    offline_tag.add_asset(test_bundle_asset_id);

    let mut tag_content = Vec::new();
    TagSerializer.write(&offline_tag, &mut tag_content).unwrap();
    fs::write(&local_tag_path, tag_content).unwrap();

    let mut serializers = AssetSerializerRegistryBuilder::default();
    serializers.add_serializer::<TestAssetSerializer>();
    serializers.add_serializer::<TagSerializer>();

    let mut restarted_registry =
        AssetRegistry::new(&assets_root, serializers.consume_and_build().into()).unwrap();

    restarted_registry
        .add_bundle(StandardAssetBundle::new(&bundle_path).unwrap())
        .unwrap();
    restarted_registry
        .add_bundle(AssetDirectory::new(&local_assets_root))
        .unwrap();

    let restarted_test_assets = restarted_registry.all_handles_of::<TestAsset>().unwrap();
    assert_eq!(restarted_test_assets.len(), 3);

    let mut restarted_by_name = HashMap::new();
    for handle in &restarted_test_assets {
        let asset = handle.get().unwrap();
        restarted_by_name.insert(asset.name.clone(), asset.value);
    }

    assert_eq!(restarted_by_name.get("Test Asset"), Some(&42));
    assert_eq!(restarted_by_name.get("Test Asset 2"), Some(&84));
    assert_eq!(restarted_by_name.get("Hello World"), None);
    assert_eq!(restarted_by_name.get("Added Asset"), Some(&999));

    let restarted_tag_handles = restarted_registry.all_handles_of::<Tag>().unwrap();
    assert_eq!(restarted_tag_handles.len(), 1);
    let restarted_tag = restarted_tag_handles[0].get().unwrap();
    assert_eq!(restarted_tag.name(), "My Tag");
    assert_eq!(restarted_tag.assets().len(), 2);
    assert!(
        restarted_tag
            .assets()
            .contains(&parse_id("0d0b1f51-0b37-3fc3-8f74-7408e8d80790"))
    );
    assert!(restarted_tag.assets().contains(&test_bundle_asset_id));

    let restarted_tagged_assets = restarted_registry
        .all_handles_of_filtered::<TestAsset>(
            AssetFilter::new().with_tag(restarted_tag.id().clone()),
        )
        .unwrap();
    assert_eq!(restarted_tagged_assets.len(), 2);

    let mut names = restarted_tagged_assets
        .iter()
        .map(|h| h.get().unwrap().name.clone())
        .collect::<Vec<_>>();
    names.sort();
    assert_eq!(
        names,
        vec!["Test Asset".to_string(), "Test Asset 2".to_string()]
    );

    let restarted_new_asset_handle = restarted_registry
        .handle::<TestAsset>(new_asset_id)
        .unwrap();
    assert_eq!(
        restarted_new_asset_handle.get().unwrap().as_ref(),
        &TestAsset {
            name: "Added Asset".to_string(),
            value: 999,
        }
    );
}
