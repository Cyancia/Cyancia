use cyancia_assets::store::AssetLoaderRegistry;

use crate::action::ActionManifestLoader;

pub mod action;
pub mod key;
pub mod mouse;

pub fn register_loaders(loaders: &mut AssetLoaderRegistry) {
    loaders.register::<ActionManifestLoader>();
}
