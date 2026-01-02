use cyancia_id::Id;
use cyancia_input::action::Action;

use crate::{ActionFunction, shell::ActionShell};

pub struct OpenBrushGraph;

impl ActionFunction for OpenBrushGraph {
    fn id(&self) -> Id<Action> {
        Id::from_str("open_brush_graph")
    }

    fn trigger(&self, shell: &mut ActionShell) {
        todo!()
    }
}
