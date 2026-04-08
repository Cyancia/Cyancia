use std::collections::HashMap;
use std::time::{Duration, Instant};

use iced::{Element, Subscription, Task};
use iced_core::window::{self, Id as WindowId, Position};
use iced_core::{Point, Size};
use iced_runtime::window as win;
use iced_widget::pane_grid;

use cyancia_dock::{
    DockAction, DockGroupData, DockId, DockState, DockWidget,
    FloatAction, FloatingDockWidget, TabEvent,
};

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> iced::Result {
    iced::daemon(App::new, App::update, App::view)
        .title(App::title)
        .theme(app_theme)
        .subscription(App::subscription)
        .run()
}

fn app_theme(_: &App, _: WindowId) -> iced::Theme {
    iced::Theme::Dark
}

fn dock(s: &'static str) -> DockId {
    DockId::from(s)
}

/// Pixels the cursor must travel after `Picked` before a detach is triggered.
const DETACH_THRESHOLD: f32 = 10.0;

// ── App state ─────────────────────────────────────────────────────────────────

struct App {
    dock_state: DockState,
    main_window_id: WindowId,
    /// Position of the main window on screen (logical coords).
    main_window_pos: Point,
    /// Size of the main window client area (logical coords).
    main_window_size: Size,
    /// Cursor position within the main window (logical coords).
    cursor_pos: Point,
    /// Currently detached pane windows.
    detached: HashMap<WindowId, DetachedInfo>,
    /// Pane that was just picked but hasn't been detached yet (waiting for
    /// the cursor to travel `DETACH_THRESHOLD` pixels).
    pending_detach: Option<(pane_grid::Pane, Point)>,
}

struct DetachedInfo {
    group: DockGroupData,
    window_pos: Point,
    window_size: Size,
    /// Last time this window moved (for the 200 ms stillness check).
    last_move: Instant,
}

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Message {
    /// Actions from the main DockWidget.
    Dock(DockAction),
    /// Actions from a floating window.
    Float { id: WindowId, action: FloatAction },
    /// OS window event.
    WinEvent(WindowId, window::Event),
    /// Cursor moved (window-relative, from the main window).
    CursorMoved(Point),
    /// Floating window finished opening — initiate OS-native drag.
    FloatReady(WindowId),
    /// Periodic timer to check whether any floating pane should re-attach.
    ReattachTick,
    Noop,
}

// ── Boot ──────────────────────────────────────────────────────────────────────

impl App {
    fn new() -> (Self, Task<Message>) {
        let (main_id, open_task) = win::open(window::Settings {
            size: Size::new(1024.0, 768.0),
            ..Default::default()
        });

        let left = DockGroupData::with_docks([
            dock("Properties"),
            dock("Timeline"),
            dock("Layers"),
        ]);
        let (mut dock_state, left_pane) = DockState::new(left);
        let right = DockGroupData::with_docks([
            dock("Viewport"),
            dock("Assets"),
            dock("Console"),
        ]);
        dock_state.split(pane_grid::Axis::Vertical, left_pane, right);

        let app = App {
            dock_state,
            main_window_id: main_id,
            main_window_pos: Point::ORIGIN,
            main_window_size: Size::new(1024.0, 768.0),
            cursor_pos: Point::ORIGIN,
            detached: HashMap::new(),
            pending_detach: None,
        };

        (app, open_task.map(|_| Message::Noop))
    }

    fn title(&self, window_id: WindowId) -> String {
        if window_id == self.main_window_id {
            "Docking Test".to_string()
        } else {
            self.detached
                .get(&window_id)
                .and_then(|info| info.group.active())
                .map(|id| id.to_string())
                .unwrap_or_else(|| "Floating".to_string())
        }
    }
}

// ── Update ────────────────────────────────────────────────────────────────────

impl App {
    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            // ── Picked: arm the detach, wait for threshold ────────────────────
            Message::Dock(DockAction::PaneDragged(pane_grid::DragEvent::Picked { pane })) => {
                self.pending_detach = Some((pane, self.cursor_pos));
                Task::none()
            }

            // ── Canceled: user released too early, discard ────────────────────
            Message::Dock(DockAction::PaneDragged(pane_grid::DragEvent::Canceled { .. })) => {
                self.pending_detach = None;
                Task::none()
            }

            // ── Other dock actions ────────────────────────────────────────────
            Message::Dock(action) => {
                self.dock_state.update(action).map(Message::Dock)
            }

            // ── Initiate OS drag once the floating window is ready ────────────
            Message::FloatReady(id) => {
                win::drag(id)
            }

            // ── Floating window: OS window drag ──────────────────────────────
            Message::Float { id, action: FloatAction::StartWindowDrag } => {
                win::drag(id)
            }

            // ── Floating window: tab actions ──────────────────────────────────
            Message::Float { id, action: FloatAction::Tab(ev) } => {
                let Some(info) = self.detached.get_mut(&id) else {
                    return Task::none();
                };
                match ev {
                    TabEvent::Select(dock_id) => {
                        info.group.set_active(dock_id);
                    }
                    TabEvent::Close(dock_id) => {
                        info.group.remove_dock(dock_id);
                        if info.group.is_empty() {
                            self.detached.remove(&id);
                            return win::close(id);
                        }
                    }
                    TabEvent::Reorder { from, to } => {
                        let dock_id = info.group.iter().nth(from).copied();
                        if let Some(d) = dock_id {
                            info.group.reorder(d, to);
                        }
                    }
                }
                Task::none()
            }

            // ── OS window events ──────────────────────────────────────────────
            Message::WinEvent(id, event) => {
                match event {
                    window::Event::Opened { position, size } => {
                        if id == self.main_window_id {
                            self.main_window_pos = position.unwrap_or(Point::ORIGIN);
                            self.main_window_size = size;
                        }
                    }
                    window::Event::Moved(pos) => {
                        if id == self.main_window_id {
                            self.main_window_pos = pos;
                        } else if let Some(info) = self.detached.get_mut(&id) {
                            info.window_pos = pos;
                            info.last_move = Instant::now();
                        }
                    }
                    window::Event::Resized(size) => {
                        if id == self.main_window_id {
                            self.main_window_size = size;
                        } else if let Some(info) = self.detached.get_mut(&id) {
                            info.window_size = size;
                        }
                    }
                    window::Event::Closed => {
                        self.detached.remove(&id);
                        if id == self.main_window_id || self.detached.is_empty() && self.dock_state.panes.is_empty() {
                            return iced::exit();
                        }
                    }
                    _ => {}
                }
                Task::none()
            }

            // ── Cursor tracking + threshold-based detach ──────────────────────
            Message::CursorMoved(pos) => {
                self.cursor_pos = pos;
                if let Some((pane, start)) = self.pending_detach {
                    if pos.distance(start) >= DETACH_THRESHOLD {
                        self.pending_detach = None;
                        return self.do_detach(pane);
                    }
                }
                Task::none()
            }

            // ── Periodic re-attach check ──────────────────────────────────────
            Message::ReattachTick => {
                let now = Instant::now();
                let candidates: Vec<WindowId> = self.detached.iter()
                    .filter(|(_, info)| {
                        now.duration_since(info.last_move) >= Duration::from_millis(200)
                            && overlaps(
                                info.window_pos, info.window_size,
                                self.main_window_pos, self.main_window_size,
                            )
                    })
                    .map(|(id, _)| *id)
                    .collect();

                let tasks: Vec<Task<Message>> = candidates
                    .into_iter()
                    .map(|id| self.reattach(id))
                    .collect();
                Task::batch(tasks)
            }

            Message::Noop => Task::none(),
        }
    }

    fn screen_cursor(&self) -> Point {
        Point::new(
            self.main_window_pos.x + self.cursor_pos.x,
            self.main_window_pos.y + self.cursor_pos.y,
        )
    }

    /// Detach `pane` from the grid and open a floating window at the cursor.
    fn do_detach(&mut self, pane: pane_grid::Pane) -> Task<Message> {
        let Some(group) = self.dock_state.detach_pane(pane) else {
            return Task::none();
        };
        let scr = self.screen_cursor();
        let win_size = Size::new(400.0, 350.0);
        let (win_id, open_task) = win::open(window::Settings {
            decorations: false,
            position: Position::Specific(scr),
            size: win_size,
            level: window::Level::AlwaysOnTop,
            platform_specific: window::settings::PlatformSpecific {
                skip_taskbar: true,
                undecorated_shadow: true,
                ..Default::default()
            },
            ..Default::default()
        });
        self.detached.insert(win_id, DetachedInfo {
            group,
            window_pos: scr,
            window_size: win_size,
            last_move: Instant::now(),
        });
        open_task.map(Message::FloatReady)
    }

    fn reattach(&mut self, win_id: WindowId) -> Task<Message> {
        let Some(info) = self.detached.remove(&win_id) else {
            return Task::none();
        };

        // Floating window centre in main-window-relative coordinates.
        let rel = Point::new(
            info.window_pos.x + info.window_size.width  / 2.0 - self.main_window_pos.x,
            info.window_pos.y + info.window_size.height / 2.0 - self.main_window_pos.y,
        );

        const SPACING: f32 = 2.0;
        let node = self.dock_state.panes.layout();
        let regions = node.pane_regions(SPACING, 0.0, self.main_window_size);

        // Find the pane under the floating window's centre, fall back to first pane.
        let target = regions.iter()
            .find(|(_, r)| r.contains(rel))
            .or_else(|| regions.iter().next())
            .map(|(&pane, r)| {
                // Horizontal split if we're closer to top/bottom edge, else vertical.
                let cx = r.x + r.width  / 2.0;
                let cy = r.y + r.height / 2.0;
                let axis = if (rel.x - cx).abs() > (rel.y - cy).abs() {
                    pane_grid::Axis::Vertical
                } else {
                    pane_grid::Axis::Horizontal
                };
                (pane, axis)
            });

        if let Some((pane, axis)) = target {
            self.dock_state.split(axis, pane, info.group);
        }

        win::close(win_id)
    }
}

fn overlaps(pos_a: Point, size_a: Size, pos_b: Point, size_b: Size) -> bool {
    pos_a.x < pos_b.x + size_b.width
        && pos_a.x + size_a.width  > pos_b.x
        && pos_a.y < pos_b.y + size_b.height
        && pos_a.y + size_a.height > pos_b.y
}

// ── Subscription ──────────────────────────────────────────────────────────────

impl App {
    fn subscription(&self) -> Subscription<Message> {
        let win_events = win::events().map(|(id, ev)| Message::WinEvent(id, ev));

        let mouse_events = iced::event::listen_with(|event, _status, _id| {
            if let iced::Event::Mouse(iced_core::mouse::Event::CursorMoved { position }) = event {
                Some(Message::CursorMoved(position))
            } else {
                None
            }
        });

        let tick = iced::time::every(Duration::from_millis(50))
            .map(|_| Message::ReattachTick);

        Subscription::batch([win_events, mouse_events, tick])
    }
}

// ── View ──────────────────────────────────────────────────────────────────────

impl App {
    fn view(&self, window_id: WindowId) -> Element<'_, Message> {
        if window_id == self.main_window_id {
            // Compute drag hint from the first floating window currently overlapping the main
            // window.  The hint is the floating window's centre in main-window coordinates.
            let drag_hint = self.detached.iter()
                .find(|(_, info)| {
                    overlaps(info.window_pos, info.window_size,
                             self.main_window_pos, self.main_window_size)
                })
                .map(|(_, info)| Point::new(
                    info.window_pos.x + info.window_size.width  / 2.0 - self.main_window_pos.x,
                    info.window_pos.y + info.window_size.height / 2.0 - self.main_window_pos.y,
                ));

            let dock_w = DockWidget::new(&self.dock_state, Message::Dock)
                .content(|_pane, id| {
                    iced::widget::center(
                        iced::widget::text(id.to_string()).size(20)
                    ).into()
                });

            if let Some(pos) = drag_hint {
                dock_w.drag_hint(pos).into()
            } else {
                dock_w.into()
            }
        } else if let Some(info) = self.detached.get(&window_id) {
            let win_id = window_id;
            FloatingDockWidget::new(
                &info.group,
                move |action| Message::Float { id: win_id, action },
            )
            .content(|dock_id| {
                iced::widget::center(
                    iced::widget::text(dock_id.to_string()).size(20)
                ).into()
            })
            .into()
        } else {
            iced::widget::text("").into()
        }
    }
}
