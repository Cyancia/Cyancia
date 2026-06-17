use std::{borrow::Cow, time::Instant};

use gpui::App;

#[derive(Default)]
pub struct UndoStack {
    cursor: usize,
    history: Vec<UndoCommandData>,
}

impl UndoStack {
    pub fn push(&mut self, mut cmd: Box<dyn UndoCommand>, cx: &mut App) {
        cmd.redo(cx);

        while self.cursor < self.history.len() {
            let mut data = self.history.pop().unwrap();
            data.command.undo(cx);
        }

        self.history.push(UndoCommandData {
            pushed_at: Instant::now(),
            command: cmd,
        });
        self.cursor = self.len();
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: usize, cx: &mut App) {
        if cursor > self.len() {
            return;
        }

        while self.cursor > cursor {
            let mut data = self.history.pop().unwrap();
            data.command.undo(cx);
        }
        while self.cursor < cursor {
            let mut data = self.history.pop().unwrap();
            data.command.redo(cx);
        }

        self.cursor = cursor;
    }

    pub fn len(&self) -> usize {
        self.history.len()
    }

    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

pub struct UndoCommandData {
    // TODO: Merge commands based on this.
    pushed_at: Instant,
    command: Box<dyn UndoCommand>,
}

pub trait UndoCommand {
    fn label(&self) -> Cow<'static, str>;
    fn undo(&mut self, cx: &mut App);
    fn redo(&mut self, cx: &mut App);
}
