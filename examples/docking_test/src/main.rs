use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cyancia_dock::dock::{Dock, DockAction, DockId, DockWidget, FloatAction, FloatingDockWidget};
use cyancia_dock::group::DockGroupData;
use cyancia_dock::state::DockState;
use cyancia_dock::{AttachOrMergeInfo, DockManager, DockMessage};
use iced::{Element, Renderer, Subscription, Task, Theme};
use iced_core::window::{self, Id as WindowId, Position};
use iced_core::{Point, Size};
use iced_runtime::window as win;
use iced_widget::{pane_grid, space, text};

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

macro_rules! text_dock {
    ($name:ident, $text:expr) => {
        struct $name;

        impl Dock<Theme, Renderer> for $name {
            type Message = ();

            fn id(&self) -> DockId {
                DockId::new($text.into())
            }

            fn view(&self) -> Element<'_, Self::Message, Theme, Renderer> {
                text(stringify!($text)).into()
            }

            fn update(&mut self, _message: ()) -> Task<()> {
                Task::none()
            }
        }
    };
}

text_dock!(PropertiesDock, "Properties");
text_dock!(TimelineDock, "Timeline");
text_dock!(LayersDock, "Layers");
text_dock!(ViewportDock, "Viewport");
text_dock!(AssetsDock, "Assets");
text_dock!(ConsoleDock, "Console");

struct App {
    manager: DockManager<Theme, Renderer>,
}

#[derive(Debug)]
enum Message {
    Dock(DockMessage),
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

        let mut manager = DockManager::new(main_id);
        manager.register_dock(PropertiesDock);
        manager.register_dock(TimelineDock);
        manager.register_dock(LayersDock);
        manager.register_dock(ViewportDock);
        manager.register_dock(AssetsDock);
        manager.register_dock(ConsoleDock);

        manager.open_dock(DockId::new("Properties".into()));
        manager.open_dock(DockId::new("Timeline".into()));
        manager.open_dock(DockId::new("Layers".into()));
        manager.open_dock(DockId::new("Viewport".into()));
        manager.open_dock(DockId::new("Assets".into()));
        manager.open_dock(DockId::new("Console".into()));

        let app = App { manager };

        (app, open_task.discard())
    }

    fn update(&mut self, msg: Message) -> Task<Message> {
        match msg {
            Message::Dock(msg) => self.manager.update(msg).discard(),
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
        self.manager
            .view(window_id)
            .unwrap_or_else(|| Element::new(space()))
            .map(Message::Dock)
    }
}
