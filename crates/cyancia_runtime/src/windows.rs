use std::{
    any::Any,
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use cyancia_utils::wrapper;
use iced_core::{Element, Theme, window};
use iced_runtime::{Task, futures::Subscription};
use parking_lot::Mutex;

use crate::{Runtime, service::Service};

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
        runtime: &'a Runtime,
    ) -> impl Into<Element<'a, Self::Message, Theme, iced_wgpu::Renderer>>;
    fn update(
        &mut self,
        message: Self::Message,
        runtime: &Runtime,
    ) -> impl Into<Task<Self::Message>>;
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::none()
    }
}

pub trait ErasedWindowView: Send + Sync + 'static {
    fn id(&self) -> WindowViewId;
    fn view<'a>(
        &'a self,
        runtime: &'a Runtime,
    ) -> Element<'a, Box<dyn Any + Send + Sync>, Theme, iced_wgpu::Renderer>;
    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        runtime: &Runtime,
    ) -> Task<Box<dyn Any + Send + Sync>>;
    fn subscription(&self) -> Subscription<Box<dyn Any + Send + Sync>> {
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
        runtime: &'a Runtime,
    ) -> Element<'a, Box<dyn Any + Send + Sync>, Theme, iced_wgpu::Renderer> {
        <T as WindowView>::view(self, runtime)
            .into()
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn update(
        &mut self,
        message: Box<dyn Any + Send + Sync>,
        runtime: &Runtime,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        let msg = *message
            .downcast::<T::Message>()
            .expect("Cast window message failed");
        <T as WindowView>::update(self, msg, runtime)
            .into()
            .map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
    }

    fn subscription(&self) -> Subscription<Box<dyn Any + Send + Sync>> {
        <T as WindowView>::subscription(self).map(|msg| Box::new(msg) as Box<dyn Any + Send + Sync>)
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

#[derive(Default)]
pub struct WindowManager {
    windows: HashMap<window::Id, WindowViewId>,
    views: HashMap<WindowViewId, Box<dyn ErasedWindowView>>,
    opened_views: HashMap<WindowViewId, window::Id>,
}

impl Service for WindowManager {}

impl WindowManager
where
    Theme: 'static,
{
    pub fn register_view<T: WindowView>(&mut self, view: T) {
        self.views.insert(view.id(), Box::new(view));
    }

    pub fn view<'a>(
        &'a self,
        id: window::Id,
        runtime: &'a Runtime,
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
        runtime: &Runtime,
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
                .with(id.clone())
                .map(|(window, msg)| ErasedWindowMessage {
                    window,
                    message: msg,
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
        if let Some(window_id) = self.opened_views.remove(&view_id) {
            self.windows.remove(&window_id);
            iced_runtime::window::close::<()>(window_id).discard()
        } else {
            Task::none()
        }
    }
}
