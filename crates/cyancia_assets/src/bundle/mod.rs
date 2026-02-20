use std::{
    collections::HashMap,
    error::Error,
    fs::{File, create_dir_all},
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use atomicow::CowArc;
use cyancia_utils::wrapper;
use parking_lot::{RwLock, RwLockReadGuard};
use serde::{Deserialize, Serialize};
use sqlx::{
    Decode, Encode, Sqlite,
    prelude::{FromRow, Type},
    types::Uuid,
};

use crate::{
    asset::{Asset, AssetId, AssetMetadata, ErasedAsset},
    error::{AssetError, AssetResult},
    index_db::AssetIndexDb,
    loader::{AssetSerializerRegistry, ErasedAssetSerializer},
};

pub mod directory;
pub mod standard;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type, Serialize, Deserialize)]
    #[sqlx(transparent)]
    pub BundleId: Uuid
}

#[derive(FromRow, Serialize, Deserialize)]
pub struct AssetBundleMetadata {
    pub bundle_id: BundleId,
    pub name: String,
}

pub struct AssetBundleCache {
    assets_root: PathBuf,
    filename: String,
    metadata: AssetBundleMetadata,
    bundle: Arc<dyn ErasedAssetBundle>,

    id_to_path: RwLock<HashMap<AssetId, PathBuf>>,
    assets: RwLock<HashMap<AssetId, Arc<dyn ErasedAsset>>>,

    serializers: Arc<AssetSerializerRegistry>,
}

impl AssetBundleCache {
    pub fn new(
        assets_root: impl AsRef<Path>,
        filename: String,
        bundle: Arc<dyn ErasedAssetBundle>,
        serializers: Arc<AssetSerializerRegistry>,
    ) -> AssetResult<(Self, Vec<AssetMetadata>)> {
        let bundle_meta = bundle.metadata().map_err(AssetError::BundleError)?;
        let mut manifest = bundle.manifest().map_err(AssetError::BundleError)?;

        let mut metadata = HashMap::with_capacity(manifest.len());
        for (id, path) in &manifest {
            let Ok(serializer) = serializers.get_for_path(path) else {
                continue;
            };
            metadata.insert(
                *id,
                AssetMetadata {
                    asset_id: *id,
                    ty: serializer.asset_type_name().to_string(),
                    bundle_id: bundle_meta.bundle_id,
                    relative_path: path.to_string_lossy().to_string(),
                    revision: 0,
                    in_memory: false,
                },
            );
        }

        // TODO: Manifest might be outdated, so probably scan the entire directory?
        let modified_manifest = read_modified_manifest(assets_root.as_ref(), &filename)?;
        manifest.reserve(modified_manifest.len());
        for (id, path) in &modified_manifest {
            let Some(revision) = parse_revision_from_path(path) else {
                // TODO
                continue;
            };

            let Ok(serializer) = serializers.get_for_path(path) else {
                continue;
            };

            metadata.insert(
                *id,
                AssetMetadata {
                    asset_id: *id,
                    ty: serializer.asset_type_name().to_string(),
                    bundle_id: bundle_meta.bundle_id,
                    relative_path: path.to_string_lossy().to_string(),
                    revision,
                    in_memory: false,
                },
            );
        }

        Ok((
            Self {
                assets_root: assets_root.as_ref().to_path_buf(),
                filename,
                metadata: bundle.metadata().map_err(AssetError::BundleError)?,
                bundle,

                id_to_path: manifest.into(),
                assets: Default::default(),

                serializers,
            },
            metadata.into_values().collect(),
        ))
    }

    pub fn get_cached(&self, id: &AssetId) -> AssetResult<Arc<dyn ErasedAsset>> {
        let assets = self.assets.read();
        assets
            .get(id)
            .cloned()
            .ok_or_else(|| AssetError::AssetNotFound(*id))
    }

    pub fn update(&self, id: AssetId, asset: Arc<dyn ErasedAsset>) -> AssetResult<()> {
        self.assets.write().insert(id, asset);
        Ok(())
    }

    pub fn write(&self, id: &AssetId, revision: u32) -> AssetResult<PathBuf> {
        let assets = self.assets.read();
        let asset = assets
            .get(id)
            .ok_or_else(|| AssetError::AssetNotFound(*id))?;
        let id_to_path = self.id_to_path.read();
        let path = id_to_path
            .get(id)
            .ok_or_else(|| AssetError::AssetPathNotFound(*id))?;
        let serializer = self.serializers.get_for_path(path)?;

        write_modified_asset(
            &self.assets_root,
            path,
            &self.filename,
            asset.as_ref(),
            revision,
            serializer.as_ref(),
        )
    }

    pub fn metadata(&self) -> &AssetBundleMetadata {
        &self.metadata
    }

    pub async fn read(&self, id: AssetId, revision: u32) -> AssetResult<Arc<dyn ErasedAsset>> {
        let id_to_path = self.id_to_path.read();
        let path = id_to_path
            .get(&id)
            .ok_or_else(|| AssetError::AssetPathNotFound(id))?;
        let serializer = self.serializers.get_for_path(path)?;

        let asset = if revision == 0 {
            self.bundle
                .read(path, serializer.as_ref())
                .map_err(AssetError::BundleError)?
        } else {
            read_modified_asset(
                &self.assets_root,
                path,
                &self.filename,
                serializer.as_ref(),
                revision,
            )?
        };

        self.assets.write().insert(id, asset.clone());
        Ok(asset)
    }
}

fn read_modified_manifest(
    assets_root: &Path,
    bundle_filename: &str,
) -> AssetResult<HashMap<AssetId, PathBuf>> {
    let path = modified_bundle_absolute_path(assets_root, bundle_filename).join("manifest.toml");
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let mut buf = String::new();
    File::open(path)?.read_to_string(&mut buf)?;
    let manifest = toml::from_str(&buf).map_err(AssetError::TomlDeError)?;
    Ok(manifest)
}

fn parse_revision_from_path(path: &Path) -> Option<u32> {
    let filename = path.file_stem()?.to_str()?;
    let parts: Vec<&str> = filename.split(".rev").collect();
    if parts.len() != 2 {
        return None;
    }
    parts[1].split('.').next()?.parse().ok()
}

fn read_modified_asset(
    assets_root: &Path,
    asset_relative_path: &Path,
    bundle_filename: &str,
    serializer: &dyn ErasedAssetSerializer,
    revision: u32,
) -> AssetResult<Arc<dyn ErasedAsset>> {
    let path = modified_bundle_absolute_path(assets_root, bundle_filename)
        .join(modified_asset_relative_path(asset_relative_path, revision));
    let mut file = File::open(path)?;
    serializer
        .read(&mut file)
        .map_err(AssetError::SerializerError)
        .map(Into::into)
}

fn write_modified_asset(
    assets_root: &Path,
    asset_relative_path: &Path,
    bundle_filename: &str,
    asset: &dyn ErasedAsset,
    revision: u32,
    serializer: &dyn ErasedAssetSerializer,
) -> AssetResult<PathBuf> {
    let new_relative_path = modified_asset_relative_path(asset_relative_path, revision);
    let modified_bundle_path = modified_bundle_absolute_path(assets_root, bundle_filename);
    let modified_asset_path = modified_bundle_path.join(&new_relative_path);

    if let Some(dir) = &modified_asset_path.parent() {
        create_dir_all(dir)?;
    }
    let mut file = File::create(modified_asset_path)?;
    serializer
        .write(asset, &mut file)
        .map_err(AssetError::SerializerError)?;

    Ok(new_relative_path)
}

fn modified_asset_relative_path(asset_relative_path: &Path, revision: u32) -> PathBuf {
    let ext = asset_relative_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let file_stem = asset_relative_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    asset_relative_path.with_file_name(format!("{}.rev{}.{}", file_stem, revision, ext))
}

fn modified_bundle_absolute_path(assets_root: &Path, bundle_filename: &str) -> PathBuf {
    assets_root.join(format!("{}.modified", bundle_filename))
}

pub trait AssetBundle: Send + Sync + 'static {
    type Error: Error + Sync + Send + 'static;

    fn metadata(&self) -> Result<AssetBundleMetadata, Self::Error>;
    fn manifest(&self) -> Result<HashMap<AssetId, PathBuf>, Self::Error>;
    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Self::Error>;
}

pub trait ErasedAssetBundle: Send + Sync + 'static {
    fn metadata(&self) -> Result<AssetBundleMetadata, Box<dyn Error + Send + Sync + 'static>>;
    fn manifest(&self)
    -> Result<HashMap<AssetId, PathBuf>, Box<dyn Error + Send + Sync + 'static>>;
    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Box<dyn Error + Send + Sync + 'static>>;
}

impl<T: AssetBundle> ErasedAssetBundle for T {
    fn metadata(&self) -> Result<AssetBundleMetadata, Box<dyn Error + Send + Sync + 'static>> {
        self.metadata().map_err(Into::into)
    }

    fn manifest(
        &self,
    ) -> Result<HashMap<AssetId, PathBuf>, Box<dyn Error + Send + Sync + 'static>> {
        self.manifest().map_err(Into::into)
    }

    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Box<dyn Error + Send + Sync + 'static>> {
        self.read(path, serializer).map_err(Into::into)
    }
}
