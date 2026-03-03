use std::sync::Arc;

use cyancia_image::CImage;
use cyancia_runtime::{Application, Runtime, Services, plugin::Plugin, service::Service};
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
        app.add_service::<CanvasRenderers>()
            .add_service::<CanvasManager>();
    }
}

#[derive(Default)]
pub struct CanvasManager {
    canvases: Vec<Arc<CCanvas>>,
    current_canvas: Option<usize>,
}

impl Service for CanvasManager {}

impl CanvasManager {
    pub fn add_canvas(&mut self, canvas: CCanvas) {
        self.current_canvas = Some(self.canvases.len());
        self.canvases.push(Arc::new(canvas));
    }

    pub fn current(&self) -> Option<Arc<CCanvas>> {
        let current_id = self.current_canvas?;
        self.canvases.get(current_id).cloned()
    }

    pub fn current_id(&self) -> Option<CanvasId> {
        self.current_canvas.map(|i| self.canvases[i].id)
    }

    pub fn set_current(&mut self, id: CanvasId) {
        self.current_canvas = self.canvases.iter().position(|c| c.id == id);
    }
}
