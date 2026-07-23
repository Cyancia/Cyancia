use std::{collections::BTreeSet, hash::Hash, marker::PhantomData, sync::Arc};

use chrono::{DateTime, Utc};
use cyancia_utils::wrapper;
use downcast_rs::DowncastSync;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    bundle::{AssetBundleCache, BundleId},
    error::{AssetErrorKind, AssetResult},
    index_db::AssetIndexDb,
    tag::TagId,
};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub UntypedAssetId: Uuid
}

impl UntypedAssetId {
    pub fn into_typed<T: Asset>(self) -> AssetId<T> {
        AssetId::new(self.0)
    }
}

impl rusqlite::types::FromSql for UntypedAssetId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(Uuid::column_result(value)?))
    }
}

impl rusqlite::types::ToSql for UntypedAssetId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[derive(Display)]
#[display("{id}")]
pub struct AssetId<T: Asset> {
    id: Uuid,
    _marker: PhantomData<T>,
}

impl<T: Asset> std::fmt::Debug for AssetId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("AssetId").field(&self.id).finish()
    }
}

impl<T: Asset> Clone for AssetId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Asset> Copy for AssetId<T> {}

impl<T: Asset> PartialEq for AssetId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Asset> Eq for AssetId<T> {}

impl<T: Asset> std::hash::Hash for AssetId<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: Asset> Serialize for AssetId<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.id.serialize(serializer)
    }
}

impl<'de, T: Asset> Deserialize<'de> for AssetId<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = Uuid::deserialize(deserializer)?;
        Ok(Self {
            id,
            _marker: PhantomData,
        })
    }
}

impl<T: Asset> std::ops::Deref for AssetId<T> {
    type Target = Uuid;

    fn deref(&self) -> &Self::Target {
        &self.id
    }
}

impl<T: Asset> AssetId<T> {
    pub fn new(id: Uuid) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }

    pub fn into_untyped(self) -> UntypedAssetId {
        UntypedAssetId::new(self.id)
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
    pub asset_id: UntypedAssetId,
    pub ty: String,
    pub bundle_id: BundleId,
    pub relative_path: String,
    pub revision: u32,
    pub last_modified: DateTime<Utc>,
    pub in_memory: bool,
}

pub struct AssetHandle<T: Asset> {
    id: AssetId<T>,
    bundle: Arc<AssetBundleCache>,
    index_db: Arc<AssetIndexDb>,
}

impl<T: Asset> Clone for AssetHandle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            bundle: Arc::clone(&self.bundle),
            index_db: Arc::clone(&self.index_db),
        }
    }
}

impl<T: Asset> PartialEq for AssetHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T: Asset> Eq for AssetHandle<T> {}

impl<T: Asset> Hash for AssetHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<T: Asset> AssetHandle<T> {
    pub(crate) fn new(
        id: AssetId<T>,
        bundle: Arc<AssetBundleCache>,
        index_db: Arc<AssetIndexDb>,
    ) -> Self {
        Self {
            id,
            bundle,
            index_db,
        }
    }

    pub fn id(&self) -> AssetId<T> {
        self.id
    }

    pub fn untyped_id(&self) -> UntypedAssetId {
        self.id.into_untyped()
    }

    pub fn bundle(&self) -> &AssetBundleCache {
        self.bundle.as_ref()
    }

    pub fn get(&self) -> AssetResult<Arc<T>> {
        let dynamic = match self.bundle.get_cached_asset(&self.untyped_id()) {
            Ok(cached) => cached,
            Err(_) => {
                let metadata = self.metadata()?;
                self.bundle
                    .read_asset(self.untyped_id(), metadata.revision)?
            }
        };

        Ok(dynamic
            .downcast_arc::<T>()
            .map_err(|_| AssetErrorKind::CastAssetError(T::TYPE_NAME.to_string()))?)
    }

    pub fn update(&self, asset: T) -> AssetResult<()> {
        self.bundle
            .update_asset(self.untyped_id(), Arc::new(asset))?;
        self.index_db.update_asset(&self.untyped_id())?;

        Ok(())
    }

    pub fn write(&self) -> AssetResult<()> {
        let metadata = self.metadata()?;
        if !metadata.in_memory {
            return Ok(());
        }
        let new_path = self
            .bundle
            .write_asset(&self.untyped_id(), metadata.revision)?;
        let last_modified =
            std::fs::metadata(self.bundle.absolute_modified_path(&new_path))?.modified()?;
        self.index_db.write_asset(
            &self.untyped_id(),
            new_path.to_str().unwrap(),
            last_modified.into(),
        )?;
        Ok(())
    }

    pub fn delete(&self) -> AssetResult<()> {
        let id = self.untyped_id();
        self.bundle.delete_cached_asset(&id)?;
        self.index_db.delete_asset(&id)?;
        Ok(())
    }

    pub fn read_tags(&self) -> AssetResult<BTreeSet<TagId>> {
        Ok(self.bundle.read_asset_tags(&self.untyped_id())?.tags)
    }

    pub fn add_tag(&self, tag_id: &TagId) -> AssetResult<()> {
        let asset_id = self.untyped_id();
        let mut tags = self.read_tags()?;
        if !tags.insert(*tag_id) {
            return Err(AssetErrorKind::TagAlreadyAssigned {
                asset_id,
                tag_id: *tag_id,
            }
            .into());
        }

        let tags = tags.into_iter().collect::<Vec<_>>();
        self.bundle.write_asset_tags(&asset_id, &tags)?;
        self.index_db.add_tag_to_asset(&asset_id, tag_id)
    }

    pub fn remove_tag(&self, tag_id: &TagId) -> AssetResult<()> {
        let asset_id = self.untyped_id();
        let mut tags = self.read_tags()?;
        if !tags.remove(tag_id) {
            return Err(AssetErrorKind::TagNotAssigned {
                asset_id,
                tag_id: *tag_id,
            }
            .into());
        }

        let tags = tags.into_iter().collect::<Vec<_>>();
        self.bundle.write_asset_tags(&asset_id, &tags)?;
        self.index_db.remove_tag_from_asset(&asset_id, tag_id)
    }

    pub fn metadata(&self) -> AssetResult<AssetMetadata> {
        self.index_db.get_asset(&self.untyped_id())
    }
}
