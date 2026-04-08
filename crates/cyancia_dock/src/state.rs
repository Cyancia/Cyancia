use crate::{dock::DockAction, group::DockGroupData};
use iced_widget::pane_grid;
use iced_runtime::Task;

/// Root state for the docking system.
///
/// Wraps `pane_grid::State<DockGroupData>` — all layout is handled by pane_grid.
#[derive(Debug)]
pub struct DockState {
    pub panes: pane_grid::State<DockGroupData>,
}

impl DockState {
    /// Create a new state with one initial group.
    pub fn new(initial: DockGroupData) -> (Self, pane_grid::Pane) {
        let (panes, pane) = pane_grid::State::new(initial);
        (Self { panes }, pane)
    }

    /// Create from a declarative `pane_grid::Configuration`.
    pub fn from_config(config: impl Into<pane_grid::Configuration<DockGroupData>>) -> Self {
        Self { panes: pane_grid::State::with_configuration(config) }
    }

    /// Split a pane along the given axis.
    pub fn split(
        &mut self,
        axis: pane_grid::Axis,
        pane: pane_grid::Pane,
        new_group: DockGroupData,
    ) -> Option<(pane_grid::Pane, pane_grid::Split)> {
        self.panes.split(axis, pane, new_group)
    }

    /// Remove a pane from the grid and return its `DockGroupData`.
    ///
    /// Returns `None` if the pane does not exist. When the last pane is removed
    /// `pane_grid::State` returns `None` from `close()` — in that case the group
    /// data is lost; callers should guard against this if needed.
    pub fn detach_pane(&mut self, pane: pane_grid::Pane) -> Option<DockGroupData> {
        self.panes.close(pane).map(|(data, _adjacent)| data)
    }

    /// Apply a `DockAction` and return any resulting `Task`.
    pub fn update(&mut self, action: DockAction) -> Task<DockAction> {
        match action {
            DockAction::PaneClicked(_) => {}

            DockAction::PaneDragged(event) => {
                if let pane_grid::DragEvent::Dropped { pane, target } = event {
                    self.panes.drop(pane, target);
                }
            }

            DockAction::PaneResized(event) => {
                self.panes.resize(event.split, event.ratio);
            }

            DockAction::TabSelect(pane, dock_id) => {
                if let Some(group) = self.panes.get_mut(pane) {
                    group.set_active(dock_id);
                }
            }

            DockAction::TabClose(pane, dock_id) => {
                if let Some(group) = self.panes.get_mut(pane) {
                    group.remove_dock(dock_id);
                    if group.is_empty() {
                        self.panes.close(pane);
                    }
                }
            }

            DockAction::TabReorder { pane, from, to } => {
                if let Some(group) = self.panes.get_mut(pane) {
                    let dock_id = group.iter().nth(from).copied();
                    if let Some(dock_id) = dock_id {
                        group.reorder(dock_id, to);
                    }
                }
            }
        }
        Task::none()
    }
}
