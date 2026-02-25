use cyancia_assets::loader::AssetSerializerRegistry;

use crate::action::ActionManifestLoader;

pub mod action;
pub mod key;
pub mod mouse;

// TODO: use plugin system
pub fn register_loaders(loaders: &mut AssetSerializerRegistry) {
    loaders.register::<ActionManifestLoader>();
}
