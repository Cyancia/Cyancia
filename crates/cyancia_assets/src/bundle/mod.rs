use std::{
    collections::HashMap,
    error::Error,
    fs::{File, create_dir_all, metadata},
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::{DateTime, Utc};
use cyancia_utils::wrapper;
use parking_lot::RwLock;
use parse_display::Display;
use path_clean::PathClean;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    asset::{AssetMetadata, ErasedAsset, UntypedAssetId},
    error::{AssetError, AssetResult},
    loader::{AssetSerializerRegistry, ErasedAssetSerializer},
};

pub mod directory;
pub mod standard;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize, Deserialize)]
    #[display("{0}")]
    pub BundleId: Uuid
}

impl rusqlite::types::FromSql for BundleId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(Uuid::column_result(value)?))
    }
}

impl rusqlite::types::ToSql for BundleId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[derive(Serialize, Deserialize)]
pub struct AssetBundleMetadata {
    pub bundle_id: BundleId,
    pub name: String,
    pub last_modified: DateTime<Utc>,
}

pub struct AssetBundleCache {
    assets_root: PathBuf,
    metadata: AssetBundleMetadata,
    bundle: Arc<dyn ErasedAssetBundle>,

    id_to_original_path: RwLock<HashMap<UntypedAssetId, PathBuf>>,
    id_to_path: RwLock<HashMap<UntypedAssetId, PathBuf>>,
    assets: RwLock<HashMap<UntypedAssetId, Arc<dyn ErasedAsset>>>,

    serializers: Arc<AssetSerializerRegistry>,
}

impl AssetBundleCache {
    pub fn new(
        assets_root: impl AsRef<Path>,
        bundle: Arc<dyn ErasedAssetBundle>,
        manifest: HashMap<UntypedAssetId, PathBuf>,
        serializers: Arc<AssetSerializerRegistry>,
    ) -> AssetResult<Self> {
        Ok(Self {
            assets_root: assets_root.as_ref().to_path_buf(),
            metadata: bundle.metadata().map_err(AssetError::BundleError)?,

            id_to_original_path: bundle.manifest().map_err(AssetError::BundleError)?.into(),
            id_to_path: manifest.into(),
            bundle,
            assets: Default::default(),

            serializers,
        })
    }

    pub fn get_cached(&self, id: &UntypedAssetId) -> AssetResult<Arc<dyn ErasedAsset>> {
        let assets = self.assets.read();
        Ok(assets
            .get(id)
            .cloned()
            .ok_or_else(|| AssetError::AssetNotFound(*id))?)
    }

    pub fn update(&self, id: UntypedAssetId, asset: Arc<dyn ErasedAsset>) -> AssetResult<()> {
        self.assets.write().insert(id, asset);
        Ok(())
    }

    pub fn write(&self, id: &UntypedAssetId, revision: u32) -> AssetResult<PathBuf> {
        let assets = self.assets.read();
        let asset = assets
            .get(id)
            .ok_or_else(|| AssetError::AssetNotFound(*id))?;
        let id_to_original_path = self.id_to_original_path.read();
        let path = id_to_original_path
            .get(id)
            .ok_or_else(|| AssetError::AssetPathNotFound(*id))?;
        let serializer = self.serializers.get_for_path(path)?;

        write_modified_asset(
            &self.assets_root,
            path,
            &self.metadata.bundle_id,
            asset.as_ref(),
            revision,
            serializer.as_ref(),
        )
    }

    pub fn is_readonly(&self) -> bool {
        self.bundle.is_readonly()
    }

    pub fn add(
        &self,
        path: impl AsRef<Path>,
        asset: Arc<dyn ErasedAsset>,
    ) -> AssetResult<UntypedAssetId> {
        let path = path.as_ref().clean();
        let serializer = self.serializers.get_for_path(&path)?;
        let id = self
            .bundle
            .add(&path, asset.as_ref(), serializer.as_ref())
            .map_err(AssetError::BundleError)?;

        self.id_to_path.write().insert(id, path.clone());
        self.id_to_original_path.write().insert(id, path.clone());
        self.assets.write().insert(id, asset);

        Ok(id)
    }

    pub fn metadata(&self) -> &AssetBundleMetadata {
        &self.metadata
    }

    pub fn read(&self, id: UntypedAssetId, revision: u32) -> AssetResult<Arc<dyn ErasedAsset>> {
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
                &self.metadata.bundle_id,
                serializer.as_ref(),
            )?
        };

        self.assets.write().insert(id, asset.clone());
        Ok(asset)
    }

    pub fn absolute_modified_path(&self, path: impl AsRef<Path>) -> PathBuf {
        modified_bundle_absolute_path(&self.assets_root, &self.metadata.bundle_id).join(path)
    }
}

pub fn scan_bundle_assets(
    assets_root: impl AsRef<Path>,
    bundle: &dyn ErasedAssetBundle,
    serializers: &AssetSerializerRegistry,
) -> AssetResult<Vec<AssetMetadata>> {
    let bundle_meta = bundle.metadata().map_err(AssetError::BundleError)?;
    let manifest = bundle.manifest().map_err(AssetError::BundleError)?;
    let mut assets = Vec::with_capacity(manifest.len());

    for (id, path) in &manifest {
        let Ok(serializer) = serializers.get_for_path(path) else {
            log::warn!(
                "Skipped loading asset at {} because of unknown extension.",
                path.display(),
            );
            continue;
        };
        assets.push(AssetMetadata {
            asset_id: *id,
            ty: serializer.asset_type_name().to_string(),
            bundle_id: bundle_meta.bundle_id,
            relative_path: path.to_string_lossy().to_string(),
            revision: 0,
            // TODO: This isn't accurate, but... probably good enough for now?
            last_modified: bundle_meta.last_modified,
            in_memory: false,
        });
    }

    let modified = scan_modified_assets(
        assets_root.as_ref(),
        &bundle_meta.bundle_id,
        &manifest,
        serializers,
    )?;
    assets.extend(modified);

    Ok(assets)
}

fn scan_modified_assets(
    assets_root: &Path,
    bundle_id: &BundleId,
    bundle_manifest: &HashMap<UntypedAssetId, PathBuf>,
    serializers: &AssetSerializerRegistry,
) -> AssetResult<Vec<AssetMetadata>> {
    let mut assets = Vec::new();
    let modified_bundle_path = modified_bundle_absolute_path(assets_root, bundle_id);
    if !modified_bundle_path.exists() {
        return Ok(assets);
    }

    let reversed_manifest = bundle_manifest
        .iter()
        .map(|(id, path)| (path.clone(), *id))
        .collect::<HashMap<_, _>>();

    scan_modified_assets_dfs(
        &modified_bundle_path,
        bundle_id,
        &modified_bundle_path,
        &mut assets,
        serializers,
        &reversed_manifest,
    )?;

    Ok(assets)
}

fn scan_modified_assets_dfs(
    modified_bundle_path: &Path,
    bundle_id: &BundleId,
    current_path: &Path,
    assets: &mut Vec<AssetMetadata>,
    serializers: &AssetSerializerRegistry,
    path_to_id: &HashMap<PathBuf, UntypedAssetId>,
) -> AssetResult<()> {
    for entry in current_path.read_dir()? {
        let Ok(entry) = entry else {
            continue;
        };

        let path = entry.path();
        if path.is_dir() {
            let _ = scan_modified_assets_dfs(
                modified_bundle_path,
                bundle_id,
                &path,
                assets,
                serializers,
                path_to_id,
            );
        } else if path.is_file() {
            let Some(revision) = parse_revision_from_path(&path) else {
                log::warn!(
                    "Skipped loading modified asset at {} because it does not have a revision.",
                    path.display(),
                );
                continue;
            };

            let Ok(serializer) = serializers.get_for_path(&path) else {
                log::warn!(
                    "Skipped loading modified asset at {} because of unknown extension.",
                    path.display(),
                );
                continue;
            };
            let modified_path = path
                .strip_prefix(modified_bundle_path)
                .unwrap()
                .to_string_lossy()
                .to_string();

            let Some(original_path) = restore_original_path(Path::new(&modified_path)) else {
                continue;
            };
            let Some(asset_id) = path_to_id.get(&original_path) else {
                continue;
            };

            assets.push(AssetMetadata {
                asset_id: *asset_id,
                ty: serializer.asset_type_name().to_string(),
                bundle_id: *bundle_id,
                relative_path: modified_path,
                revision,
                last_modified: metadata(&path)?.modified()?.into(),
                in_memory: false,
            });
        }
    }

    Ok(())
}

fn parse_revision_from_path(path: &Path) -> Option<u32> {
    let filename = path.file_stem()?.to_str()?;
    let parts = filename.split(".rev").collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    parts[1].split('.').next()?.parse().ok()
}

fn restore_original_path(modified_path: &Path) -> Option<PathBuf> {
    let filename = modified_path.file_stem()?.to_str()?;
    let parts = filename.split(".rev").collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    let original_filename = format!(
        "{}.{}",
        parts[0],
        modified_path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
    );
    Some(modified_path.with_file_name(original_filename))
}

fn read_modified_asset(
    assets_root: &Path,
    asset_relative_path: &Path,
    bundle_id: &BundleId,
    serializer: &dyn ErasedAssetSerializer,
) -> AssetResult<Arc<dyn ErasedAsset>> {
    let path = modified_bundle_absolute_path(assets_root, bundle_id).join(asset_relative_path);
    let mut file = File::open(path)?;
    Ok(serializer
        .read(&mut file)
        .map_err(AssetError::SerializerError)
        .map(Into::into)?)
}

fn write_modified_asset(
    assets_root: &Path,
    original_relative_path: &Path,
    bundle_id: &BundleId,
    asset: &dyn ErasedAsset,
    revision: u32,
    serializer: &dyn ErasedAssetSerializer,
) -> AssetResult<PathBuf> {
    let new_relative_path = modified_asset_relative_path(original_relative_path, revision);
    let modified_bundle_path = modified_bundle_absolute_path(assets_root, bundle_id);
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

pub fn modified_bundle_absolute_path(
    assets_root: impl AsRef<Path>,
    bundle_id: &BundleId,
) -> PathBuf {
    assets_root
        .as_ref()
        .join(format!("{}.modified", bundle_id.0))
}

pub trait AssetBundle: Send + Sync + 'static {
    const READONLY: bool;
    type Error: Error + Sync + Send + 'static;

    fn metadata(&self) -> Result<AssetBundleMetadata, Self::Error>;
    fn manifest(&self) -> Result<HashMap<UntypedAssetId, PathBuf>, Self::Error>;
    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Self::Error>;
    fn add(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, Self::Error>;
}

pub trait ErasedAssetBundle: Send + Sync + 'static {
    fn is_readonly(&self) -> bool;
    fn metadata(&self) -> Result<AssetBundleMetadata, Box<dyn Error + Send + Sync + 'static>>;
    fn manifest(
        &self,
    ) -> Result<HashMap<UntypedAssetId, PathBuf>, Box<dyn Error + Send + Sync + 'static>>;
    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Box<dyn Error + Send + Sync + 'static>>;
    fn add(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, Box<dyn Error + Send + Sync + 'static>>;
}

impl<T: AssetBundle> ErasedAssetBundle for T {
    fn is_readonly(&self) -> bool {
        T::READONLY
    }

    fn metadata(&self) -> Result<AssetBundleMetadata, Box<dyn Error + Send + Sync + 'static>> {
        self.metadata().map_err(Into::into)
    }

    fn manifest(
        &self,
    ) -> Result<HashMap<UntypedAssetId, PathBuf>, Box<dyn Error + Send + Sync + 'static>> {
        self.manifest().map_err(Into::into)
    }

    fn read(
        &self,
        path: &Path,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<Arc<dyn ErasedAsset>, Box<dyn Error + Send + Sync + 'static>> {
        self.read(path, serializer).map_err(Into::into)
    }

    fn add(
        &self,
        path: &Path,
        asset: &dyn ErasedAsset,
        serializer: &dyn ErasedAssetSerializer,
    ) -> Result<UntypedAssetId, Box<dyn Error + Send + Sync + 'static>> {
        self.add(path, asset, serializer).map_err(Into::into)
    }
}
