use std::collections::HashMap;

use cyancia_utils::wrapper;
use indexmap::IndexMap;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub TextureId : Uuid
}

impl TextureId {
    // Null texture should have a default fallback, so they're also valid.
    pub const NULL: Self = Self(Uuid::nil());
}

#[derive(Clone)]
pub struct TextureObject {
    pub external_id: TextureId,
    pub name: String,
}

impl PartialEq for TextureObject {
    fn eq(&self, other: &Self) -> bool {
        self.external_id == other.external_id
    }
}

impl ToString for TextureObject {
    fn to_string(&self) -> String {
        self.name.clone()
    }
}

#[derive(Default, Clone)]
pub struct GraphTextureStorage {
    textures: HashMap<TextureId, TextureObject>,
}

impl GraphTextureStorage {
    pub fn new(textures: Vec<TextureObject>) -> Self {
        let textures = textures.into_iter().map(|t| (t.external_id, t)).collect();
        Self { textures }
    }

    pub fn get(&self, id: &TextureId) -> Option<&TextureObject> {
        self.textures.get(id)
    }

    pub fn all(&self) -> &HashMap<TextureId, TextureObject> {
        &self.textures
    }
}

#[derive(Default)]
pub struct GraphTextureUsageRecorder {
    inner: IndexMap<TextureId, u32>,
}

impl GraphTextureUsageRecorder {
    pub fn use_texture(&mut self, id: TextureId) -> u32 {
        let e = self.inner.entry(id);
        let local_index = e.index() as u32;
        e.and_modify(|index| *index += 1).or_insert(0);
        local_index
    }

    pub fn used_textures_ordered(&self) -> Vec<TextureId> {
        self.inner.keys().cloned().collect()
    }
}
