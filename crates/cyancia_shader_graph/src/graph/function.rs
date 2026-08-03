use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use arc_swap::ArcSwap;
use cyancia_assets::asset::{AssetHandle, AssetId};
use cyancia_utils::{log_err::LogErr, wrapper};
use gpui::{App, Entity};
use log::error;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    graph::{
        Graph, GraphData, node::GraphNodeRegistry, texture::SharedGraphTextureStorage,
        variable::GraphTypeRegistry,
    },
    save::SerializableGraphFunction,
    wgsl_std::{
        builtin_nodes, builtin_types,
        nodes::{GraphInputNode, GraphOutputNode},
    },
};

pub struct GraphFunctionData {}

pub static GRAPH_FUNCTION_TYPE_REGISTRY: LazyLock<GraphTypeRegistry> = LazyLock::new(builtin_types);
pub static GRAPH_FUNCTION_NODE_REGISTRY: LazyLock<GraphNodeRegistry<GraphFunctionData>> =
    LazyLock::new(|| {
        let mut nodes = builtin_nodes();

        nodes.register::<GraphInputNode>();
        nodes.register::<GraphOutputNode>();

        nodes
    });

impl GraphData for GraphFunctionData {
    fn type_registry() -> &'static GraphTypeRegistry {
        LazyLock::force(&GRAPH_FUNCTION_TYPE_REGISTRY)
    }

    fn node_registry() -> &'static GraphNodeRegistry<Self> {
        LazyLock::force(&GRAPH_FUNCTION_NODE_REGISTRY)
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub GraphFunctionId : Uuid
}

pub type FunctionGraph = Graph<GraphFunctionData>;

#[derive(Clone)]
pub struct GraphFunction {
    // FIXME This should always exist
    pub asset_id: Option<AssetId<SerializableGraphFunction>>,
    pub id: GraphFunctionId,
    pub name: String,
    pub graph: Entity<FunctionGraph>,
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
