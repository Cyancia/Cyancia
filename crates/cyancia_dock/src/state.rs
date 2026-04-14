use crate::{
    AttachInfo,
    dock::{DockAction, PaneEvent},
    group::DockGroupData,
};
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
}

impl DockState {
    /// Create a new state with one initial group.
    pub fn new(initial: DockGroupData) -> (Self, pane_grid::Pane) {
        let (panes, pane) = pane_grid::State::new(initial);
        (
            Self {
                panes,
            },
            pane,
        )
    }

    /// Create from a declarative `pane_grid::Configuration`.
    pub fn from_config(config: impl Into<pane_grid::Configuration<DockGroupData>>) -> Self {
        Self {
            panes: pane_grid::State::with_configuration(config),
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

    pub fn close(&mut self, pane: pane_grid::Pane) -> Option<DockGroupData> {
        self.panes.close(pane).map(|(data, _adjacent)| data)
    }

    /// Apply a `DockAction` and return any resulting `Task`.
    pub fn update(&mut self, action: PaneEvent) {
        match action {
            PaneEvent::Clicked(_) => {}
            PaneEvent::Resized(event) => {
                self.panes.resize(event.split, event.ratio);
            }
        }
    }

    pub fn panes_state(&self) -> &pane_grid::State<DockGroupData> {
        &self.panes
    }

    pub fn panes_state_mut(&mut self) -> &mut pane_grid::State<DockGroupData> {
        &mut self.panes
    }
}
