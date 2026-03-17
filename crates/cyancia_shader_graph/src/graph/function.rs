use std::collections::HashMap;

use cyancia_utils::wrapper;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::graph::Graph;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub GraphFunctionId : Uuid
}

pub struct GraphFunction {
    pub id: GraphFunctionId,
    pub name: String,
    pub graph: Graph,
}

#[derive(Default)]
pub struct GraphFunctionStorage {
    functions: HashMap<GraphFunctionId, GraphFunction>,
}

impl GraphFunctionStorage {
    pub fn new(functions: HashMap<GraphFunctionId, GraphFunction>) -> Self {
        Self { functions }
    }

    pub fn get(&self, id: &GraphFunctionId) -> Option<&GraphFunction> {
        self.functions.get(id)
    }

    pub fn all(&self) -> &HashMap<GraphFunctionId, GraphFunction> {
        &self.functions
    }
}
