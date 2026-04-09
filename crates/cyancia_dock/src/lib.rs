pub mod dock;
pub mod group;
pub mod state;
pub mod style;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

pub use dock::{DockAction, DockId, FloatAction, TabEvent};
pub use group::{DockGroupData, TabRowWidget};
use iced::Task;
use iced_core::{
    Element, Layout, Length, Point, Rectangle, Size, layout, mouse, renderer, widget, window,
};
pub use state::DockState;
pub use style::{DockCatalog, DockStatus, DockStyle, TabBarStyle, TabStyle};

use iced_widget::{pane_grid, space};

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

    pub fn on_dock_action(&mut self, action: DockAction) -> Task<DockAction> {
        self.dock_state.update(action, self.cursor_pos)
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
                    info.group.remove_dock(dock_id);
                    if info.group.is_empty() {
                        self.detached.remove(&id);
                        return iced_runtime::window::close(id);
                    }
                }
                TabEvent::Reorder { from, to } => {
                    let dock_id = info.group.iter().nth(from).copied();
                    if let Some(d) = dock_id {
                        info.group.reorder(d, to);
                    }
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
        let Some(split) = self.attach_split_info(id) else {
            return Task::none();
        };

        let Some(info) = self.detached.remove(&id) else {
            return Task::none();
        };

        self.dock_state
            .split(split.pane, split.result_edge, info.group);

        iced_runtime::window::close(id)
    }

    pub fn detach(&mut self, pane: pane_grid::Pane) -> Task<()> {
        let Some(group) = self.dock_state.detach_pane(pane) else {
            return Task::none();
        };
        let win_size = Size::new(400.0, 350.0);
        let (win_id, open_task) = iced_runtime::window::open(window::Settings {
            decorations: false,
            position: window::Position::Specific(self.screen_cursor_pos()),
            size: win_size,
            level: window::Level::AlwaysOnTop,
            platform_specific: window::settings::PlatformSpecific {
                skip_taskbar: true,
                undecorated_shadow: true,
                ..Default::default()
            },
            ..Default::default()
        });
        self.detached.insert(
            win_id,
            GroupWindowInfo {
                id: win_id,
                group,
                position: self.screen_cursor_pos(),
                size: win_size,
                is_dragging: false,
            },
        );
        open_task.then(|id| iced_runtime::window::drag(id))
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

    pub fn attach_split_info(&self, window: window::Id) -> Option<SplitInfo> {
        const SPACING: f32 = 2.0;
        let info = self.detached_window(window)?;
        let node = self.dock_state.panes_state().layout();
        let regions = node.pane_regions(SPACING, 0.0, self.main_window.size);

        let relative_window_pos = Point::new(
            info.position.x + info.size.width / 2.0 - self.main_window.position.x,
            info.position.y + info.size.height / 2.0 - self.main_window.position.y,
        );

        let target = regions
            .iter()
            .find(|(_, r)| r.contains(relative_window_pos))
            .map(|(&pane, r)| {
                let cx = r.x + r.width / 2.0;
                let cy = r.y + r.height / 2.0;

                let edge =
                    if (relative_window_pos.y - cy).abs() > (relative_window_pos.x - cx).abs() {
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

                SplitInfo {
                    pane,
                    result_edge: edge,
                }
            });

        target
    }

    pub fn current_attach_split_info(&self) -> Option<SplitInfo> {
        let Some((window, _, _)) = self.last_overlap else {
            return None;
        };

        self.attach_split_info(window)
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
pub struct SplitInfo {
    pub pane: pane_grid::Pane,
    pub result_edge: pane_grid::Edge,
}

// ── DockWidget ────────────────────────────────────────────────────────────────

/// Top-level widget that renders the docking system backed by `PaneGrid`.
pub struct DockWidget<'a, Message, Theme, Renderer> {
    state: &'a DockState,
    content:
        Option<Box<dyn Fn(pane_grid::Pane, DockId) -> Element<'a, Message, Theme, Renderer> + 'a>>,
    on_action: Box<dyn Fn(DockAction) -> Message + 'a>,
    spacing: f32,
    drag_hint: Option<SplitInfo>,
}

impl<'a, Message: Clone + 'a, Theme, Renderer> DockWidget<'a, Message, Theme, Renderer> {
    pub fn new(state: &'a DockState, on_action: impl Fn(DockAction) -> Message + 'a) -> Self {
        Self {
            state,
            content: None,
            on_action: Box::new(on_action),
            spacing: 2.0,
            drag_hint: None,
        }
    }

    pub fn content(
        mut self,
        f: impl Fn(pane_grid::Pane, DockId) -> Element<'a, Message, Theme, Renderer> + 'a,
    ) -> Self {
        self.content = Some(Box::new(f));
        self
    }

    pub fn spacing(mut self, s: f32) -> Self {
        self.spacing = s;
        self
    }

    pub fn drag_hint(mut self, split_info: SplitInfo) -> Self {
        self.drag_hint = Some(split_info);
        self
    }
}

impl<'a, Message, Theme, Renderer> From<DockWidget<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: iced_widget::container::Catalog
        + iced_widget::pane_grid::Catalog
        + crate::style::DockCatalog
        + 'a,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    fn from(w: DockWidget<'a, Message, Theme, Renderer>) -> Self {
        use std::rc::Rc;

        let DockWidget {
            state,
            content,
            on_action,
            spacing,
            drag_hint,
        } = w;

        let on_action = Rc::<dyn Fn(DockAction) -> Message>::from(on_action);
        let a_content = Rc::clone(&on_action);
        let a_click = Rc::clone(&on_action);
        let a_drag = Rc::clone(&on_action);
        let a_resize = Rc::clone(&on_action);

        let grid =
            pane_grid::PaneGrid::new(state.panes_state(), move |pane, group_data, _maximized| {
                let body = group_data
                    .active()
                    .and_then(|id| content.as_ref().map(|c| c(pane, id)))
                    .unwrap_or_else(|| Element::new(space()));

                let a = Rc::clone(&a_content);
                let tabs = TabRowWidget::new(group_data, move |ev| {
                    let action = match ev {
                        TabEvent::Select(id) => DockAction::TabSelect(pane, id),
                        TabEvent::Close(id) => DockAction::TabClose(pane, id),
                        TabEvent::Reorder { from, to } => DockAction::TabReorder { pane, from, to },
                    };
                    (a.as_ref())(action)
                });
                // No on_title_drag set — non-tab area is pane_grid's drag pick area.

                pane_grid::Content::new(body)
                    .title_bar(pane_grid::TitleBar::new(Element::new(tabs)))
            })
            .on_click(move |p| (a_click.as_ref())(DockAction::PaneClicked(p)))
            .on_drag(move |e| (a_drag.as_ref())(DockAction::PaneDragged(e)))
            .on_resize(5.0, move |e| {
                (a_resize.as_ref())(DockAction::PaneResized(e))
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(spacing)
            .into();

        if let Some(split_info) = drag_hint {
            let overlay = HintOverlay {
                state,
                split_info,
                spacing,
            };
            iced_widget::stack![grid, Element::new(overlay)]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            grid
        }
    }
}

// ── FloatingDockWidget ────────────────────────────────────────────────────────

/// Widget for a detached (floating, borderless) dock group window.
///
/// Renders a tab row at the top and the active dock's content below.
/// Pressing the non-tab title area emits `FloatAction::StartWindowDrag` so the
/// caller can initiate an OS-native window drag via `iced_runtime::window::drag`.
pub struct FloatingDockWidget<'a, Message, Theme, Renderer> {
    group_data: &'a DockGroupData,
    content: Option<Box<dyn Fn(DockId) -> Element<'a, Message, Theme, Renderer> + 'a>>,
    on_action: Box<dyn Fn(FloatAction) -> Message + 'a>,
}

impl<'a, Message, Theme, Renderer> FloatingDockWidget<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
{
    pub fn new(
        group_data: &'a DockGroupData,
        on_action: impl Fn(FloatAction) -> Message + 'a,
    ) -> Self {
        Self {
            group_data,
            content: None,
            on_action: Box::new(on_action),
        }
    }

    pub fn content(
        mut self,
        f: impl Fn(DockId) -> Element<'a, Message, Theme, Renderer> + 'a,
    ) -> Self {
        self.content = Some(Box::new(f));
        self
    }
}

impl<'a, Message, Theme, Renderer> From<FloatingDockWidget<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: crate::style::DockCatalog + 'a,
    Renderer: iced_core::Renderer + iced_core::text::Renderer + 'a,
{
    fn from(w: FloatingDockWidget<'a, Message, Theme, Renderer>) -> Self {
        use std::rc::Rc;

        let FloatingDockWidget {
            group_data,
            content,
            on_action,
        } = w;

        let on_action: Rc<dyn Fn(FloatAction) -> Message + 'a> = Rc::from(on_action);
        let a_tab = Rc::clone(&on_action);
        let a_title_drag = Rc::clone(&on_action);

        let tab_row =
            TabRowWidget::new(group_data, move |ev| (a_tab.as_ref())(FloatAction::Tab(ev)))
                .on_title_drag(move || (a_title_drag.as_ref())(FloatAction::StartWindowDrag));

        let body = group_data
            .active()
            .and_then(|id| content.map(|c| c(id)))
            .unwrap_or_else(|| Element::new(space()));

        iced_widget::column![Element::from(tab_row), body,]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}

/// Transparent overlay widget drawn on top of `DockWidget` to show where a
/// floating window would re-attach (the pane half closest to the hint cursor).
struct HintOverlay<'a> {
    state: &'a DockState,
    split_info: SplitInfo,
    spacing: f32,
}

impl<'a, Message, Theme, Renderer> iced_core::Widget<Message, Theme, Renderer> for HintOverlay<'a>
where
    Renderer: iced_core::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let regions =
            self.state
                .panes_state()
                .layout()
                .pane_regions(self.spacing, 0.0, bounds.size());

        let Some(region) = regions.get(&self.split_info.pane) else {
            return;
        };

        let highlight = match self.split_info.result_edge {
            pane_grid::Edge::Left => iced_core::Rectangle {
                x: bounds.x + region.x,
                y: bounds.y + region.y,
                width: region.width / 2.0,
                height: region.height,
            },
            pane_grid::Edge::Right => iced_core::Rectangle {
                x: bounds.x + region.x + region.width / 2.0,
                y: bounds.y + region.y,
                width: region.width / 2.0,
                height: region.height,
            },
            pane_grid::Edge::Top => iced_core::Rectangle {
                x: bounds.x + region.x,
                y: bounds.y + region.y,
                width: region.width,
                height: region.height / 2.0,
            },
            pane_grid::Edge::Bottom => iced_core::Rectangle {
                x: bounds.x + region.x,
                y: bounds.y + region.y + region.height / 2.0,
                width: region.width,
                height: region.height / 2.0,
            },
        };

        renderer.fill_quad(
            iced_core::renderer::Quad {
                bounds: highlight,
                ..iced_core::renderer::Quad::default()
            },
            iced_core::Background::Color(iced_core::Color {
                r: 0.15,
                g: 0.55,
                b: 1.0,
                a: 0.35,
            }),
        );
    }
}
