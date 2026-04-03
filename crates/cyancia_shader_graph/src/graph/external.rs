use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use cyancia_utils::wrapper;
use dashmap::DashMap;
use iced_core::{Color, Element, color};
use iced_widget::{Column, column, pick_list};
use parking_lot::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext,
        },
        slot::{ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot},
        variable::GraphLiteral,
    },
    save::SerializableGraphLiteral,
};

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
        let contents = variables
            .into_iter()
            .map(|var| (var.id.clone(), var))
            .collect();
        Self { contents: contents }
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

    pub fn update(&self, id: &ExternalVariableId, message: ErasedGraphLiteralUpdateMessage) {
        let Some(mut var) = self.contents.get_mut(id) else {
            return;
        };

        var.value.update(message);
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
