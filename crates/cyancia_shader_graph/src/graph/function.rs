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
    graph::{Graph, GraphData},
    save::SerializableGraphFunction,
};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub GraphFunctionId : Uuid
}

pub struct GraphFunction<Data: GraphData> {
    // FIXME This should always exist
    pub asset_id: Option<AssetId<SerializableGraphFunction>>,
    pub id: GraphFunctionId,
    pub name: String,
    pub graph: Entity<Graph<Data>>,
}

#[allow(type_alias_bounds)]
pub type SharedGraphFunctionStorage<Data: GraphData> = &'static ArcSwap<GraphFunctionStorage<Data>>;

#[derive(Default)]
pub struct GraphFunctionStorage<Data: GraphData> {
    functions: HashMap<GraphFunctionId, GraphFunction<Data>>,
}

static GLOBAL_SHARED_GRAPH_FUNCTION_STORAGE: LazyLock<
    RwLock<HashMap<TypeId, &'static (dyn Any + Send + Sync)>>,
> = LazyLock::new(Default::default);

impl<Data: GraphData> GraphFunctionStorage<Data> {
    pub fn new(functions: HashMap<GraphFunctionId, GraphFunction<Data>>) -> Self {
        Self { functions }
    }

    pub fn get(&self, id: &GraphFunctionId) -> Option<&GraphFunction<Data>> {
        self.functions.get(id)
    }

    pub fn all(&self) -> &HashMap<GraphFunctionId, GraphFunction<Data>> {
        &self.functions
    }

    pub fn update_from_assets(assets: &[AssetHandle<SerializableGraphFunction>], cx: &mut App) {
        let mut functions = HashMap::with_capacity(assets.len());
        for handle in assets {
            let Ok(ser_func) = handle.get().logged_err() else {
                continue;
            };
            let (maybe_func, handle_errs) = ser_func.deserialize_func(Some(handle.id()), cx);
            if !handle_errs.is_empty() {
                error!("Error deserializing graph function {}:", handle.id());
                for err in handle_errs {
                    error!("  - {}", err);
                }
            }
            if let Some(func) = maybe_func {
                functions.insert(func.id, func);
            }
        }

        let storage = Self { functions };
        Self::get_shared().store(Arc::new(storage));
    }

    pub fn get_shared() -> SharedGraphFunctionStorage<Data> {
        let type_id = TypeId::of::<Data>();

        {
            let storages = GLOBAL_SHARED_GRAPH_FUNCTION_STORAGE.read();
            if let Some(&storage) = storages.get(&type_id) {
                return storage
                    .downcast_ref::<SharedGraphFunctionStorage<Data>>()
                    .expect("graph function storage type must match its TypeId");
            }
        }

        let mut storages = GLOBAL_SHARED_GRAPH_FUNCTION_STORAGE.write();
        let storage = *storages.entry(type_id).or_insert_with(|| {
            Box::leak(Box::new(ArcSwap::from_pointee(Self::new(HashMap::new()))))
                as &'static (dyn Any + Send + Sync)
        });

        storage
            .downcast_ref::<SharedGraphFunctionStorage<Data>>()
            .unwrap()
    }
}
