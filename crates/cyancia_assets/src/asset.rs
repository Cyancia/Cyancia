use std::{
    any::TypeId,
    marker::PhantomData,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use anyhow::Result;
use atomicow::CowArc;
use downcast_rs::{Downcast, DowncastSync};
use sqlx::{
    prelude::{FromRow, Type},
    types::{
        Uuid,
        chrono::{DateTime, Utc},
    },
};

use crate::{
    bundle::{AssetBundle, AssetBundleCache, BundleId},
    id::{AssetId, UntypedAssetId},
    index_db::AssetIndexDb,
};

pub trait Asset: Send + Sync + 'static + DowncastSync {
    const TYPE_NAME: &'static str;
    fn hash(&self) -> i64;
}

pub trait ErasedAsset: Send + Sync + 'static + DowncastSync {
    fn type_name(&self) -> &'static str;
    fn hash(&self) -> i64;
}

downcast_rs::impl_downcast!(sync ErasedAsset);

impl<T: Asset> ErasedAsset for T {
    fn type_name(&self) -> &'static str {
        T::TYPE_NAME
    }

    fn hash(&self) -> i64 {
        self.hash()
    }
}

#[derive(FromRow)]
pub struct AssetMetadata {
    pub bundle_id: BundleId,
    // TODO: Replace with Arc<str> when sqlx supports.
    pub asset_type: String,
    pub relative_path: String,
    pub content_hash: i64,
    pub updated_at: DateTime<Utc>,
    pub physical_location: AssetPhysicalLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Type)]
#[repr(u32)]
pub enum AssetPhysicalLocation {
    Memory,
    LocalModified,
    Bundle,
}

pub struct AssetUrl {
    source: BundleId,
    path: Arc<str>,
}

impl AssetUrl {
    pub fn new(source: BundleId, path: Arc<str>) -> Self {
        Self { source, path }
    }

    pub fn try_parse(path: &str) -> Option<Self> {
        let (source, path) = path.split_once(':')?;
        Some(Self {
            source: BundleId::new(Uuid::from_str(source).ok()?),
            path: Path::new(path).canonicalize().ok()?.to_str()?.into(),
        })
    }

    pub fn source(&self) -> &BundleId {
        &self.source
    }

    pub fn path_str(&self) -> &str {
        &self.path
    }

    pub fn path(&self) -> &Path {
        Path::new(self.path.as_ref())
    }
}

pub struct AssetHandle<T: Asset> {
    url: AssetUrl,
    bundle: Arc<AssetBundleCache>,
    index_db: Arc<AssetIndexDb>,
    _marker: PhantomData<T>,
}

impl<T: Asset> AssetHandle<T> {
    pub(crate) fn new(
        url: AssetUrl,
        bundle: Arc<AssetBundleCache>,
        index_db: Arc<AssetIndexDb>,
    ) -> Self {
        Self {
            url,
            bundle,
            index_db,
            _marker: PhantomData,
        }
    }

    pub fn url(&self) -> &AssetUrl {
        &self.url
    }

    pub fn bundle(&self) -> &AssetBundleCache {
        self.bundle.as_ref()
    }

    pub fn read(&self) -> Result<Arc<T>> {
        self.bundle
            .read(self.url.path_str())?
            .downcast_arc()
            .map_err(|_| anyhow::anyhow!("Failed to downcast asset"))
    }

    pub async fn update(&self, asset: T) -> Result<()> {
        let old_metadata = self.metadata().await?;
        self.index_db
            .update(&self.url, old_metadata.content_hash, asset.hash())
            .await?;
        self.bundle
            .update(self.url.path_str().to_string(), Arc::new(asset))?;

        Ok(())
    }

    pub async fn write(&self) -> Result<()> {
        self.bundle.write(&self.url.path)?;
        self.index_db.write(&self.url).await?;
        Ok(())
    }

    pub async fn metadata(&self) -> Result<AssetMetadata> {
        self.index_db.get(&self.url).await.map_err(Into::into)
    }
}

pub struct UntypedAssetHandle {
    url: AssetUrl,
    bundle: Arc<AssetBundleCache>,
    index_db: Arc<AssetIndexDb>,
    ty: TypeId,
}

impl UntypedAssetHandle {
    pub fn url(&self) -> &AssetUrl {
        &self.url
    }

    pub fn bundle(&self) -> &AssetBundleCache {
        self.bundle.as_ref()
    }

    pub fn ty(&self) -> TypeId {
        self.ty
    }

    pub fn into_typed<T: Asset>(self) -> Option<AssetHandle<T>> {
        Some(AssetHandle {
            url: self.url,
            bundle: self.bundle,
            index_db: self.index_db,
            _marker: PhantomData,
        })
    }

    pub fn read(&self) -> Result<Arc<dyn ErasedAsset>> {
        self.bundle.read(self.url.path_str())
    }

    pub async fn update(&self, asset: Arc<dyn ErasedAsset>) -> Result<()> {
        let old_metadata = self.metadata().await?;
        self.index_db
            .update(&self.url, old_metadata.content_hash, asset.hash())
            .await?;
        self.bundle.update(self.url.path_str().to_string(), asset)?;

        Ok(())
    }

    pub async fn write(&self) -> Result<()> {
        self.bundle.write(&self.url.path)?;
        self.index_db.write(&self.url).await?;
        Ok(())
    }

    pub async fn metadata(&self) -> Result<AssetMetadata> {
        self.index_db.get(&self.url).await.map_err(Into::into)
    }
}
