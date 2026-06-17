use std::collections::HashMap;

use bevy_math::IRect;
use cyancia_image::CImage;
use cyancia_tools::{ToolProxyId, ToolsAppExt};
use cyancia_undo::{UndoCommand, UndoStack, UndoStacks};
use cyancia_utils::wrapper;
use gpui::{App, AppContext, BorrowAppContext, Context, Entity, EventEmitter, Global, WeakEntity};
use parse_display::Display;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    control::CanvasTransform,
    event::{
        CanvasCreated, CanvasLayerStackUpdated, CanvasRemoved, CanvasUpdated, CurrentCanvasChanged,
    },
    tools::{PanTool, RotateTool, ZoomTool},
};

pub mod command;
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

impl EventEmitter<CanvasUpdated> for CCanvas {}

impl EventEmitter<CanvasLayerStackUpdated> for CCanvas {}

pub fn init(cx: &mut App) {
    let cm = CanvasManager::new(cx);
    cx.set_global(cm);
    cx.add_tool_function::<PanTool>();
    cx.add_tool_function::<RotateTool>();
    cx.add_tool_function::<ZoomTool>();
}

pub trait CanvasAppExt {
    fn add_canvas(&mut self, canvas: CCanvas, cx: &mut App);
    fn remove_canvas(&mut self, id: &CanvasId, cx: &mut App);
    fn global_canvas_events_entity(&self) -> Entity<GlobalCanvasEvents>;
    fn current_canvas_id(&self) -> Option<CanvasId>;
    fn current_canvas(&self) -> Option<WeakEntity<CCanvas>>;
    fn read_current_canvas(&self) -> Option<&CCanvas>;
    fn update_current_canvas<R>(
        &mut self,
        update: impl FnOnce(&mut CCanvas, &mut Context<CCanvas>) -> R,
    ) -> Option<R>;
    fn canvas(&self, id: &CanvasId) -> Option<WeakEntity<CCanvas>>;
    fn read_canvas(&self, id: &CanvasId) -> Option<&CCanvas>;
    fn update_canvas<R>(
        &mut self,
        id: &CanvasId,
        update: impl FnOnce(&mut CCanvas, &mut Context<CCanvas>) -> R,
    ) -> Option<R>;
    fn set_current_canvas(&mut self, id: CanvasId);
}

impl CanvasAppExt for App {
    fn add_canvas(&mut self, canvas: CCanvas, _: &mut App) {
        self.update_global::<CanvasManager, _>(|cm, cx| cm.add_canvas(canvas, cx));
    }

    fn remove_canvas(&mut self, id: &CanvasId, _: &mut App) {
        self.update_global::<CanvasManager, _>(|cm, cx| cm.remove_canvas(id, cx));
    }

    fn global_canvas_events_entity(&self) -> Entity<GlobalCanvasEvents> {
        self.global::<CanvasManager>().event_emitter()
    }

    fn current_canvas_id(&self) -> Option<CanvasId> {
        self.global::<CanvasManager>().current_id()
    }

    fn current_canvas(&self) -> Option<WeakEntity<CCanvas>> {
        self.global::<CanvasManager>().current()
    }

    fn read_current_canvas(&self) -> Option<&CCanvas> {
        self.global::<CanvasManager>().read_current(self)
    }

    fn update_current_canvas<R>(
        &mut self,
        update: impl FnOnce(&mut CCanvas, &mut Context<CCanvas>) -> R,
    ) -> Option<R> {
        self.update_global::<CanvasManager, _>(|cm, cx| cm.update_current(cx, update))
    }

    fn canvas(&self, id: &CanvasId) -> Option<WeakEntity<CCanvas>> {
        self.global::<CanvasManager>().get(id)
    }

    fn read_canvas(&self, id: &CanvasId) -> Option<&CCanvas> {
        self.global::<CanvasManager>().read(id, self)
    }

    fn update_canvas<R>(
        &mut self,
        id: &CanvasId,
        update: impl FnOnce(&mut CCanvas, &mut Context<CCanvas>) -> R,
    ) -> Option<R> {
        self.update_global::<CanvasManager, _>(|cm, cx| cm.update(id, cx, update))
    }

    fn set_current_canvas(&mut self, id: CanvasId) {
        self.update_global::<CanvasManager, _>(|cm, cx| cm.set_current(id, cx));
    }
}

pub struct GlobalCanvasEvents;

impl EventEmitter<CanvasCreated> for GlobalCanvasEvents {}

impl EventEmitter<CanvasRemoved> for GlobalCanvasEvents {}

impl EventEmitter<CurrentCanvasChanged> for GlobalCanvasEvents {}

pub struct CanvasManager {
    canvases: HashMap<CanvasId, Entity<CCanvas>>,
    current_canvas: Option<CanvasId>,
    event_emitter: Entity<GlobalCanvasEvents>,
}

impl Global for CanvasManager {}

impl CanvasManager {
    pub fn new(cx: &mut App) -> Self {
        Self {
            canvases: HashMap::new(),
            current_canvas: None,
            event_emitter: cx.new(|_| GlobalCanvasEvents),
        }
    }

    pub fn add_canvas(&mut self, canvas: CCanvas, cx: &mut App) {
        let id = canvas.id;
        let old = self.current_canvas.replace(id);
        self.canvases.insert(id, cx.new(|_| canvas));
        self.event_emitter.update(cx, |_, cx| {
            cx.emit(CanvasCreated { id });
            cx.emit(CurrentCanvasChanged {
                from: old,
                to: Some(id),
            });
        });
    }

    pub fn remove_canvas(&mut self, id: &CanvasId, cx: &mut App) {
        if self.canvases.remove(id).is_some() {
            if self.current_canvas.as_ref() == Some(id) {
                self.current_canvas = self.canvases.keys().next().copied();
            }
            self.event_emitter.update(cx, |_, cx| {
                cx.emit(CanvasRemoved { id: *id });
                cx.emit(CurrentCanvasChanged {
                    from: Some(*id),
                    to: self.current_canvas.as_ref().copied(),
                });
            });
        }
    }

    pub fn get(&self, id: &CanvasId) -> Option<WeakEntity<CCanvas>> {
        Some(self.canvases.get(id)?.downgrade())
    }

    pub fn read<'a>(&self, id: &CanvasId, cx: &'a App) -> Option<&'a CCanvas> {
        Some(self.canvases.get(id)?.read(cx))
    }

    pub fn update<R>(
        &self,
        id: &CanvasId,
        cx: &mut App,
        update: impl FnOnce(&mut CCanvas, &mut Context<CCanvas>) -> R,
    ) -> Option<R> {
        let canvas = self.canvases.get(id)?;
        Some(canvas.update(cx, update))
    }

    pub fn current(&self) -> Option<WeakEntity<CCanvas>> {
        let cur = self.current_canvas.as_ref()?;
        Some(self.canvases.get(cur)?.downgrade())
    }

    pub fn read_current<'a>(&self, cx: &'a App) -> Option<&'a CCanvas> {
        let cur = self.current_canvas.as_ref()?;
        Some(self.canvases.get(cur)?.read(cx))
    }

    pub fn update_current<R>(
        &mut self,
        cx: &mut App,
        update: impl FnOnce(&mut CCanvas, &mut Context<CCanvas>) -> R,
    ) -> Option<R> {
        let cur = self.current_canvas.as_ref()?;
        let canvas = self.canvases.get(cur).unwrap();
        Some(canvas.update(cx, update))
    }

    pub fn current_id(&self) -> Option<CanvasId> {
        self.current_canvas
    }

    pub fn set_current(&mut self, id: CanvasId, cx: &mut App) {
        let old = self.current_canvas.replace(id);
        self.event_emitter.update(cx, |_, cx| {
            cx.emit(CurrentCanvasChanged {
                from: old,
                to: Some(id),
            });
        });
    }

    pub fn event_emitter(&self) -> Entity<GlobalCanvasEvents> {
        self.event_emitter.clone()
    }
}

pub trait CanvasUndoStackAppExt {
    fn current_canvas_undo_stack(&self) -> Option<&UndoStack>;
    fn current_canvas_undo_stack_mut(&mut self) -> Option<&mut UndoStack>;
    fn undo_stack(&self, id: &CanvasId) -> Option<&UndoStack>;
    fn undo_stack_mut(&mut self, id: &CanvasId) -> Option<&mut UndoStack>;
    fn push_undo_command_to_current<C: UndoCommand>(&mut self, command: C) -> anyhow::Result<()>;
    fn push_undo_command<C: UndoCommand>(
        &mut self,
        id: &CanvasId,
        command: C,
    ) -> anyhow::Result<()>;
    fn push_undo_command_boxed_to_current(
        &mut self,
        command: Box<dyn UndoCommand>,
    ) -> anyhow::Result<()>;
    fn push_undo_command_boxed(
        &mut self,
        id: &CanvasId,
        command: Box<dyn UndoCommand>,
    ) -> anyhow::Result<()>;
}

impl CanvasUndoStackAppExt for App {
    fn current_canvas_undo_stack(&self) -> Option<&UndoStack> {
        self.undo_stack(&self.current_canvas_id()?)
    }

    fn current_canvas_undo_stack_mut(&mut self) -> Option<&mut UndoStack> {
        self.undo_stack_mut(&self.current_canvas_id()?)
    }

    fn undo_stack(&self, id: &CanvasId) -> Option<&UndoStack> {
        self.global::<UndoStacks>().get(id)
    }

    fn undo_stack_mut(&mut self, id: &CanvasId) -> Option<&mut UndoStack> {
        self.global_mut::<UndoStacks>().get_mut(id)
    }

    fn push_undo_command_to_current<C: UndoCommand>(&mut self, command: C) -> anyhow::Result<()> {
        self.push_undo_command_boxed_to_current(Box::new(command))
    }

    fn push_undo_command<C: UndoCommand>(
        &mut self,
        id: &CanvasId,
        command: C,
    ) -> anyhow::Result<()> {
        self.push_undo_command_boxed(id, Box::new(command))
    }

    fn push_undo_command_boxed_to_current(
        &mut self,
        command: Box<dyn UndoCommand>,
    ) -> anyhow::Result<()> {
        let cur = self
            .current_canvas_id()
            .ok_or_else(|| anyhow::anyhow!("No current canvas"))?;
        self.push_undo_command_boxed(&cur, command)
    }

    fn push_undo_command_boxed(
        &mut self,
        id: &CanvasId,
        command: Box<dyn UndoCommand>,
    ) -> anyhow::Result<()> {
        self.update_global::<UndoStacks, _>(|stacks, cx| {
            let stack = stacks
                .get_mut(id)
                .ok_or_else(|| anyhow::anyhow!("Undo stack for canvas {} not found", id))?;
            stack.push_boxed(command, cx)
        })
    }
}
