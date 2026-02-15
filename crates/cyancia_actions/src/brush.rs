use cyancia_id::Id;
use cyancia_input::action::Action;

use crate::{ActionFunction, shell::ActionShell};

#[derive(Default)]
pub struct OpenBrushEditorAction {}

impl ActionFunction for OpenBrushEditorAction {
    fn id(&self) -> Id<Action> {
        Id::from_str("open_brush_editor_action")
    }

    fn trigger(&self, shell: &mut ActionShell) {
        shell.toggle_window(Id::from_str("brush_editor"));
    }
}
