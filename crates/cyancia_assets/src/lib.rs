use std::path::PathBuf;

use cyancia_runtime::{Application, plugin::Plugin};

use crate::{
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
    pub serializers: AssetSerializerRegistry,
}

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut Application) {
        app.add_service::<AssetSerializerRegistryBuilder>();
    }

    fn finish(&self, app: &mut Application) {
        let mut builder = app
            .runtime()
            .service_mut::<AssetSerializerRegistryBuilder>();
        let serializers = builder.build();
        drop(builder);
        app.add_service_instance(AssetRegistry::new(&self.asset_root, serializers.into()).unwrap());
    }
}
