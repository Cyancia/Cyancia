use std::{path::PathBuf, sync::Arc};

use lapiz_runtime::{Application, Services, plugin::Plugin};

use crate::{
    bundle::ErasedAssetBundle,
    loader::{AssetRegistryBuilder, AssetSerializer},
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
        let mut builder = AssetRegistryBuilder::default();
        builder.set_root(self.asset_root.clone());

        for bundle in &self.bundles {
            builder.add_bundle(bundle.clone());
        }

        app.add_service_instance(builder);
    }

    fn finish(&self, app: &mut Application) {
        let builder = app
            .runtime_mut()
            .services_mut()
            .remove_service::<AssetRegistryBuilder>();
        app.add_service_instance(builder.build());
    }
}

pub trait AssetAppExt {
    fn add_asset_serializer<A: AssetSerializer + Default>(&mut self);
    fn add_asset_bundle(&mut self, bundle: Arc<dyn ErasedAssetBundle>);
    fn assets(&self) -> &AssetRegistry;
}

impl AssetAppExt for Services {
    fn add_asset_serializer<A: AssetSerializer + Default>(&mut self) {
        self.service_mut::<AssetRegistryBuilder>()
            .add_serializer::<A>();
    }

    fn add_asset_bundle(&mut self, bundle: Arc<dyn ErasedAssetBundle>) {
        self.service_mut::<AssetRegistryBuilder>()
            .add_bundle(bundle);
    }

    fn assets(&self) -> &AssetRegistry {
        self.service::<AssetRegistry>()
    }
}
