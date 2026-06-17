use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    time::Instant,
};

use cyancia_utils::{Deref, DerefMut, log_err::LogErr};
use downcast_rs::Downcast;
use gpui::{App, Global};
use tracing::info;
use uuid::Uuid;

pub fn init(cx: &mut App) {
    cx.set_global(UndoStacks::default());
}

#[derive(Default, Deref, DerefMut)]
pub struct UndoStacks {
    stacks: HashMap<Uuid, UndoStack>,
}

impl Global for UndoStacks {}

pub struct UndoStack {
    cursor: usize,
    history: VecDeque<UndoCommandData>,
    max_history: usize,
}

impl UndoStack {
    pub fn new(max_history: usize) -> Self {
        Self {
            cursor: 0,
            history: VecDeque::new(),
            max_history,
        }
    }

    pub fn push<C: UndoCommand>(&mut self, cmd: C, cx: &mut App) -> anyhow::Result<()> {
        self.push_boxed(Box::new(cmd), cx)
    }

    pub fn push_boxed(
        &mut self,
        mut cmd: Box<dyn UndoCommand>,
        cx: &mut App,
    ) -> anyhow::Result<()> {
        info!("Push command {}", cmd.label());
        self.history.truncate(self.cursor);

        if let Some(rhs) = self.history.back()
            && rhs.command.can_cancel_out(cmd.as_ref())
        {
            cmd.redo(cx).logged_err()?;
            self.history.pop_back();
        } else {
            if self.history.len() == self.max_history {
                self.history.pop_front();
            }

            cmd.redo(cx).logged_err()?;
            self.history.push_back(UndoCommandData {
                _pushed_at: Instant::now(),
                command: cmd,
            });
        }

        self.cursor = self.len();
        Ok(())
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize, cx: &mut App) -> anyhow::Result<()> {
        if cursor > self.len() {
            return Err(anyhow::anyhow!(
                "cursor {} out of bounds {}",
                cursor,
                self.len()
            ));
        }

        while self.cursor < cursor {
            self.cursor += 1;

            let data = &mut self.history[self.cursor - 1];
            info!("Redo {}", data.command.label());
            data.command.redo(cx).logged_err()?;
        }
        while self.cursor > cursor {
            self.cursor -= 1;

            let data = &mut self.history[self.cursor];
            info!("Undo {}", data.command.label());
            data.command.undo(cx).logged_err()?;
        }

        self.cursor = cursor;

        Ok(())
    }

    pub fn undo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        if self.cursor == 0 {
            return Err(anyhow::anyhow!("undo stack is empty"));
        }
        self.set_cursor(self.cursor - 1, cx)
    }

    pub fn redo(&mut self, cx: &mut App) -> anyhow::Result<()> {
        if self.cursor == self.len() {
            return Err(anyhow::anyhow!("undo stack reached the end"));
        }
        self.set_cursor(self.cursor + 1, cx)
    }

    pub fn len(&self) -> usize {
        self.history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

impl std::fmt::Debug for UndoStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UndoStack")
            .field("cursor", &self.cursor)
            .field("len", &self.len())
            .field(
                "history",
                &self
                    .history
                    .iter()
                    .map(|data| data.command.label())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

pub struct UndoCommandData {
    // TODO: Merge commands based on this.
    _pushed_at: Instant,
    command: Box<dyn UndoCommand>,
}

pub trait UndoCommand: 'static + Downcast {
    fn label(&self) -> Cow<'static, str>;
    fn redo(&mut self, cx: &mut App) -> anyhow::Result<()>;
    fn undo(&mut self, cx: &mut App) -> anyhow::Result<()>;
    fn can_cancel_out(&self, _rhs: &dyn UndoCommand) -> bool {
        false
    }
}
downcast_rs::impl_downcast!(UndoCommand);
