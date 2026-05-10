use std::collections::HashMap;

use cyancia_utils::wrapper;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::graph::{Graph, GraphData};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub GraphFunctionId : Uuid
}

pub struct GraphFunction<Data: GraphData> {
    pub id: GraphFunctionId,
    pub name: String,
    pub graph: Graph<Data>,
}

#[derive(Default)]
pub struct GraphFunctionStorage<Data: GraphData> {
    functions: HashMap<GraphFunctionId, GraphFunction<Data>>,
}

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
}
