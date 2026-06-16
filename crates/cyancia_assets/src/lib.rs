use std::sync::Arc;

use gpui::App;

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

pub fn init(cx: &mut App) {
    cx.set_global(AssetRegistryBuilder::default());
}

pub fn finish(cx: &mut App) {
    let builder = cx.remove_global::<AssetRegistryBuilder>();
    cx.set_global(builder.build());
}

pub trait AssetAppExt {
    fn add_asset_serializer<A: AssetSerializer + Default>(&mut self);
    fn add_asset_bundle(&mut self, bundle: Arc<dyn ErasedAssetBundle>);
    fn assets(&self) -> &AssetRegistry;
}

impl AssetAppExt for App {
    fn add_asset_serializer<A: AssetSerializer + Default>(&mut self) {
        self.global_mut::<AssetRegistryBuilder>()
            .add_serializer::<A>();
    }

    fn add_asset_bundle(&mut self, bundle: Arc<dyn ErasedAssetBundle>) {
        self.global_mut::<AssetRegistryBuilder>().add_bundle(bundle);
    }

    fn assets(&self) -> &AssetRegistry {
        self.global::<AssetRegistry>()
    }
}
