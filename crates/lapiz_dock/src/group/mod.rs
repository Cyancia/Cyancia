pub mod tab_row;

pub use tab_row::TabRowWidget;

use crate::dock::DockId;
use indexmap::IndexSet;
use serde::Serialize;

#[derive(Debug, Default, Clone, Serialize)]
pub struct DockGroupData {
    docks: IndexSet<DockId>,
    active: Option<DockId>,
}

impl DockGroupData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_dock(&mut self, dock_id: DockId) {
        self.docks.insert(dock_id.clone());
        if self.active.is_none() {
            self.active = Some(dock_id);
        }
    }

    pub fn remove_dock(&mut self, dock_id: &DockId) {
        if self.active.as_ref().is_some_and(|active| active == dock_id) {
            let (index, _) = self.docks.shift_remove_full(dock_id).unwrap();
            if self.docks.is_empty() {
                self.active = None;
                return;
            }

            self.active = self.docks.get_index(index.saturating_sub(1)).cloned();
        } else {
            self.docks.shift_remove(dock_id);
        }
    }

    pub fn set_active(&mut self, dock_id: DockId) {
        if self.docks.contains(&dock_id) {
            self.active = Some(dock_id);
        }
    }

    pub fn reorder(&mut self, dock_id: DockId, index: usize) {
        self.docks.shift_insert(index, dock_id);
    }

    pub fn active(&self) -> Option<&DockId> {
        self.active.as_ref()
    }

    pub fn is_empty(&self) -> bool {
        self.docks.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &DockId> {
        self.docks.iter()
    }

    pub fn len(&self) -> usize {
        self.docks.len()
    }

    pub fn extend(&mut self, other: DockGroupData) {
        self.docks.extend(other.docks);
    }
}
