use std::sync::Arc;

use bevy_math::IRect;
use cyancia_image::{CImage, layer::LayerId, tile::GpuTileStorageInner};
use cyancia_tools::{ToolProxyId, ToolsAppExt};
use cyancia_utils::wrapper;
use glam::IVec2;
use gpui::{App, AppContext, Entity, EventEmitter, Global};
use parking_lot::RwLock;
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    control::CanvasTransform,
    event::{CanvasCreated, CanvasRemoved, CanvasUpdated},
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
    id: CanvasId,
    tool_proxy_id: ToolProxyId,
    pub image: CImage,
    pub transform: CanvasTransform,
    dirty_tiles: IRect,
}

impl CCanvas {
    pub fn new(image: CImage, tool_proxy_id: ToolProxyId) -> Self {
        Self {
            id: CanvasId(Uuid::new_v4()),
            tool_proxy_id,
            image,
            transform: CanvasTransform::default(),
            dirty_tiles: IRect::default(),
        }
    }

    pub fn id(&self) -> CanvasId {
        self.id
    }

    pub fn tool_proxy_id(&self) -> ToolProxyId {
        self.tool_proxy_id
    }

    pub fn mark_dirty(&mut self, tiles: IRect) {
        self.dirty_tiles = self.dirty_tiles.union(tiles);
    }

    pub fn clear_dirty(&mut self) -> IRect {
        let rect = self.dirty_tiles;
        self.dirty_tiles = IRect::EMPTY;
        rect
    }
}

pub fn init(cx: &mut App) {
    let canvas_manager = CanvasManager::new(cx);
    cx.set_global(canvas_manager);
    cx.add_tool_function::<PanTool>();
    cx.add_tool_function::<RotateTool>();
    cx.add_tool_function::<ZoomTool>();
}

pub struct CanvasManager {
    canvases: Vec<CCanvas>,
    current_canvas: Option<usize>,
    events: Entity<CanvasEvents>,
}

pub struct CanvasEvents;

impl EventEmitter<CanvasCreated> for CanvasEvents {}

impl EventEmitter<CanvasRemoved> for CanvasEvents {}

impl EventEmitter<CanvasUpdated> for CanvasEvents {}

impl Global for CanvasManager {}

impl CanvasManager {
    pub fn new(cx: &mut App) -> Self {
        Self {
            canvases: Vec::new(),
            current_canvas: None,
            events: cx.new(|cx| CanvasEvents),
        }
    }

    pub fn add_canvas(&mut self, canvas: CCanvas, cx: &mut App) {
        self.current_canvas = Some(self.canvases.len());
        let id = canvas.id;
        let size = canvas.image.size();
        self.canvases.push(canvas);
        self.events.update(cx, move |_, cx| {
            cx.emit(CanvasCreated { id });
            cx.emit(CanvasUpdated {
                id,
                dirty_tiles: GpuTileStorageInner::pixel_rect_to_tile(IRect {
                    min: IVec2::ZERO,
                    max: size.as_ivec2(),
                }),
            });
        });
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

    pub fn events(&self) -> &Entity<CanvasEvents> {
        &self.events
    }
}
