use std::{any::TypeId, marker::PhantomData};

use cyancia_utils::Deref;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::asset::Asset;

#[derive(Deref)]
pub struct AssetId<T: Asset> {
    #[deref]
    id: Uuid,
    _marker: PhantomData<T>,
}

impl<T: Asset> std::fmt::Debug for AssetId<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.id.fmt(f)
    }
}

impl<T: Asset> Clone for AssetId<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            _marker: PhantomData,
        }
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

impl<T: Asset> AssetId<T> {
    pub fn random() -> Self {
        Self {
            id: Uuid::new_v4(),
            _marker: PhantomData,
        }
    }

    pub fn from_str(s: &str) -> Self {
        Self {
            id: Uuid::from_u128(xxhash_rust::xxh3::xxh3_128(s.as_bytes())),
            _marker: PhantomData,
        }
    }

    pub fn untyped(self) -> UntypedAssetId {
        UntypedAssetId {
            id: self.id,
            ty: TypeId::of::<T>(),
        }
    }
}

impl<T: Asset> Serialize for AssetId<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.id.serialize(serializer)
    }
}

impl<'de, T: Asset> Deserialize<'de> for AssetId<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = Uuid::deserialize(deserializer)?;
        Ok(Self {
            id,
            _marker: PhantomData,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UntypedAssetId {
    id: Uuid,
    ty: TypeId,
}

impl UntypedAssetId {
    pub fn random_typed<T: Asset>() -> Self {
        Self::random(TypeId::of::<T>())
    }

    pub fn from_str_typed<T: Asset>(s: &str) -> Self {
        Self::from_str(s, TypeId::of::<T>())
    }

    pub fn random(ty: TypeId) -> Self {
        Self {
            id: Uuid::new_v4(),
            ty,
        }
    }

    pub fn from_str(s: &str, ty: TypeId) -> Self {
        Self {
            id: Uuid::from_u128(xxhash_rust::xxh3::xxh3_128(s.as_bytes())),
            ty,
        }
    }

    pub fn typed<T: Asset>(self) -> Option<AssetId<T>> {
        if self.ty == TypeId::of::<T>() {
            Some(AssetId {
                id: self.id,
                _marker: PhantomData,
            })
        } else {
            None
        }
    }
}
