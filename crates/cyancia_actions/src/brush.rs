use std::sync::Arc;

use async_trait::async_trait;
use cyancia_input::action::{Action, ActionId};
use cyancia_runtime::{
    Services,
    windows::{OpenWindowCommand, WindowCommandBuffer, WindowViewId},
};

use crate::ActionFunction;

#[derive(Default)]
pub struct OpenBrushEditorAction {}

#[async_trait]
impl ActionFunction for OpenBrushEditorAction {
    fn id(&self) -> ActionId {
        ActionId::new("open_brush_editor_action".into())
    }

    async fn trigger(&self, services: Arc<Services>) {
        services
            .service_mut::<WindowCommandBuffer>()
            .push(OpenWindowCommand::new(WindowViewId::new("brush_editor")));
    }
}
