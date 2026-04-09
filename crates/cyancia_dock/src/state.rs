use crate::{SplitInfo, dock::DockAction, group::DockGroupData};
use iced::Point;
use iced_runtime::Task;
use iced_widget::pane_grid;

// This should not be larger than DRAG_DEADBAND_DISTANCE inside iced.
// Or the pane will be dragged inside main window for a while before it detaches.
const DRAG_DEADBAND_DISTANCE: f32 = 10.0;

/// Root state for the docking system.
///
/// Wraps `pane_grid::State<DockGroupData>` — all layout is handled by pane_grid.
#[derive(Debug)]
pub struct DockState {
    panes: pane_grid::State<DockGroupData>,
    pending_detach: Option<(pane_grid::Pane, Point)>,
}

impl DockState {
    /// Create a new state with one initial group.
    pub fn new(initial: DockGroupData) -> (Self, pane_grid::Pane) {
        let (panes, pane) = pane_grid::State::new(initial);
        (
            Self {
                panes,
                pending_detach: None,
            },
            pane,
        )
    }

    /// Create from a declarative `pane_grid::Configuration`.
    pub fn from_config(config: impl Into<pane_grid::Configuration<DockGroupData>>) -> Self {
        Self {
            panes: pane_grid::State::with_configuration(config),
            pending_detach: None,
        }
    }

    /// Split a pane along the given axis.
    pub fn split(
        &mut self,
        pane: pane_grid::Pane,
        result_edge: pane_grid::Edge,
        new_group: DockGroupData,
    ) -> Option<pane_grid::Pane> {
        let (new_pane, _) = self.panes.split(
            match result_edge {
                pane_grid::Edge::Top => pane_grid::Axis::Horizontal,
                pane_grid::Edge::Bottom => pane_grid::Axis::Horizontal,
                pane_grid::Edge::Left => pane_grid::Axis::Vertical,
                pane_grid::Edge::Right => pane_grid::Axis::Vertical,
            },
            pane,
            new_group,
        )?;

        match result_edge {
            pane_grid::Edge::Top | pane_grid::Edge::Left => {
                self.panes.swap(pane, new_pane);
            }
            pane_grid::Edge::Bottom | pane_grid::Edge::Right => {}
        };

        Some(pane)
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
    pub fn update(&mut self, action: DockAction, cursor_pos: Point) -> Task<DockAction> {
        match action {
            DockAction::PaneClicked(_) => {}

            DockAction::PaneDragged(event) => match event {
                pane_grid::DragEvent::Picked { pane } => {
                    self.pending_detach = Some((pane, cursor_pos));
                }
                pane_grid::DragEvent::Dropped { pane, target } => {
                    self.panes.drop(pane, target);
                }
                pane_grid::DragEvent::Canceled { .. } => {
                    self.pending_detach = None;
                }
            },

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

    pub fn try_detach(&mut self, cursor_pos: Point) -> Option<pane_grid::Pane> {
        if let Some((pane, point)) = self.pending_detach.take() {
            if point.distance(cursor_pos) > DRAG_DEADBAND_DISTANCE {
                return Some(pane);
            } else {
                self.pending_detach = Some((pane, point));
            }
        }

        None
    }

    pub fn panes_state(&self) -> &pane_grid::State<DockGroupData> {
        &self.panes
    }
}
