use std::sync::Arc;

use cyancia_view::{ViewAppExt, ViewId};
use gpui::{Action, App};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::ActionFunction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct OpenBrushEditorAction;

impl ActionFunction for OpenBrushEditorAction {
    fn trigger(&self, cx: &mut App) {
        cx.open_view(ViewId::new("brush_editor"));
    }
}
