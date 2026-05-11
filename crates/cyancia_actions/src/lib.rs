use std::{any::Any, collections::HashMap, sync::Arc};

use cyancia_input::{
    action::{Action, ActionId, ActionManifestCollection},
    key::{KeySequence, KeyboardState},
    mouse::PressedMouseState,
};
use cyancia_runtime::{Application, Services, plugin::Plugin, service::Service};
use iced_runtime::Task;

use crate::{
    brush::OpenBrushEditorAction,
    canvas_control::{
        BrushToolAction, CanvasToolSwitch, PanToolAction, RotateToolAction, ZoomToolAction,
    },
    file::OpenFileAction,
    layer::{CreateNewLayerAction, GroupActiveLayerAction, MoveLayerDownAction, MoveLayerUpAction},
};

pub mod actions_matcher;
pub mod brush;
pub mod canvas_control;
pub mod file;
pub mod layer;

pub struct ActionPlugin;

impl Plugin for ActionPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<ActionFunctionRegistry>()
            .add_action_function::<CanvasToolSwitch<PanToolAction>>()
            .add_action_function::<CanvasToolSwitch<RotateToolAction>>()
            .add_action_function::<CanvasToolSwitch<ZoomToolAction>>()
            .add_action_function::<CanvasToolSwitch<BrushToolAction>>()
            .add_action_function::<OpenFileAction>()
            .add_action_function::<CreateNewLayerAction>()
            .add_action_function::<MoveLayerUpAction>()
            .add_action_function::<MoveLayerDownAction>()
            .add_action_function::<GroupActiveLayerAction>()
            .add_action_function::<OpenBrushEditorAction>();
    }
}

pub trait ActionAppExt {
    fn add_action_function<A: ActionFunction + Default>(&mut self) -> &mut Self;
}

impl ActionAppExt for Application {
    fn add_action_function<A: ActionFunction + Default>(&mut self) -> &mut Self {
        self.runtime_mut()
            .services_mut()
            .service_mut::<ActionFunctionRegistry>()
            .register::<A>();
        self
    }
}

pub trait ActionFunction: Send + Sync + 'static {
    type Message: Send + Sync + 'static;

    fn id(&self) -> ActionId;
    fn trigger(&self, services: &mut Services) -> Task<Self::Message>;
    fn handle_message(
        &self,
        message: Self::Message,
        services: &mut Services,
    ) -> Task<Self::Message> {
        Task::none()
    }
}

pub trait ErasedActionFunction: Send + Sync + 'static {
    fn id(&self) -> ActionId;
    fn trigger(&self, services: &mut Services) -> Task<Box<dyn Any + Send + Sync>>;
    fn handle_message(
        &self,
        message: Box<dyn Any + Send + Sync>,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        Task::none()
    }
}

impl<T: ActionFunction> ErasedActionFunction for T {
    fn id(&self) -> ActionId {
        self.id()
    }

    fn trigger(&self, services: &mut Services) -> Task<Box<dyn Any + Send + Sync>> {
        self.trigger(services)
            .map(|message| Box::new(message) as Box<dyn Any + Send + Sync>)
    }

    fn handle_message(
        &self,
        message: Box<dyn Any + Send + Sync>,
        services: &mut Services,
    ) -> Task<Box<dyn Any + Send + Sync>> {
        let message = message
            .downcast::<T::Message>()
            .expect("Invalid message type");
        self.handle_message(*message, services)
            .map(|message| Box::new(message) as Box<dyn Any + Send + Sync>)
    }
}

#[derive(Default)]
pub struct ActionFunctionRegistry {
    functions: HashMap<ActionId, Arc<dyn ErasedActionFunction>>,
}

impl Service for ActionFunctionRegistry {}

impl ActionFunctionRegistry {
    pub fn register<A: ActionFunction + Default>(&mut self) {
        let action = A::default();
        self.functions.insert(action.id(), Arc::new(action));
    }

    pub fn get(&self, action_id: ActionId) -> Option<Arc<dyn ErasedActionFunction>> {
        self.functions.get(&action_id).cloned()
    }
}
