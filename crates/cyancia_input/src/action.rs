use std::{borrow::Borrow, collections::HashMap, sync::Arc};

use cyancia_assets::{
    asset::{Asset, AssetId, UntypedAssetId},
    loader::AssetSerializer,
};
use cyancia_utils::wrapper;
use serde::{Deserialize, Serialize};

use crate::key::KeySequence;

#[derive(Debug, Clone)]
pub struct Action {
    pub name: Arc<str>,
    pub shortcut: Vec<KeySequence>,
    pub priority: u8,
}

wrapper! {
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub ActionId : Arc<str>
}

#[derive(Debug, Clone)]
pub struct ActionManifest {
    pub actions: Vec<Action>,
}

impl Asset for ActionManifest {
    const TYPE_NAME: &'static str = "action_manifest";
}

#[derive(Serialize, Deserialize)]
pub struct SerializableAction {
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
        let actions = toml::from_slice::<HashMap<String, SerializableAction>>(&buf)?
            .into_iter()
            .map(|(name, a)| Action {
                name: Arc::from(name),
                shortcut: a.shortcut,
                priority: a.priority.unwrap_or(0),
            })
            .collect();
        Ok(ActionManifest { actions })
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        let actions = asset
            .actions
            .iter()
            .map(|a| {
                (
                    a.name.to_string(),
                    SerializableAction {
                        shortcut: a.shortcut.clone(),
                        priority: if a.priority == 0 {
                            None
                        } else {
                            Some(a.priority)
                        },
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let toml = toml::to_string(&actions)?;
        writer.write_all(toml.as_bytes())?;
        Ok(())
    }
}

pub struct ActionCollection {
    shortcuts: HashMap<KeySequence, Vec<ActionId>>,
    actions: HashMap<ActionId, Arc<Action>>,
}

impl ActionCollection {
    pub fn new(manifests: impl IntoIterator<Item = impl Borrow<ActionManifest>>) -> Self {
        let actions = manifests
            .into_iter()
            .flat_map(|manifest| manifest.borrow().actions.clone())
            .map(|action| (ActionId::new(action.name.clone()), Arc::new(action)))
            .collect::<HashMap<_, _>>();
        let mut shortcuts = actions.iter().fold(
            HashMap::<KeySequence, Vec<ActionId>>::default(),
            |mut acc, (id, a)| {
                for shortcut in &a.shortcut {
                    acc.entry(*shortcut).or_default().push(id.clone());
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
