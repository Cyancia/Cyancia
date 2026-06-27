use cyancia_assets::{asset::Asset, loader::AssetSerializer};
use gpui::InvalidKeystrokeError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingDef {
    pub shortcut: String,
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
    InvalidKeystroke(#[from] InvalidKeystrokeError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl AssetSerializer for KeyBindingDefManifestLoader {
    type Asset = KeyBindingDefManifest;

    type Error = KeyBindingDefManifestLoaderError;

    fn file_extension() -> &'static str {
        "actions"
    }

    fn read(&self, reader: &mut dyn std::io::Read) -> Result<Self::Asset, Self::Error> {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf)?;
        let manifest: KeyBindingDefManifest = serde_json::from_slice(&buf)?;
        Ok(manifest)
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        let json = serde_json::to_string(asset)?;
        writer.write_all(json.as_bytes())?;
        Ok(())
    }
}
