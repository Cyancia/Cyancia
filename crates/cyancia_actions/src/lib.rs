use std::collections::HashMap;

use cyancia_assets::AssetAppExt;
use gpui::{Action, App, Global, KeyBinding, KeyBindingContextPredicate};

use crate::{
    brush::OpenBrushEditorAction,
    file::OpenFileAction,
    layer::{
        CreateNewLayerAction, DeleteSelectedLayersAction, GroupSelectedLayersAction,
        MoveLayerDownAction, MoveLayerUpAction, PasteIntoNewLayerAction, SelectNextLayerAction,
        SelectPreviousLayerAction,
    },
    manifest::{KeyBindingDefManifest, KeyBindingDefManifestLoader},
    selection::DeleteSelectionAction,
    undo::{RedoAction, UndoAction},
};

pub mod brush;
pub mod file;
pub mod layer;
pub mod manifest;
pub mod selection;
pub mod undo;

// pub struct ActionPlugin;

// impl Plugin for ActionPlugin {
//     fn build(&self, app: &mut Application) {
//         app.add_service::<ActionFunctionRegistry>()
//             .add_action_function::<CanvasToolSwitch<PanToolAction>>()
//             .add_action_function::<CanvasToolSwitch<RotateToolAction>>()
//             .add_action_function::<CanvasToolSwitch<ZoomToolAction>>()
//             .add_action_function::<CanvasToolSwitch<BrushToolAction>>()
//             .add_action_function::<OpenFileAction>()
//             .add_action_function::<CreateNewLayerAction>()
//             .add_action_function::<MoveLayerUpAction>()
//             .add_action_function::<MoveLayerDownAction>()
//             .add_action_function::<GroupActiveLayerAction>()
//             .add_action_function::<OpenBrushEditorAction>();
//     }
// }

pub fn init(cx: &mut App) {
    cx.add_asset_serializer::<KeyBindingDefManifestLoader>();
    cx.set_global(ActionFunctionRegistry::default());
    cx.add_action_function::<DeleteSelectionAction>();
    cx.add_action_function::<OpenFileAction>();
    cx.add_action_function::<CreateNewLayerAction>();
    cx.add_action_function::<MoveLayerUpAction>();
    cx.add_action_function::<MoveLayerDownAction>();
    cx.add_action_function::<DeleteSelectedLayersAction>();
    cx.add_action_function::<GroupSelectedLayersAction>();
    cx.add_action_function::<SelectNextLayerAction>();
    cx.add_action_function::<SelectPreviousLayerAction>();
    cx.add_action_function::<PasteIntoNewLayerAction>();
    cx.add_action_function::<OpenBrushEditorAction>();
    cx.add_action_function::<UndoAction>();
    cx.add_action_function::<RedoAction>();
}

pub fn finish(cx: &mut App) {
    let manifests = cx
        .assets()
        .all_handles_of::<KeyBindingDefManifest>()
        .unwrap();
    let functions = cx.global::<ActionFunctionRegistry>();

    // TODO Use the first manifest currently. In the future, this should be confuguable.
    let manifest_handle = manifests.first().expect("No keybinding manifest available");
    let manifest = manifest_handle.get().unwrap();
    log::info!(
        "Loading {} key bindings from manifest {}",
        manifest.actions.len(),
        manifest.name
    );
    let key_bindings = manifest
        .actions
        .iter()
        .map(|def| {
            let function_parser = functions
                .functions
                .get(def.action_name.as_str())
                .ok_or_else(|| anyhow::anyhow!("Action {} not found.", def.action_name))?;
            let context = if let Some(context) = &def.context {
                Some(KeyBindingContextPredicate::parse(context)?.into())
            } else {
                None
            };

            Ok::<KeyBinding, anyhow::Error>(KeyBinding::load(
                &def.shortcut,
                function_parser(def.action_data.clone())?,
                context,
                true,
                None,
                cx.keyboard_mapper().as_ref(),
            )?)
        })
        .enumerate()
        .filter_map(|(i, binding)| match binding {
            Ok(b) => Some(b),
            Err(e) => {
                let def = manifest.actions.get(i)?;
                log::error!(
                    "Error loading keybinding {} triggered by {} with context {:?} and data {}: {}",
                    def.action_name,
                    def.shortcut,
                    def.context,
                    def.action_data,
                    e
                );
                None
            }
        })
        .collect::<Vec<_>>();
    cx.bind_keys(key_bindings);
}

pub trait ActionAppExt {
    fn add_action_function<A: ActionFunction>(&mut self);
}

impl ActionAppExt for App {
    fn add_action_function<A: ActionFunction>(&mut self) {
        self.global_mut::<ActionFunctionRegistry>().register::<A>();
        self.on_action::<A>(|f, cx| {
            log::info!("Action triggered from keymap: {}", f.name());
            f.trigger(cx);
        });
    }
}

pub trait ActionFunction: Action + Send + Sync + 'static {
    fn trigger(&self, cx: &mut App);
}

#[derive(Default)]
pub struct ActionFunctionRegistry {
    functions: HashMap<
        &'static str,
        Box<dyn Fn(serde_json::Value) -> Result<Box<dyn Action>, anyhow::Error>>,
    >,
}

impl Global for ActionFunctionRegistry {}

impl ActionFunctionRegistry {
    pub fn register<A: ActionFunction>(&mut self) {
        self.functions
            .insert(A::name_for_type(), Box::new(A::build));
    }
}
