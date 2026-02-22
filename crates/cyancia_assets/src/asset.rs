use std::{
    any::TypeId,
    fmt::Display,
    marker::PhantomData,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use cyancia_utils::wrapper;
use downcast_rs::{Downcast, DowncastSync};
use parse_display::Display;
use serde::{Deserialize, Serialize};
use sqlx::{
    prelude::{FromRow, Type},
    types::{
        Uuid,
        chrono::{DateTime, Utc},
    },
};

use crate::{
    bundle::{AssetBundle, AssetBundleCache, BundleId},
    error::{AssetError, AssetResult},
    index_db::AssetIndexDb,
};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Type, Serialize, Deserialize, Display)]
    #[sqlx(transparent)]
    #[display("{0}")]
    pub AssetId: Uuid
}

pub trait Asset: Send + Sync + 'static + DowncastSync {
    const TYPE_NAME: &'static str;
}

pub trait ErasedAsset: Send + Sync + 'static + DowncastSync {
    fn type_name(&self) -> &'static str;
}

downcast_rs::impl_downcast!(sync ErasedAsset);

impl<T: Asset> ErasedAsset for T {
    fn type_name(&self) -> &'static str {
        T::TYPE_NAME
    }
}

#[derive(FromRow, Debug, Clone)]
pub struct AssetMetadata {
    pub asset_id: AssetId,
    // TODO: Replace with Arc<str> when sqlx supports.
    pub ty: String,
    pub bundle_id: BundleId,
    pub relative_path: String,
    pub revision: u32,
    pub last_modified: DateTime<Utc>,
    pub in_memory: bool,
}

pub struct AssetHandle<T: Asset> {
    id: AssetId,
    bundle: Arc<AssetBundleCache>,
    index_db: Arc<AssetIndexDb>,
    _marker: PhantomData<T>,
}

impl<T: Asset> AssetHandle<T> {
    pub(crate) fn new(
        id: AssetId,
        bundle: Arc<AssetBundleCache>,
        index_db: Arc<AssetIndexDb>,
    ) -> Self {
        Self {
            id,
            bundle,
            index_db,
            _marker: PhantomData,
        }
    }

    pub fn id(&self) -> AssetId {
        self.id
    }

    pub fn bundle(&self) -> &AssetBundleCache {
        self.bundle.as_ref()
    }

    pub async fn get(&self) -> AssetResult<Arc<T>> {
        let dynamic = match self.bundle.get_cached(&self.id) {
            Ok(cached) => cached,
            Err(_) => {
                let metadata = self.metadata().await?;
                self.bundle.read(self.id, metadata.revision).await?
            }
        };

        Ok(dynamic
            .downcast_arc::<T>()
            .map_err(|_| AssetError::CastAssetError(T::TYPE_NAME.to_string()))?)
    }

    pub async fn update(&self, asset: T) -> AssetResult<()> {
        self.bundle.update(self.id, Arc::new(asset))?;
        self.index_db.update_asset(&self.id).await?;

        Ok(())
    }

    pub async fn write(&self) -> AssetResult<()> {
        let metadata = self.metadata().await?;
        let new_path = self.bundle.write(&self.id, metadata.revision)?;
        let last_modified =
            std::fs::metadata(self.bundle.absolute_modified_path(&new_path))?.modified()?;
        self.index_db
            .write_asset(&self.id, new_path.to_str().unwrap(), last_modified.into())
            .await?;
        Ok(())
    }

    pub async fn metadata(&self) -> AssetResult<AssetMetadata> {
        Ok(self.index_db.get_asset(&self.id).await?)
    }
}
