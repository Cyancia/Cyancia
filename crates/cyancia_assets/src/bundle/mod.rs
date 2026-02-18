use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use atomicow::CowArc;
use cyancia_utils::wrapper;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Sqlite, prelude::Type, types::Uuid};

use crate::{
    asset::{Asset, AssetMetadata, ErasedAsset, UntypedAssetHandle},
    id::UntypedAssetId,
    index_db::AssetIndexDb,
    loader::AssetSerializerRegistry,
};

pub mod data_directory;
pub mod standard;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type, Serialize, Deserialize)]
    #[sqlx(transparent)]
    pub BundleId: Uuid
}

pub struct BundleMetadata {
    pub bundle_id: BundleId,
    pub filename: String,
    pub content_hash: String,
    pub readonly: bool,
}

pub struct AssetBundleCache {
    assets_root: PathBuf,
    metadata: BundleMetadata,
    cached_asset: RwLock<HashMap<String, Arc<dyn ErasedAsset>>>,
    serializers: Arc<AssetSerializerRegistry>,
    bundle: Arc<dyn ErasedAssetBundle>,
}

impl AssetBundleCache {
    pub fn new(
        asset_root: impl AsRef<Path>,
        metadata: BundleMetadata,
        bundle: Arc<dyn ErasedAssetBundle>,
        serializers: Arc<AssetSerializerRegistry>,
    ) -> Result<Self> {
        let mut assets = read_modified(asset_root.as_ref(), &metadata.filename, &serializers)?;
        assets.extend(bundle.read(serializers.as_ref())?);

        Ok(Self {
            assets_root: asset_root.as_ref().to_path_buf(),
            metadata,
            cached_asset: RwLock::new(assets),
            serializers,
            bundle,
        })
    }

    pub fn metadata(&self) -> &BundleMetadata {
        &self.metadata
    }

    pub fn read(&self, path: &str) -> Result<Arc<dyn ErasedAsset>> {
        self.cached_asset
            .read()
            .get(path)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Asset not found in cache: {}", path))
    }

    pub fn update(&self, path: String, asset: Arc<dyn ErasedAsset>) -> Result<()> {
        self.cached_asset.write().insert(path, asset);
        Ok(())
    }

    pub fn write(&self, path: &str) -> Result<()> {
        let cache = self.cached_asset.read();
        let asset = cache
            .get(path)
            .ok_or_else(|| anyhow::anyhow!("Asset not found in cache: {}", path))?;

        if self.bundle.is_read_only() {
            write_modified(
                &self.assets_root,
                Path::new(path),
                &self.metadata.filename,
                asset.as_ref(),
                self.serializers.as_ref(),
            )?;
        } else {
            self.bundle.write(path, asset.as_ref(), &self.serializers)?;
        }

        Ok(())
    }
}

fn read_modified(
    assets_root: &Path,
    bundle_filename: &str,
    serializers: &AssetSerializerRegistry,
) -> Result<HashMap<String, Arc<dyn ErasedAsset>>> {
    let mut assets = HashMap::new();
    read_modified_dfs(
        &assets_root.join(bundle_filename),
        serializers,
        assets_root,
        &mut assets,
    )?;
    Ok(assets)
}

fn read_modified_dfs(
    bundle_root: &Path,
    serializers: &AssetSerializerRegistry,
    current_path: &Path,
    assets: &mut HashMap<String, Arc<dyn ErasedAsset>>,
) -> Result<()> {
    let entries = std::fs::read_dir(bundle_root)?;

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_dir() {
            read_modified_dfs(&path, serializers, current_path, assets)?;
        } else if path.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .ok_or_else(|| anyhow::anyhow!("Missing extension for path: {}", path.display()))?;
            let mut file = File::open(&path)?;
            let serializer = serializers
                .get(ext)
                .ok_or_else(|| anyhow::anyhow!("Missing serializer for extension: {}", ext))?;
            let asset = serializer.read(&mut file).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read asset from file for path: {}, error: {}",
                    path.display(),
                    e
                )
            })?;

            let relative_path = path
                .strip_prefix(bundle_root)?
                .to_str()
                .unwrap()
                .to_string();
            assets.insert(relative_path, asset.into());
        }
    }

    Ok(())
}

fn write_modified(
    assets_root: &Path,
    asset_path: &Path,
    bundle_filename: &str,
    asset: &dyn ErasedAsset,
    serializers: &AssetSerializerRegistry,
) -> Result<()> {
    let asset_ext = asset_path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow::anyhow!("Missing extension for path: {}", asset_path.display()))?;
    let bundle_filename = Path::new(bundle_filename)
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    let serializer = serializers
        .get(asset_ext)
        .ok_or_else(|| anyhow::anyhow!("Missing serializer for extension: {}", asset_ext))?;
    let mut file = File::open(assets_root.join(bundle_filename))?;
    serializer.write(asset, &mut file).map_err(|e| {
        anyhow::anyhow!(
            "Failed to write asset to file for path: {}, error: {}",
            asset_path.display(),
            e
        )
    })?;

    Ok(())
}

pub trait AssetBundle: Send + Sync + 'static {
    type Error: Error + Sync + Send + 'static;

    fn id(&self) -> BundleId;
    fn hash(&self) -> String;
    fn is_read_only() -> bool;

    fn read(
        &self,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<HashMap<String, Arc<dyn ErasedAsset>>, Self::Error>;
    fn write(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<(), Self::Error>;
}

pub trait ErasedAssetBundle: Send + Sync + 'static {
    fn id(&self) -> BundleId;
    fn hash(&self) -> String;
    fn is_read_only(&self) -> bool;
    fn read(
        &self,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<HashMap<String, Arc<dyn ErasedAsset>>, anyhow::Error>;
    fn write(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<(), anyhow::Error>;
}

impl<T: AssetBundle> ErasedAssetBundle for T {
    fn id(&self) -> BundleId {
        self.id()
    }

    fn hash(&self) -> String {
        self.hash()
    }

    fn is_read_only(&self) -> bool {
        T::is_read_only()
    }

    fn read(
        &self,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<HashMap<String, Arc<dyn ErasedAsset>>, anyhow::Error> {
        self.read(serializers).map_err(Into::into)
    }

    fn write(
        &self,
        path: &str,
        asset: &dyn ErasedAsset,
        serializers: &AssetSerializerRegistry,
    ) -> std::result::Result<(), anyhow::Error> {
        self.write(path, asset, serializers).map_err(Into::into)
    }
}
