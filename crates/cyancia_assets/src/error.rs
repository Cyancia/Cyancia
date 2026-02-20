use std::{
    error::Error,
    path::{PathBuf, StripPrefixError},
};

use crate::asset::AssetId;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("Asset path not found for asset ID: {0}")]
    AssetPathNotFound(AssetId),
    #[error("Asset not found for asset ID: {0}")]
    AssetNotFound(AssetId),
    #[error("No serializer found for asset extension: {0}")]
    SerializerNotFound(String),
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
    #[error("SQLx error: {0}")]
    SqlxError(#[from] sqlx::Error),
}

pub type AssetResult<T> = std::result::Result<T, AssetError>;
