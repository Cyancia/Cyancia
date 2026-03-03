use std::io::{Read, Write, read_to_string};

use cyancia_utils::wrapper;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    asset::{Asset, AssetId},
    loader::AssetSerializer,
};

wrapper! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub TagId : Uuid
}

impl rusqlite::types::FromSql for TagId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(Uuid::column_result(value)?))
    }
}

impl rusqlite::types::ToSql for TagId {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        self.0.to_sql()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    tag_id: TagId,
    name: String,
    assets: Vec<AssetId>,
}

impl Asset for Tag {
    const TYPE_NAME: &'static str = "tag";
}

impl Tag {
    pub fn new(name: String) -> Self {
        Self {
            tag_id: TagId::new(Uuid::new_v4()),
            name,
            assets: Vec::new(),
        }
    }

    pub fn id(&self) -> &TagId {
        &self.tag_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    pub fn assets(&self) -> &[AssetId] {
        &self.assets
    }

    pub fn add_asset(&mut self, asset_id: AssetId) {
        if !self.assets.contains(&asset_id) {
            self.assets.push(asset_id);
        }
    }

    pub fn remove_asset(&mut self, asset_id: &AssetId) {
        self.assets.retain(|id| id != asset_id);
    }
}

#[derive(Default)]
pub struct TagSerializer;

#[derive(Debug, thiserror::Error)]
pub enum TagSerializerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Toml serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("Toml deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
}

impl AssetSerializer for TagSerializer {
    type Asset = Tag;

    type Error = TagSerializerError;

    fn file_extension() -> &'static str {
        "ctag"
    }

    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
        Ok(toml::from_str(&read_to_string(reader)?)?)
    }

    fn write(&self, asset: &Self::Asset, writer: &mut dyn Write) -> Result<(), Self::Error> {
        let toml_str = toml::to_string(asset)?;
        writer.write_all(toml_str.as_bytes())?;
        Ok(())
    }
}
