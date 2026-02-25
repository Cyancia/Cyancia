use std::sync::Arc;

use cyancia_image::CImage;
use cyancia_runtime::{Application, Runtime, plugin::Plugin};
use cyancia_utils::wrapper;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    control::CanvasTransform,
    render::{CanvasRenderer, CanvasRenderers},
};

pub mod control;
pub mod render;
pub mod resource;
pub mod widget;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub CanvasId : Uuid
}

#[derive(Debug)]
pub struct CCanvas {
    pub id: CanvasId,
    pub image: Arc<CImage>,
    pub transform: RwLock<CanvasTransform>,
}

pub struct CanvasPlugin;

impl Plugin for CanvasPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<CanvasRenderers>();
    }
}
