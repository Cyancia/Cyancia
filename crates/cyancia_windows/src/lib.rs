use std::{any::Any, collections::HashMap, sync::Arc};

use cyancia_id::Id;
use iced_core::{Element, window};
use iced_runtime::{Task, futures::Subscription};

pub mod main_view;

pub struct Window;

pub trait WindowView<Theme>: 'static {
    type Message: Send + Sync + 'static;

    fn id(&self) -> Id<Window>;
    fn view(&self) -> Element<'static, Self::Message, Theme, iced_wgpu::Renderer>;
    fn update(
        &mut self,
        message: Self::Message,
        windows: &mut WindowManagerShell,
    ) -> Task<Self::Message>;
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }
}

pub trait ErasedWindowView<Theme> {
    fn id(&self) -> Id<Window>;
    fn view(&self) -> Element<'static, Box<dyn Any + Send + Sync>, Theme, iced_wgpu::Renderer>;
    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        windows: &mut WindowManagerShell,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn subscription(&self) -> Subscription<Box<dyn Any + Send + Sync>> {
        Subscription::none()
    }
}

impl<Theme, T> ErasedWindowView<Theme> for T
where
    Theme: 'static,
    T: WindowView<Theme>,
{
    fn id(&self) -> Id<Window> {
        <T as WindowView<Theme>>::id(self)
    }

    fn view(&self) -> Element<'static, Box<dyn Any + Send + Sync>, Theme, iced_wgpu::Renderer> {
        <T as WindowView<Theme>>::view(self).map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        windows: &mut WindowManagerShell,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        let msg = *message
            .downcast::<T::Message>()
            .expect("Cast window message failed");
        <T as WindowView<Theme>>::update(self, msg, windows)
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn subscription(&self) -> Subscription<Box<dyn Any + Send + Sync>> {
        <T as WindowView<Theme>>::subscription(self)
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

pub struct WindowManager<Theme> {
    windows: HashMap<window::Id, Id<Window>>,
    views: HashMap<Id<Window>, Box<dyn ErasedWindowView<Theme>>>,
    opened_views: HashMap<Id<Window>, window::Id>,
}

impl<Theme> WindowManager<Theme>
where
    Theme: 'static,
{
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            views: HashMap::new(),
            opened_views: HashMap::new(),
        }
    }

    pub fn register<T: WindowView<Theme> + Default>(&mut self) {
        let view = T::default();
        self.views.insert(view.id(), Box::new(view));
    }

    pub fn boot() -> (Self, Task<WindowManagerMessage>) {
        let mut instance = Self::new();
        instance.register::<main_view::MainView>();
        let task = instance.open_view(Id::from_str("main_view"));
        (instance, task.discard())
    }

    pub fn view(
        &self,
        iced_id: window::Id,
    ) -> Element<'_, WindowManagerMessage, Theme, iced_wgpu::Renderer> {
        let window = self
            .windows
            .get(&iced_id)
            .expect("Window not found")
            .clone();
        let view = self.views.get(&window).expect("Window view not found");
        view.view().map(move |msg| {
            WindowManagerMessage::Window(ErasedWindowMessage {
                window,
                message: msg,
            })
        })
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
                    task = task.chain(self.close_view(view_id).discard());
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

    fn open_view(&mut self, view_id: Id<Window>) -> Task<()> {
        let (window_id, task) = iced_runtime::window::open(Default::default());
        self.windows.insert(window_id, view_id);
        self.opened_views.insert(view_id, window_id);
        task.discard()
    }

    fn close_view(&mut self, view_id: Id<Window>) -> Task<()> {
        if let Some(window_id) = self.opened_views.remove(&view_id) {
            self.windows.remove(&window_id);
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
}

impl WindowManagerShell {
    pub fn open_window(&mut self, id: Id<Window>) {
        self.to_open.push(id);
    }

    pub fn close_window(&mut self, id: Id<Window>) {
        self.to_close.push(id);
    }
}
