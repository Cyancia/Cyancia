use bevy_math::IRect;
use cyancia_image::layer::{LayerId, properties::LayerProperties};
use cyancia_runtime::event::Event;

use crate::CanvasId;

#[derive(Debug, Clone, Event)]
pub struct CanvasCreated {
    pub id: CanvasId,
}

#[derive(Event, Debug, Clone)]
pub struct CurrentCanvasChanged {
    pub from: Option<CanvasId>,
    pub to: Option<CanvasId>,
}

#[derive(Event, Debug, Clone)]
pub struct CanvasRemoved {
    pub id: CanvasId,
}

#[derive(Event, Debug, Clone)]
pub struct CanvasUpdated {
    pub id: CanvasId,
    pub dirty_tiles: IRect,
}

#[derive(Event, Debug, Clone)]
pub struct CanvasActiveLayerChanged {
    pub from: LayerId,
    pub to: LayerId,
}

#[derive(Debug, Clone)]
pub struct CanvasLayerPropertyChanged {
    pub layer_id: LayerId,
    pub old: LayerProperties,
}
