use std::{marker::PhantomData, sync::Arc};

use chrono::{DateTime, Utc};
use cyancia_utils::wrapper;
use downcast_rs::DowncastSync;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    bundle::{AssetBundleCache, BundleId},
    error::{AssetError, AssetResult},
    index_db::AssetIndexDb,
};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub AssetId: Uuid
}

impl rusqlite::types::FromSql for AssetId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(Uuid::column_result(value)?))
    }
}

impl rusqlite::types::ToSql for AssetId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
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

#[derive(Debug, Clone)]
pub struct AssetMetadata {
    pub asset_id: AssetId,
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

    pub fn get(&self) -> AssetResult<Arc<T>> {
        let dynamic = match self.bundle.get_cached(&self.id) {
            Ok(cached) => cached,
            Err(_) => {
                let metadata = self.metadata()?;
                self.bundle.read(self.id, metadata.revision)?
            }
        };

        Ok(dynamic
            .downcast_arc::<T>()
            .map_err(|_| AssetError::CastAssetError(T::TYPE_NAME.to_string()))?)
    }

    pub fn update(&self, asset: T) -> AssetResult<()> {
        self.bundle.update(self.id, Arc::new(asset))?;
        self.index_db.update_asset(&self.id)?;
        Ok(())
    }

    pub fn write(&self) -> AssetResult<()> {
        let metadata = self.metadata()?;
        let new_path = self.bundle.write(&self.id, metadata.revision)?;
        let last_modified =
            std::fs::metadata(self.bundle.absolute_modified_path(&new_path))?.modified()?;
        self.index_db
            .write_asset(&self.id, new_path.to_str().unwrap(), last_modified.into())?;
        Ok(())
    }

    pub fn metadata(&self) -> AssetResult<AssetMetadata> {
        self.index_db.get_asset(&self.id)
    }
}
