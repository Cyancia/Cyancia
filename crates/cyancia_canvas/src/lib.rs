use std::sync::Arc;

use cyancia_image::CImage;
use cyancia_runtime::{Application, Runtime, Services, plugin::Plugin, service::Service};
use cyancia_tools::{ToolProxyId, ToolsAppExt};
use cyancia_utils::wrapper;
use parking_lot::RwLock;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    control::CanvasTransform,
    render::CanvasRenderer,
    tools::{PanTool, RotateTool, ZoomTool},
};

pub mod control;
pub mod event;
pub mod render;
pub mod resource;
pub mod tools;
pub mod widget;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display)]
    #[display("{0}")]
    pub CanvasId : Uuid
}

#[derive(Debug)]
pub struct CCanvas {
    pub id: CanvasId,
    pub tool_proxy_id: ToolProxyId,
    pub image: CImage,
    pub transform: CanvasTransform,
}

pub struct CanvasPlugin;

impl Plugin for CanvasPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<CanvasManager>()
            .add_tool_function::<PanTool>()
            .add_tool_function::<RotateTool>()
            .add_tool_function::<ZoomTool>();
    }
}

#[derive(Default)]
pub struct CanvasManager {
    canvases: Vec<CCanvas>,
    current_canvas: Option<usize>,
}

impl Service for CanvasManager {}

impl CanvasManager {
    pub fn add_canvas(&mut self, canvas: CCanvas) {
        self.current_canvas = Some(self.canvases.len());
        self.canvases.push(canvas);
    }

    pub fn get(&self, id: &CanvasId) -> Option<&CCanvas> {
        self.canvases.iter().find(|c| c.id == *id)
    }

    pub fn get_mut(&mut self, id: &CanvasId) -> Option<&mut CCanvas> {
        self.canvases.iter_mut().find(|c| c.id == *id)
    }

    pub fn current(&self) -> Option<&CCanvas> {
        let cur = self.current_canvas?;
        self.canvases.get(cur)
    }

    pub fn current_mut(&mut self) -> Option<&mut CCanvas> {
        let cur = self.current_canvas?;
        self.canvases.get_mut(cur)
    }

    pub fn current_id(&self) -> Option<CanvasId> {
        self.current_canvas.map(|i| self.canvases[i].id)
    }

    pub fn set_current(&mut self, id: CanvasId) {
        self.current_canvas = self.canvases.iter().position(|c| c.id == id);
    }
}
