use std::{
    any::TypeId,
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use iced_core::Theme;
use iced_core::{Element, window};
use iced_futures::{Subscription, backend::native};
use iced_runtime::Task;
use iced_wgpu::window::compositor::WgpuContext;
use iced_winit::program::Program;
use parking_lot::RwLock;

use crate::{
    plugin::Plugin,
    service::{FromRuntime, RenderContext, Service, ServiceMut, ServiceRef},
    windows::{ErasedWindowMessage, WindowManager, WindowViewId},
};

pub mod plugin;
pub mod service;
pub mod windows;

pub struct ApplicationProgram {
    build: Box<dyn Fn() -> Application>,
}

impl ApplicationProgram {
    pub fn new(build: impl Fn() -> Application + 'static) -> Self {
        Self {
            build: Box::new(build),
        }
    }

    pub fn run(self) -> Result<(), iced_winit::Error> {
        iced_winit::run(self)
    }
}

impl Program for ApplicationProgram {
    type State = Application;

    type Message = ApplicationMessage;

    type Theme = Theme;

    type Renderer = iced_wgpu::Renderer;

    type Executor = native::tokio::Executor;

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
        let mut app = (self.build)();
        app.add_service::<RenderContext>();
        app.prepare();
        // TODO ugly
        let task = app.wm.open_window(WindowViewId::new("main_view"));
        (app, task.discard())
    }

    fn update(&self, state: &mut Self::State, message: Self::Message) -> Task<Self::Message> {
        match message {
            ApplicationMessage::Window(m) => state
                .wm
                .update(m, &state.runtime)
                .map(ApplicationMessage::Window),
        }
    }

    fn view<'a>(
        &self,
        state: &'a Self::State,
        window: window::Id,
    ) -> Element<'a, Self::Message, Self::Theme, Self::Renderer> {
        state
            .wm
            .view(window, &state.runtime)
            .map(ApplicationMessage::Window)
    }

    fn subscription(&self, state: &Self::State) -> Subscription<Self::Message> {
        state.wm.subscription().map(ApplicationMessage::Window)
    }

    fn compositor_context(&self, state: &Self::State) -> Option<WgpuContext> {
        let render_context = state.runtime.service::<RenderContext>();
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
pub struct Application {
    plugins: Vec<Box<dyn Plugin>>,
    runtime: Runtime,
    wm: WindowManager,
}

impl Application {
    pub fn add_plugin<P: Plugin>(&mut self, plugin: P) -> &mut Self {
        self.plugins.push(Box::new(plugin));
        self
    }

    pub fn add_service<T: Service + FromRuntime>(&mut self) -> &mut Self {
        self.runtime.services.insert(
            TypeId::of::<T>(),
            Arc::new(RwLock::new(T::from_runtime(&self.runtime))),
        );
        self
    }

    pub fn add_service_instance<T: Service>(&mut self, service: T) -> &mut Self {
        self.runtime
            .services
            .insert(TypeId::of::<T>(), Arc::new(RwLock::new(service)));
        self
    }

    pub fn prepare(&mut self) {
        for plugin in std::mem::take(&mut self.plugins) {
            plugin.build(self);
        }
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    pub fn window_manager(&self) -> &WindowManager {
        &self.wm
    }

    pub fn window_manager_mut(&mut self) -> &mut WindowManager {
        &mut self.wm
    }
}

pub enum ApplicationMessage {
    Window(ErasedWindowMessage),
}

#[derive(Default)]
pub struct Runtime {
    services: HashMap<TypeId, Arc<RwLock<dyn Service>>>,
}

impl Runtime {
    pub fn service<T: Service>(&self) -> ServiceRef<'_, T> {
        let x = self
            .services
            .get(&TypeId::of::<T>())
            .expect(&format!(
                "Service of type {} not found",
                std::any::type_name::<T>()
            ))
            .read();
        ServiceRef::from_dynamic(x)
    }

    pub fn service_mut<T: Service>(&self) -> ServiceMut<'_, T> {
        let x = self
            .services
            .get(&TypeId::of::<T>())
            .expect(&format!(
                "Service of type {} not found",
                std::any::type_name::<T>()
            ))
            .write();
        ServiceMut::from_dynamic(x)
    }
}
