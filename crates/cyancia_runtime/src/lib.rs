use std::{
    any::TypeId,
    collections::HashMap,
    sync::{Arc, Mutex},
};

use iced_core::Theme;
use iced_core::{Element, window};
use iced_futures::{Subscription, backend::native};
use iced_runtime::Task;
use iced_winit::program::Program;

use crate::{
    service::Service,
    windows::{ErasedWindowMessage, WindowManager},
};

pub mod service;
pub mod windows;

pub struct RuntimeProgram {
    runtime: Arc<Runtime>,
}

impl Program for RuntimeProgram {
    type State = Arc<Runtime>;

    type Message = RuntimeMessage;

    type Theme = Theme;

    type Renderer = iced_wgpu::Renderer;

    type Executor = native::tokio::Executor;

    fn name() -> &'static str {
        "Cyancia Runtime"
    }

    fn settings(&self) -> iced_core::Settings {
        Default::default()
    }

    fn window(&self) -> Option<iced_core::window::Settings> {
        None
    }

    fn boot(&self) -> (Self::State, Task<Self::Message>) {
        (self.runtime.clone(), Task::none())
    }

    fn update(&self, state: &mut Self::State, message: Self::Message) -> Task<Self::Message> {
        match message {
            RuntimeMessage::Window(m) => {
                let windows = state.service_ref::<WindowManager>();
                windows.update(m, state).map(RuntimeMessage::Window)
            }
        }
    }

    fn view<'a>(
        &self,
        state: &'a Self::State,
        window: window::Id,
    ) -> Element<'a, Self::Message, Self::Theme, Self::Renderer> {
        let windows = state.service_ref::<WindowManager>();
        windows.view(window, state).map(RuntimeMessage::Window)
    }

    fn subscription(&self, state: &Self::State) -> Subscription<Self::Message> {
        let windows = state.service_ref::<WindowManager>();
        windows.subscription().map(RuntimeMessage::Window)
    }
}

pub struct Runtime {
    services: HashMap<TypeId, Arc<dyn Service>>,
}

impl Default for Runtime {
    fn default() -> Self {
        let mut runtime = Self {
            services: HashMap::new(),
        };

        runtime.add_service(WindowManager::new());

        runtime
    }
}

pub enum RuntimeMessage {
    Window(ErasedWindowMessage),
}

impl Runtime {
    pub fn add_service<T: Service>(&mut self, service: T) {
        self.services.insert(TypeId::of::<T>(), Arc::new(service));
    }

    pub fn service<T: Service>(&self) -> Arc<T> {
        self.services
            .get(&TypeId::of::<T>())
            .expect("Service not found")
            .clone()
            .downcast_arc::<T>()
            .unwrap_or_else(|_| unreachable!())
    }

    pub fn service_ref<T: Service>(&self) -> &T {
        self.services
            .get(&TypeId::of::<T>())
            .expect("Service not found")
            .as_ref()
            .downcast_ref::<T>()
            .unwrap_or_else(|| unreachable!())
    }

    pub fn run(self) {
        iced_winit::run(RuntimeProgram {
            runtime: Arc::new(self),
        })
        .unwrap()
    }
}
