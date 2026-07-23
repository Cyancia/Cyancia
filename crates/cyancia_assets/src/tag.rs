use std::collections::BTreeSet;

use cyancia_utils::wrapper;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::bundle::BundleId;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Display)]
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

#[derive(Debug, Clone)]
pub struct Tag {
    pub id: TagId,
    pub bundle_id: BundleId,
    pub relative_path: String,
    pub name: String,
    pub asset_ty: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagFile {
    pub id: TagId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_ty: Option<String>,
}

impl TagFile {
    pub fn new(name: String, asset_ty: Option<String>) -> Self {
        Self {
            id: TagId::new(Uuid::new_v4()),
            name,
            asset_ty,
        }
    }
}

impl From<Tag> for TagFile {
    fn from(tag: Tag) -> Self {
        Self {
            id: tag.id,
            name: tag.name,
            asset_ty: tag.asset_ty,
        }
    }
}

pub const TAG_EXT: &str = "ctag";

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct AssetTags {
    pub tags: BTreeSet<TagId>,
}

pub const ASSET_TAGS_EXT: &str = "tags";
