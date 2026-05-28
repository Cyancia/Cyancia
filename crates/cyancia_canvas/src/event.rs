use bevy_math::IRect;

use crate::CanvasId;

#[derive(Debug, Clone)]
pub struct CanvasCreated {
    pub id: CanvasId,
}

#[derive(Debug, Clone)]
pub struct CanvasRemoved {
    pub id: CanvasId,
}

#[derive(Debug, Clone)]
pub struct CanvasUpdate {
    pub id: CanvasId,
    pub dirty_tiles: IRect,
}
