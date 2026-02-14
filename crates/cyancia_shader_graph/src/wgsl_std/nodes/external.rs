use std::{collections::HashMap, sync::Arc};

use anyhow::anyhow;
use iced_core::{Color, Element, color};
use iced_widget::{Column, column, pick_list};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::{
    GraphRenderer, GraphTheme,
    graph::{
        node::{
            GraphNode, GraphNodeCodeGenContext, GraphNodeCodeGenError, GraphNodeInputsViewContext,
            GraphNodeOutputsViewContext, GraphNodeUpdateContext,
        },
        slot::{
            ErasedGraphLiteralUpdateMessage, GraphDefaultInputSlot, GraphDefaultOutputSlot,
        },
        variable::GraphLiteral,
    },
};

#[derive(Default)]
pub struct ExternalDataStorage {
    contents: RwLock<HashMap<ExternalLiteralId, Arc<GraphLiteral>>>,
}

impl ExternalDataStorage {
    pub fn insert(&self, id: ExternalLiteralId, value: GraphLiteral) {
        let mut contents = self.contents.write();

        contents.insert(id.clone(), Arc::new(value));
    }

    pub fn get(&self, id: &ExternalLiteralId) -> Option<Arc<GraphLiteral>> {
        self.contents.read().get(id).cloned()
    }

    pub fn all_id(&self) -> Vec<ExternalLiteralId> {
        self.contents.read().keys().cloned().collect()
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ExternalLiteralId {
    name: String,
}

impl ExternalLiteralId {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl ToString for ExternalLiteralId {
    fn to_string(&self) -> String {
        self.name.clone()
    }
}

impl Serialize for ExternalLiteralId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.name.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExternalLiteralId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        Ok(ExternalLiteralId { name })
    }
}

#[derive(Clone)]
pub struct ExternalLiteralType {
    storage: Arc<ExternalDataStorage>,
}

#[derive(Clone)]
pub struct ExternalNode {
    pub storage: Arc<ExternalDataStorage>,
}

impl ExternalNode {
    pub fn new(storage: Arc<ExternalDataStorage>) -> Self {
        Self { storage }
    }
}

#[derive(Clone)]
pub enum ExternalNodeMessage {
    IdChanged(ExternalLiteralId),
    LiteralUpdate(ErasedGraphLiteralUpdateMessage),
}

impl GraphNode for ExternalNode {
    type State = Option<ExternalLiteralId>;

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
            vec![GraphDefaultOutputSlot::new_boxed(dyn_clone::clone_box(
                v.ty(),
            ))]
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
        let literal = self
            .storage
            .get(id)
            .ok_or(anyhow!("External literal not found"))?;
        let code = literal
            .to_code()
            .ok_or(anyhow!("Cannot convert literal to code"))?;
        let output = ctx.get_output(0)?;
        Ok(format!("let {} = {};\n", output, code))
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

        column = column.push(pick_list(
            self.storage.all_id(),
            state.clone(),
            ExternalNodeMessage::IdChanged,
        ));

        column
            .extend(ctx.view_all_inputs(&["Var"], ExternalNodeMessage::LiteralUpdate))
            .into()
    }

    fn view_outputs(
        &self,
        state: &Self::State,
        ctx: GraphNodeOutputsViewContext,
    ) -> Element<'static, Self::Message, GraphTheme, GraphRenderer> {
        dbg!();
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
