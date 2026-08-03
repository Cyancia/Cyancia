use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use anyhow::{Result, anyhow};
use arc_swap::ArcSwap;
use cyancia_assets::asset::{AssetHandle, AssetId};
use cyancia_utils::{log_err::LogErr, wrapper};
use gpui::{App, Entity};
use log::error;
use parking_lot::RwLock;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    graph::{Graph, GraphData, texture::SharedGraphTextureStorage},
    save::SerializableGraphFunction,
};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub GraphFunctionId : Uuid
}

#[derive(Clone)]
pub struct GraphFunction {
    // FIXME This should always exist
    pub asset_id: Option<AssetId<SerializableGraphFunction>>,
    pub id: GraphFunctionId,
    pub name: String,
    pub graph: Entity<Graph<()>>,
}

#[allow(type_alias_bounds)]
pub type SharedGraphFunctionStorage = Arc<ArcSwap<GraphFunctionStorage>>;

#[derive(Default)]
pub struct GraphFunctionStorage {
    functions: HashMap<GraphFunctionId, GraphFunction>,
}

pub static ASSET_GRAPH_FUNCTION_STORAGE: LazyLock<SharedGraphFunctionStorage> =
    LazyLock::new(Default::default);

impl GraphFunctionStorage {
    pub fn new(
        textures: SharedGraphTextureStorage,
        functions: SharedGraphFunctionStorage,
        handles: Vec<AssetHandle<SerializableGraphFunction>>,
        cx: &mut App,
    ) -> Self {
        let functions = handles
            .into_iter()
            .filter_map(|handle| {
                let ser_func = handle.get().logged_err().ok()?;
                let (maybe_func, handle_errs) = ser_func.deserialize_func(
                    textures.clone(),
                    functions.clone(),
                    Some(handle.id()),
                    cx,
                );
                if !handle_errs.is_empty() {
                    error!("Error deserializing graph function {}:", handle.id());
                    for err in handle_errs {
                        error!("  - {}", err);
                    }
                }
                let func = maybe_func?;
                Some((func.id, func))
            })
            .collect();
        Self { functions }
    }

    pub fn get(&self, id: &GraphFunctionId) -> Option<&GraphFunction> {
        self.functions.get(id)
    }

    pub fn all(&self) -> &HashMap<GraphFunctionId, GraphFunction> {
        &self.functions
    }
}
