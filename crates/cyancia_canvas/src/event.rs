use bevy_math::IRect;

use crate::CanvasId;

#[derive(Debug, Clone)]
pub struct CanvasCreated {
    pub id: CanvasId,
}

#[derive(Debug, Clone)]
pub struct CurrentCanvasChanged {
    pub from: Option<CanvasId>,
    pub to: Option<CanvasId>,
}

#[derive(Debug, Clone)]
pub struct CanvasRemoved {
    pub id: CanvasId,
}

#[derive(Debug, Clone)]
pub struct CanvasUpdated {
    pub dirty_tiles: IRect,
}

#[derive(Debug, Clone)]
pub struct CanvasLayerStackUpdated {}
