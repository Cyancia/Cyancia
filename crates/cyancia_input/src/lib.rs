use cyancia_assets::loader::{AssetSerializerRegistry, AssetSerializerRegistryBuilder};
use cyancia_runtime::{Application, Runtime, plugin::Plugin};

use crate::action::{ActionManifestCollection, ActionManifestLoader};

pub mod action;
pub mod key;
pub mod mouse;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut Application) {
        app.runtime()
            .services()
            .service_mut::<AssetSerializerRegistryBuilder>()
            .add_serializer::<ActionManifestLoader>();
    }

    fn finish(&self, app: &mut Application) {
        app.add_service::<ActionManifestCollection>();
    }
}
