use std::{
    backtrace::Backtrace,
    error::Error,
    fmt::{Debug, Display},
    path::{PathBuf, StripPrefixError},
};

use crate::{asset::UntypedAssetId, bundle::BundleId, tag::TagId};

#[derive(Debug, thiserror::Error)]
pub enum AssetErrorKind {
    #[error("Asset path not found for asset ID: {0}")]
    AssetPathNotFound(UntypedAssetId),
    #[error("Asset not found for asset ID: {0}")]
    AssetNotFound(UntypedAssetId),
    #[error("Asset bundle not found for bundle ID: {0}")]
    BundleNotFound(BundleId),
    #[error("No serializer found for asset extension: {0}")]
    SerializerNotFound(String),
    #[error("Tag not found for tag ID: {0}")]
    TagNotFound(TagId),
    #[error(
        "Tag {tag_id} cannot be added to asset {asset_id} of type {asset_ty}; expected {expected_ty}"
    )]
    InvalidTagAssetType {
        tag_id: TagId,
        asset_id: UntypedAssetId,
        asset_ty: String,
        expected_ty: String,
    },
    #[error("Tag {tag_id} is already assigned to asset {asset_id}")]
    TagAlreadyAssigned {
        asset_id: UntypedAssetId,
        tag_id: TagId,
    },
    #[error("Tag {tag_id} is not assigned to asset {asset_id}")]
    TagNotAssigned {
        asset_id: UntypedAssetId,
        tag_id: TagId,
    },
    #[error("Tag asset {asset_id} cannot have an asset tag sidecar at {path}")]
    TagAssetTagsNotAllowed {
        asset_id: UntypedAssetId,
        path: PathBuf,
    },
    #[error(
        "Tag {tag_id} is defined by multiple bundles: {first_bundle_id} and {second_bundle_id}"
    )]
    DuplicateTagDefinition {
        tag_id: TagId,
        first_bundle_id: BundleId,
        second_bundle_id: BundleId,
    },
    #[error(
        "Asset type restriction for tag {tag_id} cannot be changed from {current_asset_ty:?} to {new_asset_ty:?}"
    )]
    TagAssetTypeChanged {
        tag_id: TagId,
        current_asset_ty: Option<String>,
        new_asset_ty: Option<String>,
    },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Missing extension for asset path: {0}")]
    MissingExtension(PathBuf),
    #[error("Failed to downcast asset into desired type: {0}")]
    CastAssetError(String),
    #[error("Failed to (de)serialize asset: {0}")]
    SerializerError(Box<dyn Error + Send + Sync + 'static>),
    #[error("Failed to strip prefix from path: {0}")]
    StripPrefixError(StripPrefixError),
    #[error("Asset bundle error: {0}")]
    BundleError(Box<dyn Error + Send + Sync + 'static>),
    #[error("Failed to deserialize toml: {0}")]
    TomlDeError(#[from] toml::de::Error),
    #[error("Failed to serialize toml: {0}")]
    TomlSerError(#[from] toml::ser::Error),
    #[error("Sqlite error: {0}")]
    SqliteError(#[from] rusqlite::Error),
}

pub struct AssetError {
    kind: AssetErrorKind,
    backtrace: Box<Backtrace>,
}

impl AssetError {
    pub fn new(kind: AssetErrorKind) -> Self {
        Self {
            kind,
            backtrace: Box::new(Backtrace::capture()),
        }
    }

    pub fn kind(&self) -> &AssetErrorKind {
        &self.kind
    }

    pub fn into_kind(self) -> AssetErrorKind {
        self.kind
    }

    pub fn backtrace(&self) -> &Backtrace {
        &self.backtrace
    }
}

impl From<AssetErrorKind> for AssetError {
    fn from(kind: AssetErrorKind) -> Self {
        Self::new(kind)
    }
}

impl Display for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.kind, f)
    }
}

impl Debug for AssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\nBacktrace:\n{}", self.kind, self.backtrace)
    }
}

impl Error for AssetError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.kind.source()
    }
}

macro_rules! from_error {
    ($($error:ty),*) => {
        $(
            impl From<$error> for AssetError {
                fn from(value: $error) -> Self {
                    Self::from(AssetErrorKind::from(value))
                }
            }
        )*
    };
}
from_error!(
    std::io::Error,
    zip::result::ZipError,
    toml::de::Error,
    toml::ser::Error,
    rusqlite::Error
);

pub type AssetResult<T> = std::result::Result<T, AssetError>;
