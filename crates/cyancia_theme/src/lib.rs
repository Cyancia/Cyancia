use std::io::{Read, Write};

use cyancia_assets::{AssetAppExt, asset::Asset, loader::AssetSerializer};
use gpui::{Action, App, SharedString};
use gpui_component::{Theme, ThemeRegistry};

pub fn init(cx: &mut App) {
    cx.add_asset_serializer::<ThemeAssetSerializer>();

    cx.on_action(|switch: &SwitchThemeAction, cx| {
        if let Some(theme_config) = ThemeRegistry::global(cx)
            .themes()
            .get(&switch.theme)
            .cloned()
        {
            Theme::global_mut(cx).apply_config(&theme_config);
        }
        cx.refresh_windows();
    });
}

pub fn finish(cx: &mut App) {
    let themes = cx.assets().all_handles_of::<ThemeAsset>().unwrap();
    let registry = ThemeRegistry::global_mut(cx);
    for handle in themes {
        let theme = handle.get().unwrap();
        registry.load_themes_from_str(&theme.content).unwrap();
    }
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = theme, no_json)]
pub struct SwitchThemeAction {
    pub theme: SharedString,
}

pub struct ThemeAsset {
    content: String,
}

impl Asset for ThemeAsset {
    const TYPE_NAME: &'static str = "theme";
}

#[derive(Default)]
pub struct ThemeAssetSerializer;

#[derive(Debug, thiserror::Error)]
pub enum ThemeAssetSerializerError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    String(#[from] std::string::FromUtf8Error),
}

impl AssetSerializer for ThemeAssetSerializer {
    type Asset = ThemeAsset;

    type Error = ThemeAssetSerializerError;

    fn file_extension() -> &'static str {
        "theme"
    }

    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error> {
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        Ok(ThemeAsset {
            content: String::from_utf8(data)?,
        })
    }

    fn write(&self, asset: &Self::Asset, writer: &mut dyn Write) -> Result<(), Self::Error> {
        writer.write_all(asset.content.as_bytes())?;
        Ok(())
    }
}
