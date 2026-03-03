use std::{
    borrow::Borrow,
    collections::{HashMap, hash_map::Entry},
    sync::Arc,
};

use cyancia_assets::{
    asset::{Asset, AssetId, UntypedAssetId},
    loader::AssetSerializer,
    store::AssetRegistry,
};
use cyancia_runtime::{
    Services,
    service::{FromRuntime, Service},
};
use cyancia_utils::wrapper;
use futures::executor::block_on;
use parse_display::Display;
use serde::{Deserialize, Serialize};

use crate::key::KeySequence;

#[derive(Debug, Clone)]
pub struct Action {
    pub name: ActionId,
    pub shortcut: Vec<KeySequence>,
    pub priority: u8,
}

wrapper! {
    #[derive(Debug, Clone, PartialEq, Eq, Hash, Display)]
    #[display("{0}")]
    pub ActionId : Arc<str>
}

#[derive(Debug, Clone)]
pub struct ActionManifest {
    pub actions_in_view: HashMap<String, Vec<Arc<Action>>>,
}

impl Asset for ActionManifest {
    const TYPE_NAME: &'static str = "action_manifest";
}

#[derive(Serialize, Deserialize)]
pub struct SerializableAction {
    pub shortcut: Vec<KeySequence>,
    #[serde(default)]
    pub priority: Option<u8>,
    pub enabled_in: Vec<String>,
}

#[derive(Default)]
pub struct ActionManifestLoader;

#[derive(Debug, thiserror::Error)]
pub enum ActionManifestLoaderError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSer(#[from] toml::ser::Error),
}

impl AssetSerializer for ActionManifestLoader {
    type Asset = ActionManifest;

    type Error = ActionManifestLoaderError;

    fn file_extension() -> &'static str {
        "actions"
    }

    fn read(&self, reader: &mut dyn std::io::Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let manifest = toml::from_slice::<HashMap<String, SerializableAction>>(&buf)?;
        let mut actions_in_view = HashMap::new();
        for (name, action) in manifest {
            let action_arc = Arc::new(Action {
                name: ActionId::new(name.into()),
                shortcut: action.shortcut.clone(),
                priority: action.priority.unwrap_or(0),
            });

            for view in action.enabled_in.iter().cloned() {
                actions_in_view
                    .entry(view)
                    .or_insert_with(Vec::new)
                    .push(action_arc.clone());
            }
        }
        Ok(ActionManifest { actions_in_view })
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        let mut manifest = HashMap::<String, SerializableAction>::new();
        for (view, actions) in &asset.actions_in_view {
            for action in actions {
                match manifest.entry(action.name.to_string()) {
                    Entry::Occupied(mut e) => {
                        e.get_mut().enabled_in.push(view.clone());
                    }
                    Entry::Vacant(e) => {
                        e.insert(SerializableAction {
                            shortcut: action.shortcut.clone(),
                            priority: Some(action.priority),
                            enabled_in: vec![view.clone()],
                        });
                    }
                }
            }
        }
        let toml = toml::to_string(&manifest)?;
        writer.write_all(toml.as_bytes())?;
        Ok(())
    }
}

pub struct ActionManifestCollection {
    manifests: Vec<Arc<ActionManifest>>,
}

impl Service for ActionManifestCollection {}

impl FromRuntime for ActionManifestCollection {
    fn from_runtime(runtime: &Services) -> Self {
        let assets = runtime.service::<AssetRegistry>();
        let handles = assets.all_handles_of::<ActionManifest>().unwrap();
        let manifests = handles
            .into_iter()
            .map(|handle| handle.get().unwrap())
            .collect();

        Self::new(manifests)
    }
}

impl ActionManifestCollection {
    pub fn new(manifests: Vec<Arc<ActionManifest>>) -> Self {
        Self { manifests }
    }

    pub fn subset_for_view(&self, view: &str) -> ActionCollection {
        let mut shortcuts = HashMap::new();
        let mut actions = HashMap::new();

        for manifest in &self.manifests {
            let Some(manifest) = manifest.actions_in_view.get(view) else {
                continue;
            };

            for action in manifest {
                for shortcut in &action.shortcut {
                    shortcuts
                        .entry(shortcut.clone())
                        .or_insert_with(Vec::new)
                        .push(action.name.clone());
                }
                actions.insert(action.name.clone(), action.clone());
            }
        }

        for shortcuts in shortcuts.values_mut() {
            shortcuts.sort_by_key(|id| actions.get(id).map(|a| a.priority).unwrap_or(0));
        }

        ActionCollection { shortcuts, actions }
    }
}

pub struct ActionCollection {
    shortcuts: HashMap<KeySequence, Vec<ActionId>>,
    actions: HashMap<ActionId, Arc<Action>>,
}

impl ActionCollection {
    pub fn get_action_id(&self, shortcut: KeySequence) -> Option<ActionId> {
        let ids = self.shortcuts.get(&shortcut)?;
        ids.first().cloned()
    }

    pub fn get_action(&self, id: ActionId) -> Option<Arc<Action>> {
        self.actions.get(&id).cloned()
    }

    pub fn get_all_action_ids(&self, shortcut: KeySequence) -> Option<Vec<ActionId>> {
        self.shortcuts.get(&shortcut).cloned()
    }
}
