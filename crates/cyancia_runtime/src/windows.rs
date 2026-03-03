use std::{
    any::Any,
    collections::HashMap,
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

pub trait WindowView: Send + Sync + 'static {
    type Message: Send + Sync + 'static;

    fn id(&self) -> WindowViewId;
    fn view<'a>(
        &'a self,
        runtime: Arc<Services>,
    ) -> impl Into<Element<'a, Self::Message, Theme, iced_wgpu::Renderer>>;
    fn update(
        &mut self,
        message: Self::Message,
        runtime: Arc<Services>,
    ) -> impl Into<Task<Self::Message>>;
    // TODO: Iced subscriptions are global, and we need the implementation to provide the window id.
    //       Is it possible to distinguish between windows without the id?
    fn subscription(&self) -> Subscription<(window::Id, Self::Message)> {
        Subscription::none()
    }
}

pub trait ErasedWindowView: Send + Sync + 'static {
    fn id(&self) -> WindowViewId;
    fn view<'a>(
        &'a self,
        runtime: Arc<Services>,
    ) -> Element<'a, Box<dyn Any + Send + Sync>, Theme, iced_wgpu::Renderer>;
    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        runtime: Arc<Services>,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn subscription(&self) -> Subscription<(window::Id, Box<dyn Any + Send + Sync>)> {
        Subscription::none()
    }
}

impl<T> ErasedWindowView for T
where
    T: WindowView,
{
    fn id(&self) -> WindowViewId {
        <T as WindowView>::id(self)
    }

    fn view<'a>(
        &'a self,
        runtime: Arc<Services>,
    ) -> Element<'a, Box<dyn Any + Send + Sync>, Theme, iced_wgpu::Renderer> {
        <T as WindowView>::view(self, runtime)
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

    fn subscription(&self) -> Subscription<(window::Id, Box<dyn Any + Send + Sync>)> {
        <T as WindowView>::subscription(self)
            .map(|msg| (msg.0, Box::new(msg.1) as Box<dyn Any + Send + Sync>))
    }
}

#[derive(Debug)]
pub struct ErasedWindowMessage {
    window: WindowViewId,
    message: Box<dyn Any + Send + Sync>,
}

pub enum WindowManagerMessage {
    WindowOpened(window::Id, WindowViewId),
    Window(ErasedWindowMessage),
}

#[derive(Default, Clone, Deref, DerefMut)]
pub struct OpenedViewMap(HashMap<WindowViewId, window::Id>);

impl Hash for OpenedViewMap {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for key in self.0.keys() {
            key.hash(state);
        }
    }
}

#[derive(Default)]
pub struct WindowManager {
    windows: HashMap<window::Id, WindowViewId>,
    views: HashMap<WindowViewId, Box<dyn ErasedWindowView>>,
    opened_views: OpenedViewMap,
    root_view: Option<WindowViewId>,
}

impl Service for WindowManager {}

impl WindowManager
where
    Theme: 'static,
{
    pub fn register_view<T: WindowView>(&mut self, view: T) {
        self.views.insert(view.id(), Box::new(view));
    }

    pub fn set_root_view(&mut self, view_id: WindowViewId) {
        self.root_view = Some(view_id);
    }

    pub fn root_view(&self) -> Option<WindowViewId> {
        self.root_view
    }

    pub fn view<'a>(
        &'a self,
        id: window::Id,
        runtime: Arc<Services>,
    ) -> Element<'a, ErasedWindowMessage, Theme, iced_wgpu::Renderer> {
        let window = self.windows.get(&id).expect("Window not found").clone();
        let view = self.views.get(&window).expect("Window view not found");
        view.view(runtime).map(move |msg| ErasedWindowMessage {
            window,
            message: msg,
        })
    }

    pub fn update(
        &mut self,
        message: ErasedWindowMessage,
        runtime: Arc<Services>,
    ) -> Task<ErasedWindowMessage> {
        let view = self
            .views
            .get_mut(&message.window)
            .expect("Window view not found");
        view.update(message.message, runtime)
            .map(move |msg| ErasedWindowMessage {
                window: message.window.clone(),
                message: msg,
            })
    }

    pub fn subscription(&self) -> Subscription<ErasedWindowMessage> {
        let subscriptions = self.views.iter().map(|(id, view)| {
            view.subscription()
                .with((id.clone(), self.opened_views.clone()))
                .filter_map(|((view_id, opened_windows), (window_id, msg))| {
                    if Some(&window_id) == opened_windows.get(&view_id) {
                        Some(ErasedWindowMessage {
                            window: view_id,
                            message: msg,
                        })
                    } else {
                        None
                    }
                })
        });

        Subscription::batch(subscriptions)
    }

    pub fn open_window(&mut self, view_id: WindowViewId) -> Task<()> {
        let (window_id, task) = iced_runtime::window::open(Default::default());
        self.windows.insert(window_id, view_id.clone());
        self.opened_views.insert(view_id, window_id);
        task.discard()
    }

    pub fn close_window(&mut self, view_id: WindowViewId) -> Task<()> {
        if Some(view_id) == self.root_view {
            iced_runtime::exit()
        } else if let Some(window_id) = self.opened_views.remove(&view_id) {
            self.windows.remove(&window_id);
            iced_runtime::window::close::<()>(window_id).discard()
        } else {
            Task::none()
        }
    }

    pub fn window_closed(&mut self, window_id: window::Id) -> Task<()> {
        if let Some(view_id) = self.windows.remove(&window_id) {
            self.opened_views.remove(&view_id);
            if Some(view_id) == self.root_view {
                return iced_runtime::exit();
            }
        }

        Task::none()
    }
}

pub trait WindowCommand: Send + Sync + 'static {
    fn execute(self: Box<Self>, wm: &mut WindowManager, runtime: Arc<Services>)
    -> Option<Task<()>>;
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

    pub fn execute(&mut self, wm: &mut WindowManager, runtime: Arc<Services>) -> Task<()> {
        let mut tasks = Vec::new();
        for command in self.commands.drain(..) {
            if let Some(task) = command.execute(wm, runtime.clone()) {
                tasks.push(task);
            }
        }
        Task::batch(tasks)
    }
}

pub struct OpenWindowCommand {
    view_id: WindowViewId,
}

impl WindowCommand for OpenWindowCommand {
    fn execute(
        self: Box<Self>,
        wm: &mut WindowManager,
        runtime: Arc<Services>,
    ) -> Option<Task<()>> {
        Some(wm.open_window(self.view_id))
    }
}

impl OpenWindowCommand {
    pub fn new(view_id: WindowViewId) -> Self {
        Self { view_id }
    }
}

pub struct CloseWindowCommand {
    view_id: WindowViewId,
}

impl WindowCommand for CloseWindowCommand {
    fn execute(
        self: Box<Self>,
        wm: &mut WindowManager,
        runtime: Arc<Services>,
    ) -> Option<Task<()>> {
        Some(wm.close_window(self.view_id))
    }
}

impl CloseWindowCommand {
    pub fn new(view_id: WindowViewId) -> Self {
        Self { view_id }
    }
}

pub struct ToggleWindowCommand {
    view_id: WindowViewId,
}

impl WindowCommand for ToggleWindowCommand {
    fn execute(
        self: Box<Self>,
        wm: &mut WindowManager,
        runtime: Arc<Services>,
    ) -> Option<Task<()>> {
        if wm.opened_views.contains_key(&self.view_id) {
            Some(wm.close_window(self.view_id))
        } else {
            Some(wm.open_window(self.view_id))
        }
    }
}

impl ToggleWindowCommand {
    pub fn new(view_id: WindowViewId) -> Self {
        Self { view_id }
    }
}
