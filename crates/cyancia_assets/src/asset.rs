use std::{
    any::TypeId,
    marker::PhantomData,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use atomicow::CowArc;
use downcast_rs::{Downcast, DowncastSync};
use sqlx::{
    prelude::FromRow,
    types::{
        Uuid,
        chrono::{DateTime, Utc},
    },
};

use crate::{
    bundle::{AssetBundle, BundleId, CachedAssetBundle},
    id::{AssetId, UntypedAssetId},
    index_db::AssetIndexDb,
};

pub trait Asset: Send + Sync + 'static + DowncastSync {
    const TYPE_NAME: &'static str;
    fn hash(&self) -> String;
}

pub trait ErasedAsset: Send + Sync + 'static + DowncastSync {
    fn type_name(&self) -> &'static str;
    fn hash(&self) -> String;
}

downcast_rs::impl_downcast!(sync ErasedAsset);

impl<T: Asset> ErasedAsset for T {
    fn type_name(&self) -> &'static str {
        T::TYPE_NAME
    }

    fn hash(&self) -> String {
        self.hash()
    }
}

#[derive(FromRow)]
pub struct AssetMetadata {
    pub bundle_id: BundleId,
    // TODO: Replace with Arc<str> when sqlx supports.
    pub asset_type: String,
    pub relative_path: String,
    pub content_hash: String,
    pub updated_at: DateTime<Utc>,
}

pub struct AssetUrl<'a> {
    source: BundleId,
    path: CowArc<'a, str>,
}

impl<'a> AssetUrl<'a> {
    pub(crate) fn new(source: BundleId, path: CowArc<'a, str>) -> Self {
        Self { source, path }
    }

    pub fn try_parse(path: &'a str) -> Option<Self> {
        let (source, path) = path.split_once(':')?;
        Some(Self {
            source: BundleId::new(source.to_string()),
            path: CowArc::Borrowed(path),
        })
    }

    pub fn source(&self) -> &BundleId {
        &self.source
    }

    pub fn path_str(&self) -> &str {
        &self.path
    }

    pub fn path(&'a self) -> &'a Path {
        Path::new(self.path.as_ref())
    }
}

pub struct AssetHandle<T: Asset> {
    url: AssetUrl<'static>,
    bundle: Arc<CachedAssetBundle>,
    index_db: Arc<AssetIndexDb>,
    _marker: PhantomData<T>,
}

impl<T: Asset> AssetHandle<T> {
    pub(crate) fn new(
        url: AssetUrl<'static>,
        bundle: Arc<CachedAssetBundle>,
        index_db: Arc<AssetIndexDb>,
    ) -> Self {
        Self {
            url,
            bundle,
            index_db,
            _marker: PhantomData,
        }
    }

    pub fn url(&self) -> &AssetUrl<'_> {
        &self.url
    }

    pub fn bundle(&self) -> &CachedAssetBundle {
        self.bundle.as_ref()
    }

    pub fn read(&self) -> Option<Arc<T>> {
        self.bundle
            .read_by_path(self.url.path_str())?
            .downcast_arc()
            .ok()
    }

    pub fn update(&self, asset: T) {
        self.bundle
            .update_by_path(self.url.path_str().to_string(), Arc::new(asset));
    }

    pub async fn write(&self) {
        let Some(asset) = self.read() else {
            return;
        };

        let _ = self.index_db.update_by_url(&self.url, asset.hash()).await;
        self.bundle.write_by_path(self.url.path_str())
    }

    pub async fn metadata(&self) -> Option<AssetMetadata> {
        self.index_db.get_by_url(&self.url).await.ok()?
    }
}

pub struct UntypedAssetHandle {
    url: AssetUrl<'static>,
    bundle: Arc<CachedAssetBundle>,
    index_db: Arc<AssetIndexDb>,
    ty: TypeId,
}

impl UntypedAssetHandle {
    pub fn url(&self) -> &AssetUrl<'static> {
        &self.url
    }

    pub fn bundle(&self) -> &CachedAssetBundle {
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

    pub fn read(&self) -> Option<Arc<dyn ErasedAsset>> {
        self.bundle.read_by_path(self.url.path_str())
    }

    pub fn update(&self, asset: Arc<dyn ErasedAsset>) {
        self.bundle
            .update_by_path(self.url.path_str().to_string(), asset);
    }

    pub fn write(&self) {
        self.bundle.write_by_path(self.url.path_str())
    }
}
