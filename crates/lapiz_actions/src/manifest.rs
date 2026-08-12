use std::collections::HashMap;
use std::io::{Read, Write};

use lapiz_assets::{asset::Asset, loader::AssetSerializer};
use lapiz_input::key::KeySequence;
use serde::{Deserialize, Serialize};

use crate::ActionId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingDef {
    pub shortcut: KeySequence,
    pub action_name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_null")]
    pub action_data: serde_json::Value,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_none")]
    pub context: Option<String>,
}

fn is_null(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::Null)
}

fn is_none<T>(value: &Option<T>) -> bool {
    value.is_none()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingDefManifest {
    pub name: String,
    pub actions: Vec<KeyBindingDef>,
}

impl Asset for KeyBindingDefManifest {
    const TYPE_NAME: &'static str = "key_bindings";
}

#[derive(Default)]
pub struct KeyBindingDefManifestLoader;

#[derive(Debug, thiserror::Error)]
pub enum KeyBindingDefManifestLoaderError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    String(#[from] std::string::FromUtf8Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl AssetSerializer for KeyBindingDefManifestLoader {
    type Asset = KeyBindingDefManifest;

    type Error = KeyBindingDefManifestLoaderError;

    fn file_extension() -> &'static str {
        "actions"
    }

    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let manifest: KeyBindingDefManifest = serde_json::from_slice(&buf)?;
        Ok(manifest)
    }

    fn write(&self, asset: &Self::Asset, writer: &mut dyn Write) -> Result<(), Self::Error> {
        let json = serde_json::to_string(asset)?;
        writer.write_all(json.as_bytes())?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ActionCollection {
    shortcuts: HashMap<KeySequence, ActionId>,
}

impl ActionCollection {
    pub fn new(manifest: &KeyBindingDefManifest) -> Self {
        let mut shortcuts = HashMap::new();

        for def in &manifest.actions {
            shortcuts.insert(def.shortcut, ActionId::new(def.action_name.clone().into()));
        }

        Self { shortcuts }
    }

    pub fn get_action_id(&self, shortcut: KeySequence) -> Option<ActionId> {
        self.shortcuts.get(&shortcut).cloned()
    }
}
