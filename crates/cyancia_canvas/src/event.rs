use bevy_math::IRect;
use cyancia_image::layer::{LayerId, properties::LayerProperties};

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
pub struct CanvasActiveLayerChanged {
    pub from: LayerId,
    pub to: LayerId,
}

#[derive(Debug, Clone)]
pub struct CanvasLayerPropertyChanged {
    pub layer_id: LayerId,
    pub old: LayerProperties,
}
