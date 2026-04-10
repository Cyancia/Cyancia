use std::{collections::HashMap, sync::Arc};

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
};

pub mod brush;
pub mod canvas_control;
pub mod file;
pub mod actions_matcher;

pub struct ActionPlugin;

impl Plugin for ActionPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<ActionFunctionRegistry>()
            .add_action_function::<CanvasToolSwitch<PanToolAction>>()
            .add_action_function::<CanvasToolSwitch<RotateToolAction>>()
            .add_action_function::<CanvasToolSwitch<ZoomToolAction>>()
            .add_action_function::<CanvasToolSwitch<BrushToolAction>>()
            .add_action_function::<OpenFileAction>()
            .add_action_function::<OpenBrushEditorAction>();
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

pub trait ActionFunction: Send + Sync + 'static {
    fn id(&self) -> ActionId;
    fn trigger(&self, services: Arc<Services>) -> Task<()>;
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
