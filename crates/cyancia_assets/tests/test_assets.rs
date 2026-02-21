use std::{
    fmt::Debug,
    hash::{BuildHasher, RandomState},
    io::read_to_string,
    str::FromStr,
};

use cyancia_assets::{
    asset::{Asset, AssetId, AssetUrl},
    bundle::{AssetBundle, BundleId, directory::AssetDirectory, standard::StandardAssetBundle},
    index_db::AssetFilter,
    loader::{AssetSerializer, AssetSerializerRegistry},
    store::AssetRegistry,
    tag::{TagId, TagSerializer},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
struct TestAsset {
    name: String,
    value: i32,
}

impl Asset for TestAsset {
    const TYPE_NAME: &'static str = "test_asset";
}

#[derive(Default)]
struct TestAssetSerializer;

#[derive(Debug, thiserror::Error)]
enum TestAssetSerializerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Toml serialization error: {0}")]
    TomlSerError(#[from] toml::ser::Error),
    #[error("Toml deserialization error: {0}")]
    TomlDeError(#[from] toml::de::Error),
}

impl AssetSerializer for TestAssetSerializer {
    type Asset = TestAsset;

    type Error = TestAssetSerializerError;

    fn file_extension() -> &'static str {
        "toml"
    }

    fn read(&self, reader: &mut dyn std::io::Read) -> Result<Self::Asset, Self::Error> {
        Ok(toml::from_str(&read_to_string(reader)?)?)
    }

    fn write(
        &self,
        asset: &Self::Asset,
        writer: &mut dyn std::io::Write,
    ) -> Result<(), Self::Error> {
        let content = toml::to_string(asset)?;
        writer.write_all(content.as_bytes())?;
        Ok(())
    }
}

#[tokio::test]
async fn test() {
    let mut serializers = AssetSerializerRegistry::new();
    serializers.register::<TestAssetSerializer>();
    serializers.register::<TagSerializer>();

    let mut registry = AssetRegistry::new("assets", serializers.into())
        .await
        .unwrap();
    registry
        .add_bundle(AssetDirectory::new("assets/local_assets"))
        .await
        .unwrap();

    let (bundles, errors) = StandardAssetBundle::scan_bundles("assets").await;
    assert!(errors.is_empty());
    assert_eq!(bundles.len(), 1);
    for bundle in bundles {
        registry.add_bundle(bundle).await.unwrap();
    }

    for handle in registry.all_handles_of::<TestAsset>().await.unwrap() {
        let asset = handle.get().await.unwrap();
        println!("Test asset: {:?}", asset);

        handle
            .update(TestAsset {
                name: asset.name.clone(),
                value: asset.value * 2,
            })
            .await
            .unwrap();
    }

    for handle in registry
        .all_handles_of_filtered::<TestAsset>(AssetFilter::new().with_tag(TagId::new(
            Uuid::from_str("f6d3cfcd-d9d8-49e4-a63c-b216444834ba").unwrap(),
        )))
        .await
        .unwrap()
    {
        let asset = handle.get().await.unwrap();
        println!("Test asset with tag: {:?}", asset);
    }

    for handle in registry
        .all_handles_of_filtered::<TestAsset>(AssetFilter::new().with_bundle(BundleId::new(
            Uuid::from_str("63f361f6-afbd-4df5-8e8d-13848d1d2cc1").unwrap(),
        )))
        .await
        .unwrap()
    {
        let asset = handle.get().await.unwrap();
        println!("Test asset in bundle: {:?}", asset);
    }
}
