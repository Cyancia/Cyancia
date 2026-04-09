pub mod dock;
pub mod group;
pub mod state;
pub mod style;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use dock::{DockAction, DockId, FloatAction, TabEvent};
use group::{DockGroupData, TabRowWidget};
use iced::Task;
use iced_core::{
    Element, Layout, Length, Point, Rectangle, Size, layout, mouse, renderer, widget, window,
};
use state::DockState;
use style::{DockCatalog, DockStatus, DockStyle, TabBarStyle, TabStyle};

use iced_widget::{pane_grid, space};

use crate::dock::PaneEvent;

const ATTACH_DWELL: Duration = Duration::from_millis(200);

pub struct DockManager {
    main_window: GroupWindowInfo,
    dock_state: DockState,
    detached: HashMap<window::Id, GroupWindowInfo>,
    cursor_pos: Point,
    last_overlap: Option<(window::Id, std::time::Instant, Point)>,
}

impl DockManager {
    pub fn new(main_window: window::Id, dock_state: DockState) -> Self {
        Self {
            main_window: GroupWindowInfo {
                id: main_window,
                position: Point::ORIGIN,
                size: Size::ZERO,
                group: DockGroupData::new(),
                is_dragging: false,
            },
            dock_state,
            detached: HashMap::new(),
            cursor_pos: Point::default(),
            last_overlap: None,
        }
    }

    pub fn on_dock_action(&mut self, action: DockAction) -> Task<()> {
        match action {
            DockAction::Pane(event) => self.dock_state.update(event, self.cursor_pos),
            DockAction::Tab(pane, tab_event) => {
                let pane_state = self.dock_state.panes_state_mut();
                match tab_event {
                    TabEvent::Select(dock_id) => {
                        if let Some(group) = pane_state.get_mut(pane) {
                            group.set_active(dock_id);
                        }
                    }
                    TabEvent::Close(dock_id) => {
                        if let Some(group) = pane_state.get_mut(pane) {
                            group.remove_dock(&dock_id);
                            if group.is_empty() {
                                pane_state.close(pane);
                            }
                        }
                    }
                    TabEvent::Reorder { from, to } => {
                        if let Some(group) = pane_state.get_mut(pane) {
                            let dock_id = group.iter().nth(from).cloned();
                            if let Some(dock_id) = dock_id {
                                group.reorder(dock_id, to);
                            }
                        }
                    }
                    TabEvent::Detach(dock_id) => {
                        if let Some(group) = pane_state.get_mut(pane) {
                            group.remove_dock(&dock_id);
                            if group.is_empty() {
                                pane_state.close(pane);
                            }

                            let mut new_group = DockGroupData::new();
                            new_group.add_dock(dock_id);
                            return self.detach_group(new_group).1.discard();
                        }
                    }
                }
            }
        }

        Task::none()
    }

    pub fn on_main_window_cursor_moved(&mut self, pos: Point) -> Task<()> {
        self.cursor_pos = pos;
        if let Some(pane) = self.dock_state.try_detach(pos) {
            self.detach(pane)
        } else {
            Task::none()
        }
    }

    pub fn on_float_window_drag_end(&mut self) -> Task<()> {
        let mut try_detach = None;
        for (id, info) in &mut self.detached {
            if !info.is_dragging {
                continue;
            }

            info.is_dragging = false;
            try_detach = Some(*id);
        }

        let Some(detach_window) = try_detach else {
            return Task::none();
        };

        if let Some((window, last_overlap, _)) = self.last_overlap.take()
            && last_overlap.elapsed() > ATTACH_DWELL
            && window == detach_window
        {
            self.attach(window)
        } else {
            Task::none()
        }
    }

    pub fn on_window_event(&mut self, id: window::Id, event: window::Event) -> Task<()> {
        match event {
            window::Event::Opened { position, size } => {
                if id == self.main_window.id {
                    self.main_window.position = position.unwrap_or(Point::ORIGIN);
                    self.main_window.size = size;
                }
            }
            window::Event::Moved(pos) => {
                if id == self.main_window.id {
                    self.main_window.position = pos;
                } else if let Some(info) = self.detached.get_mut(&id) {
                    info.position = pos;
                    if self
                        .last_overlap
                        .is_none_or(|(_, _, p)| p.distance(pos) > 10.0)
                    {
                        if overlaps(
                            info.position,
                            info.size,
                            self.main_window.position,
                            self.main_window.size,
                        ) {
                            self.last_overlap = Some((id, Instant::now(), pos));
                        } else {
                            self.last_overlap = None;
                        }
                    }
                }
            }
            window::Event::Resized(size) => {
                if id == self.main_window.id {
                    self.main_window.size = size;
                } else if let Some(info) = self.detached.get_mut(&id) {
                    info.size = size;
                }
            }
            window::Event::Closed => {
                self.detached.remove(&id);
            }
            _ => {}
        }
        Task::none()
    }

    pub fn on_float_action(&mut self, id: window::Id, action: FloatAction) -> Task<()> {
        let Some(info) = self.detached.get_mut(&id) else {
            return Task::none();
        };

        match action {
            FloatAction::Tab(tab_event) => match tab_event {
                TabEvent::Select(dock_id) => {
                    info.group.set_active(dock_id);
                }
                TabEvent::Close(dock_id) => {
                    info.group.remove_dock(&dock_id);
                    if info.group.is_empty() {
                        self.detached.remove(&id);
                        return iced_runtime::window::close(id);
                    }
                }
                TabEvent::Reorder { from, to } => {
                    let dock_id = info.group.iter().nth(from);
                    if let Some(d) = dock_id {
                        info.group.reorder(d.clone(), to);
                    }
                }
                TabEvent::Detach(dock_id) => {
                    let mut new_group = DockGroupData::new();
                    new_group.add_dock(dock_id);
                    return self.detach_group(new_group).1.discard();
                }
            },
            FloatAction::StartWindowDrag => {
                info.is_dragging = true;
                return iced_runtime::window::drag(id);
            }
        }

        Task::none()
    }

    pub fn attach(&mut self, id: window::Id) -> Task<()> {
        let Some(attach) = self.attach_info(id) else {
            return Task::none();
        };

        let Some(info) = self.detached.remove(&id) else {
            return Task::none();
        };

        match attach {
            AttachInfo::Split { pane, result_edge } => {
                self.dock_state.split(pane, result_edge, info.group);
            }
            AttachInfo::Merge { pane } => {
                if let Some(group) = self.dock_state.panes_state_mut().get_mut(pane) {
                    for dock in info.group.iter() {
                        group.add_dock(dock.clone());
                    }
                }
            }
        }

        iced_runtime::window::close(id)
    }

    pub fn detach(&mut self, pane: pane_grid::Pane) -> Task<()> {
        let Some(group) = self.dock_state.close(pane) else {
            return Task::none();
        };

        self.detach_group(group).1.discard()
    }

    fn detach_group(&mut self, group: DockGroupData) -> (window::Id, Task<window::Id>) {
        let window_size = Size::new(400.0, 350.0);
        let (window_id, open_task) = iced_runtime::window::open(window::Settings {
            decorations: false,
            position: window::Position::Specific(self.screen_cursor_pos()),
            size: window_size,
            level: window::Level::AlwaysOnTop,
            platform_specific: window::settings::PlatformSpecific {
                skip_taskbar: true,
                undecorated_shadow: true,
                ..Default::default()
            },
            ..Default::default()
        });
        self.detached.insert(
            window_id,
            GroupWindowInfo {
                id: window_id,
                group,
                position: self.screen_cursor_pos(),
                size: window_size,
                is_dragging: true,
            },
        );
        (
            window_id,
            open_task.then(|id| iced_runtime::window::drag(id)),
        )
    }

    fn screen_cursor_pos(&self) -> Point {
        Point::new(
            self.cursor_pos.x + self.main_window.position.x,
            self.cursor_pos.y + self.main_window.position.y,
        )
    }

    pub fn is_over_main_window(&self, id: window::Id) -> bool {
        if id == self.main_window.id {
            return true;
        }

        if let Some(info) = self.detached.get(&id) {
            return overlaps(
                info.position,
                info.size,
                self.main_window.position,
                self.main_window.size,
            );
        }

        false
    }

    pub fn main_window(&self) -> &GroupWindowInfo {
        &self.main_window
    }

    pub fn detached_window(&self, id: window::Id) -> Option<&GroupWindowInfo> {
        self.detached.get(&id)
    }

    pub fn window_info(&self, id: window::Id) -> Option<&GroupWindowInfo> {
        if id == self.main_window.id {
            Some(&self.main_window)
        } else {
            self.detached.get(&id)
        }
    }

    pub fn dock_state(&self) -> &DockState {
        &self.dock_state
    }

    pub fn attach_info(&self, window: window::Id) -> Option<AttachInfo> {
        const SPACING: f32 = 2.0;
        let info = self.detached_window(window)?;
        let node = self.dock_state.panes_state().layout();
        let regions = node.pane_regions(SPACING, 0.0, self.main_window.size);

        let relative_window_pos = Point::new(
            info.position.x + info.size.width / 2.0 - self.main_window.position.x,
            info.position.y + info.size.height / 2.0 - self.main_window.position.y,
        );
        let half_window_size = Size::new(info.size.width / 2.0, info.size.height / 2.0);

        let target = regions
            .iter()
            .find(|(_, r)| r.contains(relative_window_pos))
            .map(|(&pane, r)| {
                let cx = r.x + r.width / 2.0;
                let cy = r.y + r.height / 2.0;

                if (relative_window_pos.x - cx).abs() < half_window_size.width
                    && (relative_window_pos.y - cy).abs() < half_window_size.height
                {
                    AttachInfo::Merge { pane }
                } else {
                    let edge = if (relative_window_pos.y - cy).abs()
                        > (relative_window_pos.x - cx).abs()
                    {
                        if relative_window_pos.y < cy {
                            pane_grid::Edge::Top
                        } else {
                            pane_grid::Edge::Bottom
                        }
                    } else {
                        if relative_window_pos.x < cx {
                            pane_grid::Edge::Left
                        } else {
                            pane_grid::Edge::Right
                        }
                    };

                    AttachInfo::Split {
                        pane,
                        result_edge: edge,
                    }
                }
            });

        target
    }

    pub fn current_attach_info(&self) -> Option<AttachInfo> {
        let Some((window, _, _)) = self.last_overlap else {
            return None;
        };

        self.attach_info(window)
    }
}

fn overlaps(pos_a: Point, size_a: Size, pos_b: Point, size_b: Size) -> bool {
    pos_a.x < pos_b.x + size_b.width
        && pos_a.x + size_a.width > pos_b.x
        && pos_a.y < pos_b.y + size_b.height
        && pos_a.y + size_a.height > pos_b.y
}

#[derive(Debug, Clone)]
pub struct GroupWindowInfo {
    pub id: window::Id,
    pub position: Point,
    pub size: Size,
    pub group: DockGroupData,
    pub is_dragging: bool,
}

#[derive(Debug, Clone)]
pub enum AttachInfo {
    Split {
        pane: pane_grid::Pane,
        result_edge: pane_grid::Edge,
    },
    Merge {
        pane: pane_grid::Pane,
    },
}

impl AttachInfo {
    pub fn target_pane(&self) -> pane_grid::Pane {
        match self {
            AttachInfo::Split { pane, .. } => *pane,
            AttachInfo::Merge { pane } => *pane,
        }
    }
}
