use std::{
    fmt::Debug,
    hash::{BuildHasher, RandomState},
    io::read_to_string,
    path::Path,
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
    env_logger::init();

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

    {
        let expected = &[
            TestAsset {
                name: "Test Asset".to_string(),
                value: 42,
            },
            TestAsset {
                name: "Test Asset 2".to_string(),
                value: 84,
            },
        ];
        for (i, handle) in registry
            .all_handles_of::<TestAsset>()
            .await
            .unwrap()
            .iter()
            .enumerate()
        {
            let asset = handle.get().await.unwrap();
            assert_eq!(asset.as_ref(), &expected[i]);

            handle
                .update(TestAsset {
                    name: asset.name.clone(),
                    value: asset.value * 2,
                })
                .await
                .unwrap();
        }
    }

    {
        let expected = &[
            TestAsset {
                name: "Test Asset".to_string(),
                value: 42 * 2,
            },
            TestAsset {
                name: "Test Asset 2".to_string(),
                value: 84 * 2,
            },
        ];
        for (i, handle) in registry
            .all_handles_of::<TestAsset>()
            .await
            .unwrap()
            .iter()
            .enumerate()
        {
            let asset = handle.get().await.unwrap();
            assert_eq!(asset.as_ref(), &expected[i]);

            handle.write().await.unwrap();
        }
    }

    for handle in registry
        .all_handles_of_filtered::<TestAsset>(AssetFilter::new().with_tag(TagId::new(
            Uuid::from_str("ef34773e-d825-974f-3268-7d7fed983dc8").unwrap(),
        )))
        .await
        .unwrap()
    {
        let asset = handle.get().await.unwrap();
        assert_eq!(
            asset.as_ref(),
            &TestAsset {
                name: "Test Asset".to_string(),
                value: 84,
            }
        );
    }

    for handle in registry
        .all_handles_of_filtered::<TestAsset>(AssetFilter::new().with_bundle(BundleId::new(
            Uuid::from_str("63f361f6-afbd-4df5-8e8d-13848d1d2cc1").unwrap(),
        )))
        .await
        .unwrap()
    {
        let asset = handle.get().await.unwrap();
        assert_eq!(
            asset.as_ref(),
            &TestAsset {
                name: "Test Asset 2".to_string(),
                value: 168,
            }
        );
    }

    std::fs::remove_dir_all("assets/ef34773e-d825-974f-3268-7d7fed983dc8.modified").unwrap();
    std::fs::remove_dir_all("assets/63f361f6-afbd-4df5-8e8d-13848d1d2cc1.modified").unwrap();
    sqlx::query(
        r#"
DROP TABLE IF EXISTS bundles;
DROP TABLE IF EXISTS assets;
DROP TABLE IF EXISTS asset_revisions;
DROP TABLE IF EXISTS asset_tags;
DROP TABLE IF EXISTS tags;
        "#,
    )
    .execute(registry.index_db().pool())
    .await
    .unwrap();
}
