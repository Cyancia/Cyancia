use iced_runtime::Task;
use lapiz_runtime::{
    Services,
    windows::{OpenWindowViewCommand, WindowCommandBuffer, WindowViewId},
};

use crate::{ActionFunction, ActionId};

#[derive(Default)]
pub struct OpenBrushEditorAction;

impl ActionFunction for OpenBrushEditorAction {
    type Message = ();

    fn id(&self) -> ActionId {
        ActionId::new("OpenBrushEditorAction".into())
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
