use std::{any::Any, cell::UnsafeCell, collections::HashMap, sync::Arc};

use async_trait::async_trait;
use cyancia_input::{
    action::{Action, ActionId, ActionManifestCollection},
    key::{KeySequence, KeyboardState},
    mouse::PressedMouseState,
};
use cyancia_runtime::{Application, Runtime, Services, plugin::Plugin, service::Service};
use iced_core::Point;
use parking_lot::RwLock;

use crate::{
    canvas_control::{CanvasToolSwitch, PanToolAction, RotateToolAction, ZoomToolAction},
    file::OpenFileAction,
};

pub mod canvas_control;
pub mod file;

pub struct ActionPlugin;

impl Plugin for ActionPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<ActionFunctionRegistry>()
            .add_action_function::<CanvasToolSwitch<PanToolAction>>()
            .add_action_function::<CanvasToolSwitch<RotateToolAction>>()
            .add_action_function::<CanvasToolSwitch<ZoomToolAction>>()
            .add_action_function::<OpenFileAction>();
    }
}

pub trait ActionAppExt {
    fn add_action_function<A: ActionFunction + Default>(&mut self) -> &mut Self;
}

impl ActionAppExt for Application {
    fn add_action_function<A: ActionFunction + Default>(&mut self) -> &mut Self {
        self.runtime()
            .services()
            .service_mut::<ActionFunctionRegistry>()
            .register::<A>();
        self
    }
}

#[async_trait]
pub trait ActionFunction: Send + Sync + 'static {
    fn id(&self) -> ActionId;
    async fn trigger(&self, services: Arc<Services>);
}

#[derive(Default)]
pub struct ActionFunctionRegistry {
    functions: HashMap<ActionId, Arc<dyn ActionFunction>>,
}

impl Service for ActionFunctionRegistry {}

impl ActionFunctionRegistry {
    pub fn register<A: ActionFunction + Default>(&mut self) {
        let action = A::default();
        self.functions.insert(action.id(), Arc::new(action));
    }

    pub fn get(&self, action_id: ActionId) -> Option<Arc<dyn ActionFunction>> {
        self.functions.get(&action_id).cloned()
    }
}
