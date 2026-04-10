use std::{
    any::Any,
    collections::{BTreeSet, HashMap, HashSet, hash_map::Entry},
    hash::Hash,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use cyancia_utils::{Deref, DerefMut, wrapper};
use iced_core::{Element, Theme, window};
use iced_runtime::{Task, futures::Subscription};
use parking_lot::Mutex;

use crate::{Services, service::Service};

wrapper! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub WindowViewId : &'static str
}

pub struct Window;

pub trait WindowView: Send + Sync + 'static + Sized {
    type Message: Send + Sync + 'static;

    fn id() -> WindowViewId;
    fn boot(runtime: Arc<Services>) -> (Self, Task<Self::Message>);
    fn view<'a>(
        &'a self,
        window: window::Id,
        runtime: Arc<Services>,
    ) -> impl Into<Element<'a, Self::Message, Theme, iced_wgpu::Renderer>>;
    fn update(
        &mut self,
        message: Self::Message,
        runtime: Arc<Services>,
    ) -> impl Into<Task<Self::Message>>;
    fn close(self, runtime: Arc<Services>) -> Task<()>;
    // TODO: Iced subscriptions are global, and we need the implementation to provide the window id.
    //       Is it possible to distinguish between windows without the id?
    fn subscription(&self) -> Subscription<(window::Id, Self::Message)> {
        Subscription::none()
    }
    fn windows(&self) -> Vec<window::Id>;
}

pub trait ErasedWindowView: Send + Sync + 'static {
    fn id(&self) -> WindowViewId;
    fn view<'a>(
        &'a self,
        window: window::Id,
        runtime: Arc<Services>,
    ) -> Element<'a, Box<dyn Any + Send + Sync>, Theme, iced_wgpu::Renderer>;
    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        runtime: Arc<Services>,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn close(self: Box<Self>, runtime: Arc<Services>) -> Task<()>;
    fn subscription(&self) -> Subscription<(window::Id, Box<dyn Any + Send + Sync>)>;
    fn windows(&self) -> Vec<window::Id>;
}

impl<T> ErasedWindowView for T
where
    T: WindowView,
{
    fn id(&self) -> WindowViewId {
        <T as WindowView>::id()
    }

    fn view<'a>(
        &'a self,
        window: window::Id,
        runtime: Arc<Services>,
    ) -> Element<'a, Box<dyn Any + Send + Sync>, Theme, iced_wgpu::Renderer> {
        <T as WindowView>::view(self, window, runtime)
            .into()
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        runtime: Arc<Services>,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        let msg = *message
            .downcast::<T::Message>()
            .expect("Cast window message failed");
        <T as WindowView>::update(self, msg, runtime)
            .into()
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn close(self: Box<Self>, runtime: Arc<Services>) -> Task<()> {
        <T as WindowView>::close(*self, runtime)
    }

    fn subscription(&self) -> Subscription<(window::Id, Box<dyn Any + Send + Sync>)> {
        <T as WindowView>::subscription(self)
            .map(|msg| (msg.0, Box::new(msg.1) as Box<dyn Any + Send + Sync>))
    }

    fn windows(&self) -> Vec<window::Id> {
        <T as WindowView>::windows(self)
    }
}

#[derive(Debug)]
pub struct ErasedWindowViewMessage {
    view: WindowViewId,
    message: Box<dyn Any + Send + Sync>,
}

#[derive(Debug)]
pub struct ErasedSubscriptionWindowMessage {
    view: WindowViewId,
    window: window::Id,
    message: Box<dyn Any + Send + Sync>,
}

pub enum WindowManagerMessage {
    ViewUpdate(ErasedWindowViewMessage),
    Subscription(ErasedSubscriptionWindowMessage),
}

type WindowViewBootFn = Box<
    dyn Fn(Arc<Services>) -> (Box<dyn ErasedWindowView>, Task<ErasedWindowViewMessage>)
        + Send
        + Sync
        + 'static,
>;

#[derive(Default)]
pub struct WindowViewManager {
    view_windows: HashMap<WindowViewId, Vec<window::Id>>,
    windows: HashMap<window::Id, WindowViewId>,
    registered_views: HashMap<WindowViewId, WindowViewBootFn>,
    opened_views: HashMap<WindowViewId, Box<dyn ErasedWindowView>>,
    root_view: Option<WindowViewId>,
}

impl Service for WindowViewManager {}

impl WindowViewManager
where
    Theme: 'static,
{
    pub fn register_view<T: WindowView>(&mut self) {
        self.registered_views.insert(
            T::id(),
            Box::new(|runtime| {
                let (view, task) = T::boot(runtime);
                (
                    Box::new(view),
                    task.map(|o| ErasedWindowViewMessage {
                        view: T::id(),
                        message: Box::new(o) as Box<dyn Any + Send + Sync>,
                    }),
                )
            }),
        );
    }

    pub fn set_root_view<T: WindowView>(&mut self) {
        self.root_view = Some(T::id());
    }

    pub fn root_view(&self) -> Option<WindowViewId> {
        self.root_view
    }

    pub fn boot(&mut self, runtime: Arc<Services>) -> Task<WindowManagerMessage> {
        self.open_window_view(self.root_view.expect("No root view specified."), runtime)
    }

    pub fn view<'a>(
        &'a self,
        id: window::Id,
        runtime: Arc<Services>,
    ) -> Option<Element<'a, WindowManagerMessage, Theme, iced_wgpu::Renderer>> {
        let window = self.windows.get(&id).cloned()?;
        let view = self.opened_views.get(&window)?;
        Some(view.view(id, runtime).map(move |msg| {
            WindowManagerMessage::ViewUpdate(ErasedWindowViewMessage {
                view: window,
                message: msg,
            })
        }))
    }

    pub fn update(
        &mut self,
        message: WindowManagerMessage,
        runtime: Arc<Services>,
    ) -> Task<WindowManagerMessage> {
        match message {
            WindowManagerMessage::ViewUpdate(message) => {
                let Some(view) = self.opened_views.get_mut(&message.view) else {
                    return Task::none();
                };

                let task = view.update(message.message, runtime).map(move |msg| {
                    WindowManagerMessage::ViewUpdate(ErasedWindowViewMessage {
                        view: message.view,
                        message: msg,
                    })
                });

                let windows = view.windows();
                self.update_view_windows(message.view, windows);

                task
            }
            WindowManagerMessage::Subscription(message) => {
                if self.windows.get(&message.window) != Some(&message.view) {
                    return Task::none();
                }

                let Some(view) = self.opened_views.get_mut(&message.view) else {
                    return Task::none();
                };

                let task = view.update(message.message, runtime).map(move |msg| {
                    WindowManagerMessage::ViewUpdate(ErasedWindowViewMessage {
                        view: message.view,
                        message: msg,
                    })
                });

                task
            }
        }
    }

    pub fn subscription(&self) -> Subscription<WindowManagerMessage> {
        let subscriptions = self.opened_views.iter().map(|(id, view)| {
            view.subscription()
                .with(*id)
                .filter_map(|(view, (window, message))| {
                    Some(WindowManagerMessage::Subscription(
                        ErasedSubscriptionWindowMessage {
                            view,
                            window,
                            message,
                        },
                    ))
                })
        });

        Subscription::batch(subscriptions)
    }

    pub fn open_window_view(
        &mut self,
        view_id: WindowViewId,
        runtime: Arc<Services>,
    ) -> Task<WindowManagerMessage> {
        let Some(boot) = self.registered_views.get(&view_id) else {
            log::error!(
                "Unable to open a window view that is not registered: {}",
                view_id.0
            );
            return Task::none();
        };

        let (view, task) = boot(runtime);
        self.update_view_windows(view_id, view.windows());
        self.opened_views.insert(view_id, view);
        task.map(WindowManagerMessage::ViewUpdate)
    }

    pub fn close_window_view(&mut self, view_id: WindowViewId, runtime: Arc<Services>) -> Task<()> {
        let Some(view) = self.opened_views.remove(&view_id) else {
            return Task::none();
        };

        view.close(runtime)
    }

    fn update_view_windows(&mut self, view_id: WindowViewId, windows: Vec<window::Id>) {
        match self.view_windows.entry(view_id) {
            Entry::Occupied(mut e) => 'a: {
                if e.get() == &windows {
                    break 'a;
                }

                for window in e.get() {
                    self.windows.remove(window);
                }
                for window in &windows {
                    self.windows.insert(*window, view_id);
                }

                e.insert(windows);
            }
            Entry::Vacant(e) => {
                self.windows
                    .extend(windows.iter().map(|window| (*window, view_id)));
                e.insert(windows);
            }
        }
    }
}

pub trait WindowCommand: Send + Sync + 'static {
    fn execute(
        self: Box<Self>,
        wm: &mut WindowViewManager,
        runtime: Arc<Services>,
    ) -> Option<Task<WindowManagerMessage>>;
}

#[derive(Default)]
pub struct WindowCommandBuffer {
    commands: Vec<Box<dyn WindowCommand>>,
}

impl Service for WindowCommandBuffer {}

impl WindowCommandBuffer {
    pub fn push<T: WindowCommand>(&mut self, command: T) {
        self.commands.push(Box::new(command));
    }

    pub fn execute(
        &mut self,
        wm: &mut WindowViewManager,
        runtime: Arc<Services>,
    ) -> Task<WindowManagerMessage> {
        let mut tasks = Vec::new();
        for command in self.commands.drain(..) {
            if let Some(task) = command.execute(wm, runtime.clone()) {
                tasks.push(task);
            }
        }
        Task::batch(tasks)
    }
}

pub struct OpenWindowViewCommand {
    view_id: WindowViewId,
}

impl WindowCommand for OpenWindowViewCommand {
    fn execute(
        self: Box<Self>,
        wm: &mut WindowViewManager,
        runtime: Arc<Services>,
    ) -> Option<Task<WindowManagerMessage>> {
        Some(wm.open_window_view(self.view_id, runtime))
    }
}

impl OpenWindowViewCommand {
    pub fn new(view_id: WindowViewId) -> Self {
        Self { view_id }
    }
}

pub struct CloseWindowViewCommand {
    view_id: WindowViewId,
}

impl WindowCommand for CloseWindowViewCommand {
    fn execute(
        self: Box<Self>,
        wm: &mut WindowViewManager,
        runtime: Arc<Services>,
    ) -> Option<Task<WindowManagerMessage>> {
        Some(wm.close_window_view(self.view_id, runtime).discard())
    }
}

impl CloseWindowViewCommand {
    pub fn new(view_id: WindowViewId) -> Self {
        Self { view_id }
    }
}

pub struct ToggleWindowViewCommand {
    view_id: WindowViewId,
}

impl WindowCommand for ToggleWindowViewCommand {
    fn execute(
        self: Box<Self>,
        wm: &mut WindowViewManager,
        runtime: Arc<Services>,
    ) -> Option<Task<WindowManagerMessage>> {
        if wm.opened_views.contains_key(&self.view_id) {
            Some(wm.close_window_view(self.view_id, runtime).discard())
        } else {
            Some(wm.open_window_view(self.view_id, runtime))
        }
    }
}

impl ToggleWindowViewCommand {
    pub fn new(view_id: WindowViewId) -> Self {
        Self { view_id }
    }
}

pub struct SubWindowOpenedCommand {
    view_id: WindowViewId,
    window: window::Id,
}

impl SubWindowOpenedCommand {
    pub fn new(view_id: WindowViewId, window: window::Id) -> Self {
        Self { view_id, window }
    }
}

impl WindowCommand for SubWindowOpenedCommand {
    fn execute(
        self: Box<Self>,
        wm: &mut WindowViewManager,
        _runtime: Arc<Services>,
    ) -> Option<Task<WindowManagerMessage>> {
        wm.windows.insert(self.window, self.view_id);
        None
    }
}
