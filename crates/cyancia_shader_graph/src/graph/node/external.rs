use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use cyancia_utils::wrapper;
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
    format!(
        "external_{}_{}",
        var.name,
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
        var.value.ty().wgsl_type().unwrap()
    )
}

#[derive(Default)]
pub struct ExternalVariableStorage {
    contents: RwLock<HashMap<ExternalVariableId, Arc<ExternalVariable>>>,
}

impl ExternalVariableStorage {
    pub fn from_hashmap(contents: HashMap<ExternalVariableId, Arc<ExternalVariable>>) -> Self {
        Self {
            contents: RwLock::new(contents),
        }
    }

    pub fn insert(&self, id: ExternalVariableId, value: ExternalVariable) {
        let mut contents = self.contents.write();

        contents.insert(id.clone(), Arc::new(value));
    }

    pub fn get(&self, id: &ExternalVariableId) -> Option<Arc<ExternalVariable>> {
        self.contents.read().get(id).cloned()
    }

    pub fn update(&self, id: ExternalVariableId, message: ErasedGraphLiteralUpdateMessage) {
        let mut contents = self.contents.write();
        let Some(lit) = contents.get(&id) else {
            return;
        };
        let mut lit = lit.as_ref().clone();
        lit.value.update(message);
        contents.insert(id, Arc::new(lit));
    }

    pub fn remove(&self, id: &ExternalVariableId) {
        self.contents.write().remove(id);
    }

    pub fn all(&self) -> RwLockReadGuard<'_, HashMap<ExternalVariableId, Arc<ExternalVariable>>> {
        self.contents.read()
    }

    pub fn all_mut(
        &self,
    ) -> RwLockWriteGuard<'_, HashMap<ExternalVariableId, Arc<ExternalVariable>>> {
        self.contents.write()
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

#[derive(Clone)]
pub struct ExternalVariableReference {
    pub id: ExternalVariableId,
    pub name: String,
}

impl PartialEq for ExternalVariableReference {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl ToString for ExternalVariableReference {
    fn to_string(&self) -> String {
        self.name.clone()
    }
}

#[derive(Clone)]
pub struct ExternalNode {
    pub storage: Arc<ExternalVariableStorage>,
}

impl ExternalNode {
    pub fn new(storage: Arc<ExternalVariableStorage>) -> Self {
        Self { storage }
    }
}

#[derive(Clone)]
pub enum ExternalNodeMessage {
    IdChanged(ExternalVariableId),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl GraphNode for ExternalNode {
    type State = Option<ExternalVariableId>;

    type Message = ExternalNodeMessage;

    fn name(&self) -> &'static str {
        "External"
    }

    fn header_color(&self) -> Color {
        color!(0x79c9f2)
    }

    fn create_inputs(&self, _state: &Self::State) -> Vec<GraphDefaultInputSlot> {
        vec![]
    }

    fn create_outputs(&self, state: &Self::State) -> Vec<GraphDefaultOutputSlot> {
        if let Some(v) = state.as_ref().and_then(|id| self.storage.get(&id)) {
            vec![GraphDefaultOutputSlot::new_boxed(v.value.ty().clone())]
        } else {
            vec![]
        }
    }

    fn generate_code(
        &self,
        state: &Self::State,
        mut ctx: GraphNodeCodeGenContext,
    ) -> Result<String, GraphNodeCodeGenError> {
        let id = state
            .as_ref()
            .ok_or(anyhow!("No external literal selected"))?;
        let var = self
            .storage
            .get(id)
            .ok_or(anyhow!("Selected external literal not found in storage"))?;
        let output = ctx.get_output(0)?;
        // TODO: Use uniform buffer to transfer external variables into shader.
        //       For current architecture, everytime user modifies them, the whole shader needs to be recompiled.
        Ok(format!(
            "let {} = {};\n",
            output,
            generate_external_variable_name(var.as_ref())
        ))
    }

    fn default_state(&self) -> Self::State {
        None
    }

    fn view_inputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeInputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        let mut column = column![];

        let vars = self.storage.all();
        let refs = vars
            .iter()
            .map(|(id, lit)| ExternalVariableReference {
                id: id.clone(),
                name: lit.name.clone(),
            })
            .collect::<Vec<_>>();
        let selected = state.as_ref().and_then(|id| {
            self.storage.get(id).map(|v| ExternalVariableReference {
                id: id.clone(),
                name: v.name.clone(),
            })
        });
        column = column.push(pick_list(refs, selected, |v| {
            ExternalNodeMessage::IdChanged(v.id)
        }));

        column
            .extend(ctx.view_all_inputs(&["Var"], ExternalNodeMessage::LiteralUpdate))
            .into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        Column::with_children(ctx.view_all_outputs(&["Value"])).into()
    }

    fn update(
        &self,
        state: &mut Self::State,
        message: Self::Message,
        mut ctx: GraphNodeUpdateContext,
    ) {
        match message {
            ExternalNodeMessage::IdChanged(id) => *state = Some(id),
            ExternalNodeMessage::LiteralUpdate(m) => {
                ctx.update_literal(m);
            }
        }
    }
}
