use cyancia_canvas::CanvasAppExt;
use cyancia_undo::UndoStacks;
use cyancia_utils::log_err::LogErr;
use gpui::{App, BorrowAppContext, actions};

use crate::ActionFunction;

actions!([UndoAction, RedoAction]);

impl ActionFunction for UndoAction {
    fn trigger(&self, cx: &mut App) {
        let Some(cur_canvas) = cx.current_canvas_id() else {
            return;
        };
        cx.update_global::<UndoStacks, _>(|stacks, cx| {
            if let Some(stack) = stacks.get_mut(&*cur_canvas) {
                stack.undo(cx).log_err();
            }
        });
    }
}

impl ActionFunction for RedoAction {
    fn trigger(&self, cx: &mut App) {
        let Some(cur_canvas) = cx.current_canvas_id() else {
            return;
        };
        cx.update_global::<UndoStacks, _>(|stacks, cx| {
            if let Some(stack) = stacks.get_mut(&*cur_canvas) {
                stack.redo(cx).log_err();
            }
        });
    }
}
