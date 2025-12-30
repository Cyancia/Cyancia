use std::{collections::HashMap, sync::Arc};

use cyancia_assets::{asset::Asset, loader::AssetLoader, store::AssetStore};
use cyancia_id::Id;
use serde::{Deserialize, Serialize};

use crate::key::KeySequence;

pub mod matching;

#[derive(Debug, Clone)]
pub struct Action {
    pub name: Arc<str>,
    pub ty: ActionType,
    pub shortcut: Vec<KeySequence>,
    pub priority: u8,
}

impl Asset for Action {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionType {
    /// A action that triggers once per shortcut activation. Like inverting a layer.
    OneShot,
    /// A action that triggers on shortcut activation.
    /// If the shortcut is held for more than 0.2 seconds before deactivation,
    /// it will deactivate on shortcut release. Otherwise it will remain active until
    /// another shortcut is activated.
    /// Like tools, e.g. straight line tool.
    Toggle,
    /// A action that only remains active while the shortcut is held down.
    /// Like panning or zooming the canvas.
    Hold,
}

#[derive(Debug, Clone)]
pub struct ActionManifest {
    pub actions: Vec<Action>,
}

impl Asset for ActionManifest {}

#[derive(Serialize, Deserialize)]
pub struct SerializableAction {
    pub ty: ActionType,
    pub shortcut: Vec<KeySequence>,
    #[serde(default)]
    pub priority: Option<u8>,
}

#[derive(Default)]
pub struct ActionManifestLoader;

#[derive(Debug, thiserror::Error)]
pub enum ActionManifestLoaderError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
}

impl AssetLoader for ActionManifestLoader {
    type Asset = ActionManifest;

    type Error = ActionManifestLoaderError;

    fn file_extensions() -> &'static [&'static str] {
        &["actions"]
    }

    fn read(&self, reader: &mut dyn std::io::Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let actions = toml::from_slice::<HashMap<String, SerializableAction>>(&buf)?
            .into_iter()
            .map(|(name, a)| Action {
                name: Arc::from(name),
                ty: a.ty,
                shortcut: a.shortcut,
                priority: a.priority.unwrap_or(0),
            })
            .collect();
        Ok(ActionManifest { actions })
    }
}

pub struct ActionCollection {
    shortcuts: HashMap<KeySequence, Vec<Id<Action>>>,
    actions: HashMap<Id<Action>, Arc<Action>>,
}

impl ActionCollection {
    pub fn new(manifests: AssetStore<ActionManifest>) -> Self {
        let actions = manifests
            .into_map()
            .into_iter()
            .flat_map(|(_, manifest)| manifest.actions.clone())
            .map(|action| (Id::from_str(&action.name), Arc::new(action)))
            .collect::<HashMap<_, _>>();
        let mut shortcuts = actions.iter().fold(
            HashMap::<KeySequence, Vec<Id<Action>>>::default(),
            |mut acc, (id, a)| {
                for shortcut in &a.shortcut {
                    acc.entry(*shortcut).or_default().push(*id);
                }
                acc
            },
        );

        for ids in shortcuts.values_mut() {
            if ids.len() > 1 {
                ids.sort_by_key(|a| actions.get(a).unwrap().priority);
            }
        }

        Self { shortcuts, actions }
    }

    pub fn get_action_id(&self, shortcut: KeySequence) -> Option<Id<Action>> {
        let ids = self.shortcuts.get(&shortcut)?;
        ids.first().cloned()
    }

    pub fn get_action(&self, id: Id<Action>) -> Option<Arc<Action>> {
        self.actions.get(&id).cloned()
    }

    pub fn get_all_action_ids(&self, shortcut: KeySequence) -> Option<Vec<Id<Action>>> {
        self.shortcuts.get(&shortcut).cloned()
    }
}
