use cyancia_assets::AssetAppExt;
use cyancia_runtime::{Application, plugin::Plugin};

use crate::save::SerializableGraphFunctionSerializer;

pub mod editor;
pub mod graph;
pub mod save;
pub mod wgsl_std;

pub type GraphTheme = iced_core::Theme;
pub type GraphRenderer = iced_wgpu::Renderer;
pub type GraphSerializer<'a> = toml::Serializer<'a>;
pub type GraphDeserializer<'a> = toml::de::Deserializer<'a>;

pub struct ShaderGraphPlugin;

impl Plugin for ShaderGraphPlugin {
    fn build(&self, app: &mut Application) {
        app.add_asset_serializer::<SerializableGraphFunctionSerializer>();
    }
}
