use std::io::Read;

use crate::{
    asset::{Asset, ErasedAsset},
    id::UntypedAssetId,
    store::AssetRegistry,
};

pub trait AssetLoader: Send + Sync + 'static {
    type Asset: Asset;
    type Error: std::error::Error;
    fn file_extensions() -> &'static [&'static str];
    fn read(&self, reader: &mut dyn Read) -> Result<Self::Asset, Self::Error>;
}

pub trait ErasedAssetLoader: Send + Sync + 'static {
    fn read(&self, reader: &mut dyn Read) -> Result<Box<dyn Asset>, Box<dyn std::error::Error>>;
    fn insert_asset(&self, id: UntypedAssetId, asset: Box<dyn Asset>, assets: &mut AssetRegistry);
}

impl<T: AssetLoader> ErasedAssetLoader for T {
    fn read(&self, reader: &mut dyn Read) -> Result<Box<dyn Asset>, Box<dyn std::error::Error>> {
        match <Self as AssetLoader>::read(self, reader) {
            Ok(a) => Ok(Box::new(a)),
            Err(e) => Err(Box::new(e)),
        }
    }

    fn insert_asset(&self, id: UntypedAssetId, asset: Box<dyn Asset>, assets: &mut AssetRegistry) {
        assets.init_store::<<Self as AssetLoader>::Asset>();
        let id = id.typed::<T::Asset>().unwrap();
        let asset = match asset.downcast::<T::Asset>() {
            Ok(a) => a,
            Err(_) => unreachable!(),
        };
        assets
            .store_mut::<<Self as AssetLoader>::Asset>()
            .insert(id, asset.into());
    }
}
