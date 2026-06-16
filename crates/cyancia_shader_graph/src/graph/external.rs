use cyancia_utils::wrapper;
use dashmap::DashMap;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::graph::variable::{GraphLiteral, GraphLiteralValue};

pub fn generate_external_variable_name(var: &ExternalVariable) -> String {
    let sanitized_name = var
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>();

    format!(
        "external_{}_{}",
        sanitized_name,
        var.id.to_string().replace('-', "_")
    )
}

pub fn generate_external_variable_binding(
    group: u32,
    binding: u32,
    var: &ExternalVariable,
) -> String {
    format!(
        "@group({}) @binding({}) var<storage> {}: {};",
        group,
        binding,
        generate_external_variable_name(var),
        var.value
            .ty()
            .wgsl_type()
            .expect("External variables must has a corresponding wgsl type.")
    )
}

#[derive(Default)]
pub struct GraphExternalVariableStorage {
    contents: DashMap<ExternalVariableId, ExternalVariable>,
}

impl GraphExternalVariableStorage {
    pub fn new(variables: Vec<ExternalVariable>) -> Self {
        let contents = variables.into_iter().map(|var| (var.id, var)).collect();
        Self { contents }
    }

    pub fn get(&self, id: &ExternalVariableId) -> Option<ExternalVariable> {
        self.contents.get(id).map(|v| v.value().clone())
    }

    pub fn all(&self) -> &DashMap<ExternalVariableId, ExternalVariable> {
        &self.contents
    }

    pub fn insert(&self, var: ExternalVariable) {
        self.contents.insert(var.id, var);
    }

    pub fn rename(&self, id: &ExternalVariableId, new_name: String) {
        if let Some(mut var) = self.contents.get_mut(id) {
            var.name = new_name;
        }
    }

    pub fn update(&self, id: &ExternalVariableId, new_value: Box<dyn GraphLiteralValue>) {
        let Some(mut var) = self.contents.get_mut(id) else {
            return;
        };

        var.value.set_boxed(new_value);
    }

    pub fn remove(&self, id: &ExternalVariableId) {
        self.contents.remove(id);
    }
}

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub ExternalVariableId : Uuid
}

#[derive(Clone)]
pub struct ExternalVariable {
    pub id: ExternalVariableId,
    pub name: String,
    pub value: GraphLiteral,
}
