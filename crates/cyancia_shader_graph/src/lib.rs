use cyancia_assets::AssetAppExt;
use gpui::App;

use crate::save::SerializableGraphFunctionSerializer;

pub mod editor;
pub mod graph;
pub mod save;
pub mod wgsl_std;

pub type GraphSerializer<'a> = toml::Serializer<'a>;
pub type GraphDeserializer<'a> = toml::de::Deserializer<'a>;

pub fn init(cx: &mut App) {
    editor::init(cx);
    cx.add_asset_serializer::<SerializableGraphFunctionSerializer>();
}

// pub struct ShaderGraphPlugin;

// impl Plugin for ShaderGraphPlugin {
//     fn build(&self, app: &mut Application) {
//         app.add_asset_serializer::<SerializableGraphFunctionSerializer>();
//     }
// }
