use std::{path::PathBuf, sync::Arc};

use cyancia_runtime::{Application, Runtime, plugin::Plugin};
use futures::executor::block_on;

use crate::{
    bundle::ErasedAssetBundle,
    loader::{AssetSerializerRegistry, AssetSerializerRegistryBuilder},
    store::AssetRegistry,
};

pub mod asset;
pub mod bundle;
pub mod error;
pub mod index_db;
pub mod loader;
pub mod store;
pub mod tag;

pub struct AssetsPlugin {
    pub asset_root: PathBuf,
    pub bundles: Vec<Arc<dyn ErasedAssetBundle>>,
}

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<AssetSerializerRegistryBuilder>();
    }

    fn finish(&self, app: &mut Application) {
        let mut builder = app
            .runtime()
            .services()
            .service_mut::<AssetSerializerRegistryBuilder>();
        let serializers = builder.consume_and_build();
        drop(builder);
        let mut registry = AssetRegistry::new(&self.asset_root, serializers.into()).unwrap();
        for bundle in self.bundles.clone() {
            registry.add_erased_bundle(bundle).unwrap();
        }
        app.add_service_instance(registry);
    }
}
