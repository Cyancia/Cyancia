use std::{any::Any, collections::HashMap, sync::Arc};

use cyancia_id::Id;
use iced_core::{Element, window};
use iced_runtime::{Task, futures::Subscription};
use iced_widget::space;

pub struct Window;

pub trait WindowView<Theme, Renderer>: 'static {
    type Message: Send + Sync + 'static;

    fn id(&self) -> Id<Window>;
    fn view<'a>(&'a self) -> Element<'a, Self::Message, Theme, Renderer>;
    fn update(
        &mut self,
        message: Self::Message,
        windows: &mut WindowManagerShell,
    ) -> Task<Self::Message>;
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }
}

pub trait ErasedWindowView<Theme, Renderer> {
    fn id(&self) -> Id<Window>;
    fn view<'a>(&'a self) -> Element<'a, Box<dyn Any + Send + Sync>, Theme, Renderer>;
    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        windows: &mut WindowManagerShell,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn subscription(&self) -> Subscription<Box<dyn Any + Send + Sync>> {
        Subscription::none()
    }
}

impl<Theme, Renderer, T> ErasedWindowView<Theme, Renderer> for T
where
    Theme: 'static,
    Renderer: iced_core::Renderer + 'static,
    T: WindowView<Theme, Renderer>,
{
    fn id(&self) -> Id<Window> {
        <T as WindowView<Theme, Renderer>>::id(self)
    }

    fn view<'a>(&'a self) -> Element<'a, Box<dyn Any + Send + Sync>, Theme, Renderer> {
        <T as WindowView<Theme, Renderer>>::view(self)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        windows: &mut WindowManagerShell,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        let msg = *message
            .downcast::<T::Message>()
            .expect("Cast window message failed");
        <T as WindowView<Theme, Renderer>>::update(self, msg, windows)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn subscription(&self) -> Subscription<Box<dyn Any + Send + Sync>> {
        <T as WindowView<Theme, Renderer>>::subscription(self)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }
}

#[derive(Debug)]
pub struct ErasedWindowMessage {
    window: Id<Window>,
    message: Box<dyn Any + Send + Sync>,
}

#[derive(Debug)]
pub enum WindowManagerMessage {
    WindowClosed(window::Id),
    Window(ErasedWindowMessage),
}

pub struct WindowManager<Theme, Renderer> {
    windows: HashMap<window::Id, Id<Window>>,
    views: HashMap<Id<Window>, Box<dyn ErasedWindowView<Theme, Renderer>>>,
    opened_views: HashMap<Id<Window>, window::Id>,
}

impl<Theme, Renderer> WindowManager<Theme, Renderer>
where
    Theme: 'static,
    Renderer: iced_core::Renderer + 'static,
{
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            views: HashMap::new(),
            opened_views: HashMap::new(),
        }
    }

    pub fn register<T: WindowView<Theme, Renderer>>(&mut self, view: T) {
        self.views.insert(view.id(), Box::new(view));
    }

    pub fn view(
        &self,
        iced_id: window::Id,
    ) -> Option<Element<'_, WindowManagerMessage, Theme, Renderer>> {
        let window = self.windows.get(&iced_id)?;
        let view = self.views.get(window)?;
        Some(view.view().map(move |msg| {
            WindowManagerMessage::Window(ErasedWindowMessage {
                window: *window,
                message: msg,
            })
        }))
    }

    pub fn update(&mut self, message: WindowManagerMessage) -> Task<WindowManagerMessage> {
        match message {
            WindowManagerMessage::WindowClosed(id) => {
                if let Some(window_id) = self.windows.remove(&id) {
                    self.opened_views.remove(&window_id);
                }

                if self.opened_views.contains_key(&Id::from_str("main_view")) {
                    Task::none()
                } else {
                    iced_runtime::exit()
                }
            }
            WindowManagerMessage::Window(message) => {
                let view = self
                    .views
                    .get_mut(&message.window)
                    .expect("Window view not found");
                let mut shell = WindowManagerShell::default();
                let mut task =
                    view.update(message.message, &mut shell)
                        .map(move |msg| ErasedWindowMessage {
                            window: message.window,
                            message: msg,
                        });

                for view_id in shell.to_open {
                    if self.opened_views.contains_key(&view_id) {
                        continue;
                    }

                    task = task.chain(self.open_view(view_id).discard());
                }

                for view_id in shell.to_close {
                    if !self.opened_views.contains_key(&view_id) {
                        continue;
                    }

                    task = task.chain(self.close_view(view_id).discard());
                }

                for view_id in shell.to_toggle {
                    if self.opened_views.contains_key(&view_id) {
                        task = task.chain(self.close_view(view_id).discard());
                    } else {
                        task = task.chain(self.open_view(view_id).discard());
                    }
                }

                task.map(WindowManagerMessage::Window)
            }
        }
    }

    pub fn subscription(&self) -> Subscription<WindowManagerMessage> {
        let views = self.views.iter().map(|(id, view)| {
            view.subscription().with(*id).map(|(window, msg)| {
                WindowManagerMessage::Window(ErasedWindowMessage {
                    window,
                    message: msg,
                })
            })
        });

        let manager = iced_runtime::window::close_events().map(WindowManagerMessage::WindowClosed);

        Subscription::batch(views.chain([manager]))
    }

    pub fn open_view(&mut self, view_id: Id<Window>) -> Task<()> {
        let (window_id, task) = iced_runtime::window::open(Default::default());
        self.windows.insert(window_id, view_id);
        self.opened_views.insert(view_id, window_id);
        log::info!("Opened window {:?} for view {:?}", window_id, view_id);
        task.discard()
    }

    pub fn close_view(&mut self, view_id: Id<Window>) -> Task<()> {
        if let Some(window_id) = self.opened_views.remove(&view_id) {
            self.windows.remove(&window_id);
            log::info!("Closed window {:?} for view {:?}", window_id, view_id);
            iced_runtime::window::close::<()>(window_id).discard()
        } else {
            Task::none()
        }
    }
}

#[derive(Debug, Default)]
pub struct WindowManagerShell {
    to_open: Vec<Id<Window>>,
    to_close: Vec<Id<Window>>,
    to_toggle: Vec<Id<Window>>,
}

impl WindowManagerShell {
    pub fn open_window(&mut self, id: Id<Window>) {
        self.to_open.push(id);
    }

    pub fn close_window(&mut self, id: Id<Window>) {
        self.to_close.push(id);
    }

    pub fn toggle_window(&mut self, id: Id<Window>) {
        self.to_toggle.push(id);
    }
}
