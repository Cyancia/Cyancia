use std::sync::Arc;

use gpui::{Action, App};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::ActionFunction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct OpenBrushEditorAction;

impl ActionFunction for OpenBrushEditorAction {
    fn trigger(&self, cx: &mut App) {
        // TODO Open brush editor
        todo!()
    }
}
