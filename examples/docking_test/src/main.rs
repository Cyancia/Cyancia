use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cyancia_dock::dock::{DockAction, DockId, DockWidget, FloatAction, FloatingDockWidget};
use cyancia_dock::group::DockGroupData;
use cyancia_dock::state::DockState;
use cyancia_dock::{AttachOrMergeInfo, DockManager};
use iced::{Element, Subscription, Task};
use iced_core::window::{self, Id as WindowId, Position};
use iced_core::{Point, Size};
use iced_runtime::window as win;
use iced_widget::pane_grid;

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> iced::Result {
    iced::daemon(App::new, App::update, App::view)
        .theme(app_theme)
        .subscription(App::subscription)
        .run()
}

fn app_theme(_: &App, _: WindowId) -> iced::Theme {
    iced::Theme::Dark
}

fn dock(s: &'static str) -> DockId {
    DockId::new(s.into())
}

struct App {
    manager: DockManager,
}

#[derive(Debug, Clone)]
enum Message {
    Dock(DockAction),
    Float { id: WindowId, action: FloatAction },
    WinEvent(WindowId, window::Event),
    CursorMoved(window::Id, Point),
    CursorReleased,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let (main_id, open_task) = win::open(window::Settings {
            size: Size::new(1024.0, 768.0),
            ..Default::default()
        });

        let left =
            DockGroupData::with_docks([dock("Properties"), dock("Timeline"), dock("Layers")]);
        let (mut dock_state, left_pane) = DockState::new(left);
        let right = DockGroupData::with_docks([dock("Viewport"), dock("Assets"), dock("Console")]);
        dock_state.split(left_pane, pane_grid::Edge::Right, right);

        let app = App {
            manager: DockManager::new(main_id, dock_state),
        };

        (app, open_task.discard())
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Dock(dock_action) => self.manager.on_dock_action(dock_action).discard(),
            Message::Float { id, action } => self.manager.on_float_action(id, action).discard(),
            Message::WinEvent(id, event) => self.manager.on_window_event(id, event).discard(),
            Message::CursorMoved(window, position) => {
                self.manager.on_cursor_moved(window, position).discard()
            }
            Message::CursorReleased => self.manager.on_float_window_drag_end().discard(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let win_events = win::events().map(|(id, ev)| Message::WinEvent(id, ev));

        let mouse_events = iced::event::listen_with(|event, _status, window| match event {
            iced::Event::Mouse(e) => match e {
                iced::mouse::Event::CursorMoved { position } => {
                    Some(Message::CursorMoved(window, position))
                }
                iced::mouse::Event::ButtonReleased(_) => Some(Message::CursorReleased),
                _ => None,
            },
            _ => None,
        });

        Subscription::batch([win_events, mouse_events])
    }

    fn view(&self, window_id: WindowId) -> Element<'_, Message> {
        if window_id == self.manager.main_window().id {
            let dock_w =
                DockWidget::new(self.manager.dock_state(), Message::Dock).content(|_pane, id| {
                    iced::widget::center(iced::widget::text(id.to_string()).size(20)).into()
                });

            if let Some(AttachOrMergeInfo::Attach(attach)) =
                self.manager.current_attach_or_merge_info()
            {
                dock_w.attach_info(attach).into()
            } else {
                dock_w.into()
            }
        } else if let Some(info) = self.manager.detached_window(window_id) {
            FloatingDockWidget::new(&info.group, move |action| Message::Float {
                id: window_id,
                action,
            })
            .content(|dock_id| {
                iced::widget::center(iced::widget::text(dock_id.to_string()).size(20)).into()
            })
            .is_merging(match self.manager.current_attach_or_merge_info() {
                Some(AttachOrMergeInfo::Merge { dst }) => dst == window_id,
                _ => false,
            })
            .into()
        } else {
            iced::widget::text("").into()
        }
    }
}
