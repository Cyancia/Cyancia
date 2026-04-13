use std::{
    any::TypeId,
    cell::{Ref, RefCell, RefMut},
    collections::{HashMap, VecDeque},
    marker::PhantomData,
    sync::{Arc, Mutex, OnceLock},
};

use iced_core::{Element, window};
use iced_core::{Length, Theme, Widget};
use iced_futures::{Subscription, backend::native};
use iced_runtime::{Task, window::close_events};
use iced_wgpu::window::compositor::WgpuContext;
use iced_winit::program::Program;
use parking_lot::RwLock;

use crate::{
    plugin::Plugin,
    service::{FromRuntime, RenderContext, Service, ServiceMut, ServiceRef},
    windows::{
        ErasedWindowViewMessage, WindowCommandBuffer, WindowViewId, WindowViewManager,
        WindowViewManagerMessage,
    },
};

pub mod event;
#[doc(hidden)]
pub use event::__private;
pub mod plugin;
pub mod service;
pub mod windows;

pub enum ApplicationState {
    Adding,
    Built,
    Finished,
}

pub struct Application {
    state: ApplicationState,
    // TODO remove this ref cell
    runtime: RefCell<Runtime>,
    plugins: VecDeque<Box<dyn Plugin>>,
}

impl Application {
    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> &mut Self {
        if !matches!(self.state, ApplicationState::Adding) {
            panic!("Plugins can only be added in the Adding state");
        }

        self.plugins.push_back(Box::new(plugin));
        self
    }

    pub fn add_service<T: Service + FromRuntime>(&mut self) -> &mut Self {
        self.runtime.borrow_mut().add_service::<T>();
        self
    }

    pub fn add_service_instance<T: Service>(&mut self, service: T) -> &mut Self {
        self.runtime.borrow_mut().add_service_instance(service);
        self
    }

    pub fn runtime(&self) -> Ref<'_, Runtime> {
        self.runtime.borrow()
    }

    pub fn runtime_mut(&self) -> RefMut<'_, Runtime> {
        self.runtime.borrow_mut()
    }

    pub fn build_plugins(&mut self) {
        let mut plugins = Vec::with_capacity(self.plugins.len());
        while let Some(plugin) = self.plugins.pop_front() {
            plugin.build(self);
            plugins.push(plugin);
        }
        self.state = ApplicationState::Built;

        for plugin in plugins {
            plugin.finish(self);
        }
        self.state = ApplicationState::Finished;
    }

    pub fn run(self) -> Result<(), iced_winit::Error> {
        if !matches!(self.state, ApplicationState::Finished) {
            panic!("Plugins must be built before running the application");
        }

        iced_winit::run(self)
    }
}

impl Default for Application {
    fn default() -> Self {
        Self {
            state: ApplicationState::Adding,
            runtime: RefCell::new(Runtime::default()),
            plugins: VecDeque::new(),
        }
    }
}

impl Program for Application {
    type State = Runtime;

    type Message = ApplicationMessage;

    type Theme = Theme;

    type Renderer = iced_wgpu::Renderer;

    type Executor = native::smol::Executor;

    fn name() -> &'static str {
        "Cyancia Runtime"
    }

    fn settings(&self) -> iced_core::Settings {
        Default::default()
    }

    fn window(&self) -> Option<window::Settings> {
        None
    }

    fn boot(&self) -> (Self::State, Task<Self::Message>) {
        let mut rt = std::mem::take::<Runtime>(&mut self.runtime.borrow_mut());

        let window_task = rt
            .wm
            .boot(rt.services.clone())
            .map(ApplicationMessage::Window);
        let deadlock_detect_task = Task::future(async {
            loop {
                smol::Timer::after(std::time::Duration::from_secs(5)).await;
                let deadlocks = parking_lot::deadlock::check_deadlock();
                for (i_dl, threads) in deadlocks.into_iter().enumerate() {
                    log::error!("#{} Deadlock detected", i_dl);

                    for (it, t) in threads.into_iter().enumerate() {
                        log::error!("Thread {}:", it);
                        log::error!("{:#?}", t.backtrace());
                    }
                }
            }
        });
        (
            rt,
            Task::batch([window_task, deadlock_detect_task.discard()]),
        )
    }

    fn update(&self, state: &mut Self::State, message: Self::Message) -> Task<Self::Message> {
        let mut task = match message {
            ApplicationMessage::Window(m) => state
                .wm
                .update(m, state.services.clone())
                .map(ApplicationMessage::Window),
            ApplicationMessage::WindowClosed(id) => state
                .wm
                .on_window_closed(id, state.services.clone())
                .discard(),
        };

        task = task.chain(
            state
                .services
                .service_mut::<WindowCommandBuffer>()
                .execute(&mut state.wm, state.services.clone())
                .discard(),
        );

        task
    }

    fn view<'a>(
        &self,
        state: &'a Self::State,
        window: window::Id,
    ) -> Element<'a, Self::Message, Self::Theme, Self::Renderer> {
        struct DummyWidget;
        impl<Message, Theme, Renderer: iced_core::Renderer> Widget<Message, Theme, Renderer>
            for DummyWidget
        {
            fn size(&self) -> iced_core::Size<iced_core::Length> {
                iced_core::Size::new(iced_core::Length::Fill, iced_core::Length::Fill)
            }

            fn layout(
                &mut self,
                tree: &mut iced_core::widget::Tree,
                renderer: &Renderer,
                limits: &iced_core::layout::Limits,
            ) -> iced_core::layout::Node {
                iced_core::layout::atomic(limits, Length::Fill, Length::Fill)
            }

            fn draw(
                &self,
                tree: &iced_core::widget::Tree,
                renderer: &mut Renderer,
                theme: &Theme,
                style: &iced_core::renderer::Style,
                layout: iced_core::Layout<'_>,
                cursor: iced_core::mouse::Cursor,
                viewport: &iced_core::Rectangle,
            ) {
            }
        }

        state
            .wm
            .view(window, state.services.clone())
            .unwrap_or_else(|| Element::new(DummyWidget))
            .map(ApplicationMessage::Window)
    }

    fn subscription(&self, state: &Self::State) -> Subscription<Self::Message> {
        let windows = state.wm.subscription().map(ApplicationMessage::Window);
        let window_closed = close_events().map(ApplicationMessage::WindowClosed);

        Subscription::batch([windows, window_closed])
    }

    fn compositor_context(&self, state: &Self::State) -> Option<WgpuContext> {
        let render_context = state.services.service::<RenderContext>();
        let context = WgpuContext {
            instance: render_context.instance.as_ref().clone(),
            adapter: render_context.adapter.as_ref().clone(),
            device: render_context.device.as_ref().clone(),
            queue: render_context.queue.as_ref().clone(),
        };
        Some(context)
    }
}

#[derive(Default)]
pub struct Runtime {
    services: Arc<Services>,
    wm: WindowViewManager,
}

impl Runtime {
    pub fn add_service<T: Service + FromRuntime>(&mut self) -> &mut Self {
        let instance = T::from_runtime(&self.services);
        self.add_service_instance(instance);
        self
    }

    pub fn add_service_instance<T: Service>(&mut self, service: T) -> &mut Self {
        self.services
            .services
            .write()
            .insert(TypeId::of::<T>(), Arc::new(RwLock::new(service)));
        self
    }

    pub fn services(&self) -> &Arc<Services> {
        &self.services
    }

    pub fn window_manager(&self) -> &WindowViewManager {
        &self.wm
    }

    pub fn window_manager_mut(&mut self) -> &mut WindowViewManager {
        &mut self.wm
    }
}

pub enum ApplicationMessage {
    Window(WindowViewManagerMessage),
    WindowClosed(window::Id),
}

#[derive(Default)]
pub struct Services {
    services: RwLock<HashMap<TypeId, Arc<RwLock<dyn Service>>>>,
}

impl Services {
    pub fn service<T: Service>(&self) -> ServiceRef<T> {
        let arc = self
            .services
            .read()
            .get(&TypeId::of::<T>())
            .expect(&format!(
                "Service of type {} not found",
                std::any::type_name::<T>()
            ))
            .clone();
        ServiceRef::from_arc(arc)
    }

    pub fn service_mut<T: Service>(&self) -> ServiceMut<T> {
        let arc = self
            .services
            .read()
            .get(&TypeId::of::<T>())
            .expect(&format!(
                "Service of type {} not found",
                std::any::type_name::<T>()
            ))
            .clone();
        ServiceMut::from_arc(arc)
    }

    pub fn get_service<T: Service>(&self) -> Option<ServiceRef<T>> {
        self.services
            .read()
            .get(&TypeId::of::<T>())
            .map(|arc| ServiceRef::from_arc(arc.clone()))
    }

    pub fn get_service_mut<T: Service>(&self) -> Option<ServiceMut<T>> {
        self.services
            .read()
            .get(&TypeId::of::<T>())
            .map(|arc| ServiceMut::from_arc(arc.clone()))
    }

    pub fn insert_service<T: Service>(&self, service: T) {
        self.services
            .write()
            .insert(TypeId::of::<T>(), Arc::new(RwLock::new(service)));
    }
}
