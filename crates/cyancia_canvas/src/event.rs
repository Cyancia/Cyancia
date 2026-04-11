use cyancia_runtime::event::Event;

use crate::CanvasId;

#[derive(Event, Debug, Clone)]
pub struct CanvasCreated {
    pub id: CanvasId,
}

#[derive(Event, Debug, Clone)]
pub struct CanvasRemoved {
    pub id: CanvasId,
}
