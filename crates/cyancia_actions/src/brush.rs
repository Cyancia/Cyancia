use std::sync::Arc;

use cyancia_input::action::{Action, ActionId};
use cyancia_runtime::{
    Services,
    windows::{OpenWindowCommand, WindowCommandBuffer, WindowViewId},
};
use iced_runtime::Task;

use crate::ActionFunction;

#[derive(Default)]
pub struct OpenBrushEditorAction {}

impl ActionFunction for OpenBrushEditorAction {
    fn id(&self) -> ActionId {
        ActionId::new("open_brush_editor_action".into())
    }

    fn trigger(&self, services: Arc<Services>) -> Task<()> {
        services
            .service_mut::<WindowCommandBuffer>()
            .push(OpenWindowCommand::new(WindowViewId::new("brush_editor")));
        Task::none()
    }
}
