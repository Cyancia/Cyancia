use cyancia_utils::wrapper;
use iced_widget::pane_grid;
use parse_display::Display;
use serde::Serialize;

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, Serialize)]
    #[display("{0}")]
    pub DockId : &'static str
}

/// Events the docking system can emit.
#[derive(Debug, Clone)]
pub enum DockAction {
    // ── Pane-level (forwarded from PaneGrid) ──
    PaneClicked(pane_grid::Pane),
    PaneDragged(pane_grid::DragEvent),
    PaneResized(pane_grid::ResizeEvent),

    // ── Tab-level ──
    TabSelect(pane_grid::Pane, DockId),
    TabClose(pane_grid::Pane, DockId),
    TabReorder {
        pane: pane_grid::Pane,
        from: usize,
        to: usize,
    },
}

/// Low-level tab events emitted by `TabRowWidget` (no pane coupling).
///
/// `DockWidget` maps these back to `DockAction` variants. `FloatingDockWidget` wraps them
/// inside `FloatAction::Tab`.
#[derive(Debug, Clone)]
pub enum TabEvent {
    Select(DockId),
    Close(DockId),
    Reorder { from: usize, to: usize },
}

/// Actions emitted by `FloatingDockWidget`.
#[derive(Debug, Clone)]
pub enum FloatAction {
    Tab(TabEvent),
    /// User pressed the non-tab title area — the app should call `window::drag(id)`.
    StartWindowDrag,
}
