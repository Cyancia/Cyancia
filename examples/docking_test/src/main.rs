use std::collections::HashMap;
use std::time::{Duration, Instant};

use iced::{Element, Subscription, Task};
use iced_core::window::{self, Id as WindowId, Position};
use iced_core::{Point, Size};
use iced_runtime::window as win;
use iced_widget::pane_grid;

use cyancia_dock::{
    DockAction, DockGroupData, DockId, DockManager, DockState, DockWidget, FloatAction,
    FloatingDockWidget, AttachInfo, TabEvent,
};

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
    DockId::from(s)
}

struct App {
    manager: DockManager,
}

#[derive(Debug, Clone)]
enum Message {
    Dock(DockAction),
    Float { id: WindowId, action: FloatAction },
    WinEvent(WindowId, window::Event),
    MainWindowCursorMoved(Point),
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
            Message::MainWindowCursorMoved(point) => {
                self.manager.on_main_window_cursor_moved(point).discard()
            }
            Message::CursorReleased => self.manager.on_float_window_drag_end().discard(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let win_events = win::events().map(|(id, ev)| Message::WinEvent(id, ev));

        let mouse_events = iced::event::listen_with(|event, _status, window| match event {
            iced::Event::Mouse(e) => Some((e, window)),
            _ => None,
        })
        .with(self.manager.main_window().id)
        .filter_map(|(main_window, (ev, window))| match ev {
            iced::mouse::Event::CursorMoved { position } => {
                if window == main_window {
                    Some(Message::MainWindowCursorMoved(position))
                } else {
                    None
                }
            }
            iced::mouse::Event::ButtonReleased(button) => {
                if button == iced::mouse::Button::Left {
                    Some(Message::CursorReleased)
                } else {
                    None
                }
            }
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

            if let Some(pos) = self.manager.current_attach_info() {
                dock_w.drag_hint(pos).into()
            } else {
                dock_w.into()
            }
        } else if let Some(info) = self.manager.detached_window(window_id) {
            let win_id = window_id;
            FloatingDockWidget::new(&info.group, move |action| Message::Float {
                id: win_id,
                action,
            })
            .content(|dock_id| {
                iced::widget::center(iced::widget::text(dock_id.to_string()).size(20)).into()
            })
            .into()
        } else {
            iced::widget::text("").into()
        }
    }
}
