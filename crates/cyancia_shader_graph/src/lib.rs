use cyancia_assets::AssetAppExt;
use cyancia_render::texture::Image;
use gpui::App;

use crate::{
    graph::{
        function::{ASSET_GRAPH_FUNCTION_STORAGE, GraphFunctionStorage},
        texture::{ASSET_GRAPH_TEXTURE_STORAGE, GraphTextureStorage},
    },
    save::{SerializableGraphFunction, SerializableGraphFunctionSerializer},
};

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

pub fn finish(cx: &mut App) {
    ASSET_GRAPH_TEXTURE_STORAGE
        .store(GraphTextureStorage::new(cx.assets().all_handles_of::<Image>().unwrap()).into());
    ASSET_GRAPH_FUNCTION_STORAGE.store(
        GraphFunctionStorage::new(
            ASSET_GRAPH_TEXTURE_STORAGE.clone(),
            // This cyclic dependency is allowed because function node only stores the id of function,
            // so the storage is not going to be used during deserialization, but only code generation.
            ASSET_GRAPH_FUNCTION_STORAGE.clone(),
            cx.assets()
                .all_handles_of::<SerializableGraphFunction>()
                .unwrap(),
            cx,
        )
        .into(),
    );
}

// pub struct ShaderGraphPlugin;

// impl Plugin for ShaderGraphPlugin {
//     fn build(&self, app: &mut Application) {
//         app.add_asset_serializer::<SerializableGraphFunctionSerializer>();
//     }
// }
