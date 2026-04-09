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
const MERGE_DISTANCE: f32 = 30.0;

pub struct DockManager {
    main_window: GroupWindowInfo,
    dock_state: DockState,
    detached: HashMap<window::Id, GroupWindowInfo>,
    cursor_pos: Option<(window::Id, Point)>,
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
                last_overlap: None,
            },
            dock_state,
            detached: HashMap::new(),
            cursor_pos: None,
        }
    }

    pub fn on_dock_action(&mut self, action: DockAction) -> Task<()> {
        match action {
            DockAction::Pane(event) => {
                if let Some(cursor_pos) = self.screen_cursor_pos() {
                    self.dock_state.update(
                        event,
                        Point::new(
                            cursor_pos.x - self.main_window.position.x,
                            cursor_pos.y - self.main_window.position.y,
                        ),
                    )
                }
            }
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
                            return match self.detach_group(new_group) {
                                Some((_, task)) => task.discard(),
                                None => Task::none(),
                            };
                        }
                    }
                }
            }
        }

        Task::none()
    }

    pub fn on_cursor_moved(&mut self, window: window::Id, pos: Point) -> Task<()> {
        self.cursor_pos = Some((window, pos));

        if let Some(pane) = self.dock_state.try_detach(pos) {
            self.detach(pane)
        } else {
            Task::none()
        }
    }

    pub fn on_float_window_drag_end(&mut self) -> Task<()> {
        let mut try_attach_or_merge = None;
        for (id, info) in &mut self.detached {
            if !info.is_dragging {
                continue;
            }

            info.is_dragging = false;
            try_attach_or_merge = Some((*id, info.last_overlap.take()));
        }

        let Some((src_window, Some((overlap_dst_window, overlap_since, _)))) = try_attach_or_merge
        else {
            return Task::none();
        };

        if overlap_since.elapsed() > ATTACH_DWELL {
            if overlap_dst_window == self.main_window.id {
                return self.attach_to_main(src_window);
            } else {
                return self.merge_floating(src_window, overlap_dst_window);
            }
        }

        Task::none()
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

                    if info
                        .last_overlap
                        .is_none_or(|(_, _, p)| p.distance(pos) > 10.0)
                    {
                        let mut next_dst = None;
                        if let Some(dst_id) = self.floating_merge_info(id) {
                            next_dst = Some(dst_id);
                        } else if self.is_over_main_window(id) {
                            next_dst.get_or_insert(self.main_window.id);
                        }

                        self.detached.get_mut(&id).unwrap().last_overlap =
                            next_dst.map(|dst| (dst, Instant::now(), pos));
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
                    info.group.remove_dock(&dock_id);
                    let mut new_group = DockGroupData::new();
                    new_group.add_dock(dock_id);
                    return match self.detach_group(new_group) {
                        Some((_, task)) => task.discard(),
                        None => Task::none(),
                    };
                }
            },
            FloatAction::StartWindowDrag => {
                info.is_dragging = true;
                return iced_runtime::window::drag(id);
            }
        }

        Task::none()
    }

    pub fn attach_to_main(&mut self, id: window::Id) -> Task<()> {
        let Some(attach) = self.main_attach_info(id) else {
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

    pub fn merge_floating(&mut self, src: window::Id, dst: window::Id) -> Task<()> {
        let Some(src_info) = self.detached.remove(&src) else {
            return Task::none();
        };
        let Some(dst_info) = self.detached.get_mut(&dst) else {
            return Task::none();
        };

        for dock in src_info.group.iter() {
            dst_info.group.add_dock(dock.clone());
        }

        iced_runtime::window::close(src)
    }

    pub fn detach(&mut self, pane: pane_grid::Pane) -> Task<()> {
        let Some(group) = self.dock_state.close(pane) else {
            return Task::none();
        };

        if let Some((_, task)) = self.detach_group(group) {
            task.discard()
        } else {
            Task::none()
        }
    }

    fn detach_group(&mut self, group: DockGroupData) -> Option<(window::Id, Task<window::Id>)> {
        let window_size = Size::new(400.0, 350.0);
        let (window_id, open_task) = iced_runtime::window::open(window::Settings {
            decorations: false,
            position: window::Position::Specific(self.screen_cursor_pos()?),
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
                position: self.screen_cursor_pos()?,
                size: window_size,
                is_dragging: true,
                last_overlap: None,
            },
        );

        Some((
            window_id,
            open_task.then(|id| iced_runtime::window::drag(id)),
        ))
    }

    pub fn screen_cursor_pos(&self) -> Option<Point> {
        let (window, cursor) = self.cursor_pos?;

        if window == self.main_window.id {
            Some(Point::new(
                self.main_window.position.x + cursor.x,
                self.main_window.position.y + cursor.y,
            ))
        } else if let Some(info) = self.detached.get(&window) {
            Some(Point::new(
                info.position.x + cursor.x,
                info.position.y + cursor.y,
            ))
        } else {
            None
        }
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

    pub fn main_attach_info(&self, window: window::Id) -> Option<AttachInfo> {
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

    pub fn floating_merge_info(&self, src_window: window::Id) -> Option<window::Id> {
        let info = self.detached_window(src_window)?;
        let src_center = Point::new(
            info.position.x + info.size.width / 2.0,
            info.position.y + info.size.height / 2.0,
        );

        for (dst_id, dst_window) in &self.detached {
            if *dst_id == src_window {
                continue;
            }

            let dst_center = Point::new(
                dst_window.position.x + dst_window.size.width / 2.0,
                dst_window.position.y + dst_window.size.height / 2.0,
            );

            if src_center.distance(dst_center) < MERGE_DISTANCE {
                return Some(*dst_id);
            }
        }

        None
    }

    pub fn current_attach_or_merge_info(&self) -> Option<AttachOrMergeInfo> {
        let dragging = self.detached.values().find(|info| info.is_dragging)?;

        let src_id = dragging.id;
        let dst_id = dragging.last_overlap?.0;
        if dst_id == self.main_window.id {
            self.main_attach_info(src_id).map(AttachOrMergeInfo::Attach)
        } else {
            Some(AttachOrMergeInfo::Merge { dst: dst_id })
        }
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
    pub last_overlap: Option<(window::Id, std::time::Instant, Point)>,
}

#[derive(Debug, Clone)]
pub enum AttachOrMergeInfo {
    Attach(AttachInfo),
    Merge { dst: window::Id },
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
