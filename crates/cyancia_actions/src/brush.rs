use std::sync::Arc;

use cyancia_input::action::{Action, ActionId};
use cyancia_runtime::{
    Services,
    windows::{OpenWindowViewCommand, WindowCommandBuffer, WindowViewId},
};
use iced_runtime::Task;

use crate::ActionFunction;

#[derive(Default)]
pub struct OpenBrushEditorAction {}

impl ActionFunction for OpenBrushEditorAction {
    type Message = ();

    fn id(&self) -> ActionId {
        ActionId::new("open_brush_editor_action".into())
    }

    fn trigger(&self, services: &mut Services) -> Task<Self::Message> {
        services
            .service_mut::<WindowCommandBuffer>()
            .push(OpenWindowViewCommand::new(WindowViewId::new(
                "brush_editor",
            )));
        Task::none()
    }
}
