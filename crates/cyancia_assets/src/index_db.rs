use std::{fs::File, marker::PhantomData, path::Path};

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::{
    asset::{Asset, AssetMetadata, UntypedAssetId},
    bundle::{AssetBundleMetadata, BundleId},
    error::AssetResult,
    tag::{Tag, TagId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemStatus {
    UpToDate,
    Outdated,
}

pub struct AssetFilter<T: Asset> {
    tag: Option<TagId>,
    bundle: Option<BundleId>,
    _marker: PhantomData<T>,
}

impl<T: Asset> Default for AssetFilter<T> {
    fn default() -> Self {
        Self {
            tag: Default::default(),
            bundle: Default::default(),
            _marker: PhantomData,
        }
    }
}

impl<T: Asset> AssetFilter<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_tag(mut self, tag: TagId) -> Self {
        self.tag = Some(tag);
        self
    }

    pub fn with_bundle(mut self, bundle: BundleId) -> Self {
        self.bundle = Some(bundle);
        self
    }

    pub fn into_untyped(self) -> UntypedAssetFilter {
        UntypedAssetFilter {
            ty: Some(T::TYPE_NAME.to_string()),
            tag: self.tag,
            bundle: self.bundle,
        }
    }
}

#[derive(Default)]
pub struct UntypedAssetFilter {
    pub ty: Option<String>,
    pub tag: Option<TagId>,
    pub bundle: Option<BundleId>,
}

pub struct AssetIndexDb {
    conn: Mutex<Connection>,
}

impl AssetIndexDb {
    pub fn connect(path: impl AsRef<Path>) -> AssetResult<Self> {
        let path = path.as_ref();
        if !path.exists() {
            File::create(path)?;
        }

        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Self { conn: conn.into() };
        db.initialize_tables()?;
        db.revert_all_assets()?;
        Ok(db)
    }

    pub fn open_in_memory() -> AssetResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Self { conn: conn.into() };
        db.initialize_tables()?;
        db.revert_all_assets()?;
        Ok(db)
    }

    fn initialize_tables(&self) -> AssetResult<()> {
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
CREATE TABLE IF NOT EXISTS bundles (
    bundle_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS assets (
    asset_id TEXT PRIMARY KEY,
    ty TEXT NOT NULL,
    bundle_id TEXT NOT NULL,

    FOREIGN KEY (bundle_id) REFERENCES bundles(bundle_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS asset_revisions (
    asset_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    relative_path TEXT,
    last_modified TEXT NOT NULL,
    in_memory INTEGER NOT NULL,

    PRIMARY KEY (asset_id, revision),
    FOREIGN KEY (asset_id) REFERENCES assets(asset_id) ON DELETE CASCADE,

    CHECK (
        (in_memory = 1 AND relative_path IS NULL)
        OR
        (in_memory = 0 AND relative_path IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS tags (
    tag_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS asset_tags (
    asset_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (asset_id, tag_id),
    FOREIGN KEY (asset_id) REFERENCES assets(asset_id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(tag_id) ON DELETE CASCADE
);
            "#,
        )?;

        Ok(())
    }

    pub fn upsert_bundle(&self, bundle: &AssetBundleMetadata) -> AssetResult<ItemStatus> {
        let conn = self.conn.lock();
        let result = conn.query_row(
            r#"
INSERT INTO bundles (bundle_id, name, last_modified)
VALUES (?1, ?2, ?3)
ON CONFLICT(bundle_id) DO UPDATE SET
    name = excluded.name,
    last_modified = excluded.last_modified
WHERE bundles.last_modified IS NOT excluded.last_modified
RETURNING 0;
            "#,
            params![bundle.bundle_id, bundle.name, bundle.last_modified,],
            |_| Ok(()),
        );

        match result {
            Ok(_) => Ok(ItemStatus::Outdated),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ItemStatus::UpToDate),
            Err(e) => Err(e.into()),
        }
    }

    pub fn replace_assets(&self, bundle: &BundleId, assets: &[AssetMetadata]) -> AssetResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        tx.execute("DELETE FROM assets WHERE bundle_id = ?1", params![bundle])?;

        for asset in assets {
            tx.execute(
                r#"
INSERT INTO assets (asset_id, ty, bundle_id)
VALUES (?1, ?2, ?3)
ON CONFLICT DO NOTHING;
                "#,
                params![asset.asset_id, asset.ty, asset.bundle_id,],
            )?;
            tx.execute(
                r#"
INSERT INTO asset_revisions (asset_id, revision, relative_path, last_modified, in_memory)
VALUES (?1, ?2, ?3, ?4, ?5);
                "#,
                params![
                    asset.asset_id,
                    asset.revision,
                    asset.relative_path,
                    asset.last_modified,
                    asset.in_memory as i64,
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn upsert_tag(&self, tag: &Tag, last_modified: DateTime<Utc>) -> AssetResult<()> {
        let mut conn = self.conn.lock();

        let needs_update = {
            let result = conn.query_row(
                r#"
INSERT INTO tags (tag_id, name, last_modified)
VALUES (?1, ?2, ?3)
ON CONFLICT(tag_id) DO UPDATE SET
    name = excluded.name,
    last_modified = excluded.last_modified
WHERE tags.last_modified IS NOT excluded.last_modified
RETURNING 0;
                "#,
                params![tag.id(), tag.name(), last_modified,],
                |_| Ok(()),
            );
            match result {
                Ok(_) => true,
                Err(rusqlite::Error::QueryReturnedNoRows) => false,
                Err(e) => return Err(e.into()),
            }
        };

        if !needs_update {
            return Ok(());
        }

        let tx = conn.transaction()?;

        tx.execute(
            "DELETE FROM asset_tags WHERE tag_id = ?1",
            params![tag.id()],
        )?;

        for asset_id in tag.assets() {
            println!("Associating asset {} with tag {}", asset_id, tag.name());
            tx.execute(
                "INSERT INTO asset_tags (asset_id, tag_id) VALUES (?1, ?2)",
                params![asset_id, tag.id()],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn add_asset(&self, asset: &AssetMetadata) -> AssetResult<UntypedAssetId> {
        let asset_id = asset.asset_id;
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        tx.execute(
            r#"
INSERT INTO assets (asset_id, ty, bundle_id)
VALUES (?1, ?2, ?3)
ON CONFLICT DO NOTHING;
            "#,
            params![asset.asset_id, asset.ty, asset.bundle_id,],
        )?;

        tx.execute(
            r#"
INSERT INTO asset_revisions (asset_id, revision, relative_path, last_modified, in_memory)
VALUES (?1, ?2, ?3, ?4, ?5);
            "#,
            params![
                asset.asset_id,
                asset.revision,
                asset.relative_path,
                asset.last_modified,
                asset.in_memory as i64,
            ],
        )?;

        tx.commit()?;
        Ok(asset_id)
    }

    pub fn get_asset(&self, id: &UntypedAssetId) -> AssetResult<AssetMetadata> {
        let conn = self.conn.lock();
        let asset = conn.query_row(
            r#"
SELECT
    r.asset_id,
    a.ty,
    a.bundle_id,
    r.relative_path,
    r.revision,
    r.last_modified,
    r.in_memory
FROM asset_revisions r
JOIN assets a USING (asset_id)
WHERE r.asset_id = ?1
ORDER BY r.revision DESC
LIMIT 1;
            "#,
            params![id],
            |row| {
                Ok(AssetMetadata {
                    asset_id: row.get(0)?,
                    ty: row.get(1)?,
                    bundle_id: row.get(2)?,
                    relative_path: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    revision: row.get(4)?,
                    last_modified: row.get(5)?,
                    in_memory: row.get::<_, i64>(6)? == 1,
                })
            },
        )?;

        Ok(asset)
    }

    pub fn get_assets(&self, filter: UntypedAssetFilter) -> AssetResult<Vec<AssetMetadata>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
WITH latest AS (
    SELECT
        r.asset_id,
        r.relative_path,
        r.revision,
        r.last_modified,
        r.in_memory,
        ROW_NUMBER() OVER (PARTITION BY r.asset_id ORDER BY r.revision DESC) AS ord
    FROM asset_revisions r
)
SELECT
    l.asset_id,
    a.ty,
    a.bundle_id,
    l.relative_path,
    l.revision,
    l.last_modified,
    l.in_memory
FROM latest l
JOIN assets a ON a.asset_id = l.asset_id
WHERE l.ord = 1
    AND (?1 IS NULL OR a.ty = ?1)
    AND (?2 IS NULL OR a.asset_id IN (SELECT asset_id FROM asset_tags WHERE tag_id = ?2))
    AND (?3 IS NULL OR a.bundle_id = ?3)
ORDER BY l.relative_path ASC;
            "#,
        )?;

        let rows = stmt.query_map(
            params![filter.ty, filter.tag.as_ref(), filter.bundle.as_ref(),],
            |row| {
                Ok(AssetMetadata {
                    asset_id: row.get(0)?,
                    ty: row.get(1)?,
                    bundle_id: row.get(2)?,
                    relative_path: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    revision: row.get(4)?,
                    last_modified: row.get(5)?,
                    in_memory: row.get::<_, i64>(6)? == 1,
                })
            },
        )?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn update_asset(&self, id: &UntypedAssetId) -> AssetResult<u32> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        let (revision, in_memory) = tx.query_row(
            r#"
SELECT revision, in_memory
FROM asset_revisions
WHERE asset_id = ?1
ORDER BY revision DESC
LIMIT 1;
            "#,
            params![id],
            |row| Ok((row.get::<_, u32>(0)?, row.get::<_, i64>(1)? == 1)),
        )?;

        if in_memory {
            tx.commit()?;
            return Ok(revision);
        }

        let revision = tx.query_one(
            r#"
INSERT INTO asset_revisions (
    asset_id,
    relative_path,
    revision,
    last_modified,
    in_memory
)
VALUES (?1, NULL, ?2, ?3, 1)
RETURNING revision;
            "#,
            params![id, revision + 1, Utc::now()],
            |row| row.get::<_, u32>(0),
        )?;

        tx.commit()?;
        Ok(revision)
    }

    pub fn write_asset(
        &self,
        id: &UntypedAssetId,
        new_path: &str,
        last_modified: DateTime<Utc>,
    ) -> AssetResult<u32> {
        let conn = self.conn.lock();
        let revision = conn.query_row(
            r#"
WITH latest AS (
    SELECT revision, in_memory
    FROM asset_revisions
    WHERE asset_id = ?1
    ORDER BY revision DESC
    LIMIT 1
)
UPDATE asset_revisions
SET in_memory = 0, relative_path = ?2, last_modified = ?3
WHERE asset_id = ?1
  AND revision = (SELECT revision FROM latest)
  AND (SELECT in_memory FROM latest) = 1
RETURNING revision;
            "#,
            params![id, new_path, last_modified],
            |row| row.get::<_, u32>(0),
        )?;

        Ok(revision)
    }

    pub fn revert_asset(&self, id: &Uuid) -> AssetResult<()> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM asset_revisions WHERE in_memory = 1 AND asset_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn revert_all_assets(&self) -> AssetResult<()> {
        let conn = self.conn.lock();
        conn.execute("DELETE FROM asset_revisions WHERE in_memory = 1", [])?;
        Ok(())
    }

    pub fn get_bundle(&self, id: &BundleId) -> AssetResult<AssetBundleMetadata> {
        let conn = self.conn.lock();
        let bundle = conn.query_row(
            r#"
SELECT bundle_id, name, last_modified
FROM bundles
WHERE bundle_id = ?1;
            "#,
            params![id],
            |row| {
                Ok(AssetBundleMetadata {
                    bundle_id: row.get(0)?,
                    name: row.get(1)?,
                    last_modified: row.get(2)?,
                })
            },
        )?;

        Ok(bundle)
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    struct TestAsset;

    impl Asset for TestAsset {
        const TYPE_NAME: &'static str = "test_asset";
    }

    struct OtherAsset;

    impl Asset for OtherAsset {
        const TYPE_NAME: &'static str = "other_asset";
    }

    #[test]
    fn bundle_upsert_and_get() -> AssetResult<()> {
        let db = AssetIndexDb::open_in_memory()?;
        let bundle_id = BundleId::new(Uuid::from_u128(1));
        let mut bundle = AssetBundleMetadata {
            bundle_id,
            name: "Original".to_string(),
            last_modified: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        };

        assert_eq!(db.upsert_bundle(&bundle)?, ItemStatus::Outdated);

        let stored = db.get_bundle(&bundle_id)?;
        assert_eq!(stored.bundle_id, bundle_id);
        assert_eq!(stored.name, "Original");
        assert_eq!(stored.last_modified, bundle.last_modified);

        bundle.name = "Ignored".to_string();
        assert_eq!(db.upsert_bundle(&bundle)?, ItemStatus::UpToDate);
        assert_eq!(db.get_bundle(&bundle_id)?.name, "Original");

        bundle.name = "Updated".to_string();
        bundle.last_modified = Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap();
        assert_eq!(db.upsert_bundle(&bundle)?, ItemStatus::Outdated);

        let stored = db.get_bundle(&bundle_id)?;
        assert_eq!(stored.name, "Updated");
        assert_eq!(stored.last_modified, bundle.last_modified);

        Ok(())
    }

    #[test]
    fn add_get_and_filter_assets() -> AssetResult<()> {
        let db = AssetIndexDb::open_in_memory()?;
        let first_bundle_id = BundleId::new(Uuid::from_u128(10));
        let second_bundle_id = BundleId::new(Uuid::from_u128(11));
        let last_modified = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();

        for bundle in [
            AssetBundleMetadata {
                bundle_id: first_bundle_id,
                name: "First".to_string(),
                last_modified,
            },
            AssetBundleMetadata {
                bundle_id: second_bundle_id,
                name: "Second".to_string(),
                last_modified,
            },
        ] {
            db.upsert_bundle(&bundle)?;
        }

        let first_id = UntypedAssetId::new(Uuid::from_u128(20));
        let second_id = UntypedAssetId::new(Uuid::from_u128(21));
        let third_id = UntypedAssetId::new(Uuid::from_u128(22));
        let first = AssetMetadata {
            asset_id: first_id,
            ty: TestAsset::TYPE_NAME.to_string(),
            bundle_id: first_bundle_id,
            relative_path: "zeta.asset".to_string(),
            revision: 0,
            last_modified,
            in_memory: false,
        };
        let second = AssetMetadata {
            asset_id: second_id,
            ty: OtherAsset::TYPE_NAME.to_string(),
            bundle_id: first_bundle_id,
            relative_path: "alpha.asset".to_string(),
            revision: 0,
            last_modified,
            in_memory: false,
        };
        let third = AssetMetadata {
            asset_id: third_id,
            ty: TestAsset::TYPE_NAME.to_string(),
            bundle_id: second_bundle_id,
            relative_path: "middle.asset".to_string(),
            revision: 4,
            last_modified,
            in_memory: false,
        };

        assert_eq!(db.add_asset(&first)?, first_id);
        assert_eq!(db.add_asset(&second)?, second_id);
        assert_eq!(db.add_asset(&third)?, third_id);

        let stored = db.get_asset(&third_id)?;
        assert_eq!(stored.asset_id, third_id);
        assert_eq!(stored.ty, TestAsset::TYPE_NAME);
        assert_eq!(stored.bundle_id, second_bundle_id);
        assert_eq!(stored.relative_path, "middle.asset");
        assert_eq!(stored.revision, 4);
        assert_eq!(stored.last_modified, last_modified);
        assert!(!stored.in_memory);

        let all = db.get_assets(UntypedAssetFilter::default())?;
        assert_eq!(
            all.iter().map(|asset| asset.asset_id).collect::<Vec<_>>(),
            vec![second_id, third_id, first_id]
        );

        let typed = db.get_assets(AssetFilter::<TestAsset>::new().into_untyped())?;
        assert_eq!(
            typed.iter().map(|asset| asset.asset_id).collect::<Vec<_>>(),
            vec![third_id, first_id]
        );

        let in_first_bundle = db.get_assets(
            AssetFilter::<TestAsset>::new()
                .with_bundle(first_bundle_id)
                .into_untyped(),
        )?;
        assert_eq!(in_first_bundle.len(), 1);
        assert_eq!(in_first_bundle[0].asset_id, first_id);

        let untyped_in_first_bundle = db.get_assets(UntypedAssetFilter {
            bundle: Some(first_bundle_id),
            ..Default::default()
        })?;
        assert_eq!(
            untyped_in_first_bundle
                .iter()
                .map(|asset| asset.asset_id)
                .collect::<Vec<_>>(),
            vec![second_id, first_id]
        );

        Ok(())
    }

    #[test]
    fn replace_assets_replaces_one_bundle_and_keeps_latest_revisions() -> AssetResult<()> {
        let db = AssetIndexDb::open_in_memory()?;
        let replaced_bundle_id = BundleId::new(Uuid::from_u128(30));
        let retained_bundle_id = BundleId::new(Uuid::from_u128(31));
        let last_modified = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();

        for bundle in [
            AssetBundleMetadata {
                bundle_id: replaced_bundle_id,
                name: "Replaced".to_string(),
                last_modified,
            },
            AssetBundleMetadata {
                bundle_id: retained_bundle_id,
                name: "Retained".to_string(),
                last_modified,
            },
        ] {
            db.upsert_bundle(&bundle)?;
        }

        let removed_id = UntypedAssetId::new(Uuid::from_u128(40));
        let retained_id = UntypedAssetId::new(Uuid::from_u128(41));
        db.add_asset(&AssetMetadata {
            asset_id: removed_id,
            ty: TestAsset::TYPE_NAME.to_string(),
            bundle_id: replaced_bundle_id,
            relative_path: "removed.asset".to_string(),
            revision: 0,
            last_modified,
            in_memory: false,
        })?;
        db.add_asset(&AssetMetadata {
            asset_id: retained_id,
            ty: TestAsset::TYPE_NAME.to_string(),
            bundle_id: retained_bundle_id,
            relative_path: "retained.asset".to_string(),
            revision: 0,
            last_modified,
            in_memory: false,
        })?;

        let replacement_id = UntypedAssetId::new(Uuid::from_u128(42));
        db.replace_assets(
            &replaced_bundle_id,
            &[
                AssetMetadata {
                    asset_id: replacement_id,
                    ty: TestAsset::TYPE_NAME.to_string(),
                    bundle_id: replaced_bundle_id,
                    relative_path: "replacement.asset".to_string(),
                    revision: 0,
                    last_modified,
                    in_memory: false,
                },
                AssetMetadata {
                    asset_id: replacement_id,
                    ty: TestAsset::TYPE_NAME.to_string(),
                    bundle_id: replaced_bundle_id,
                    relative_path: "replacement.rev3.asset".to_string(),
                    revision: 3,
                    last_modified: Utc.with_ymd_and_hms(2026, 3, 2, 0, 0, 0).unwrap(),
                    in_memory: false,
                },
            ],
        )?;

        assert!(db.get_asset(&removed_id).is_err());
        assert_eq!(db.get_asset(&retained_id)?.asset_id, retained_id);

        let replacement = db.get_asset(&replacement_id)?;
        assert_eq!(replacement.revision, 3);
        assert_eq!(replacement.relative_path, "replacement.rev3.asset");

        let replaced_bundle_assets = db.get_assets(UntypedAssetFilter {
            bundle: Some(replaced_bundle_id),
            ..Default::default()
        })?;
        assert_eq!(replaced_bundle_assets.len(), 1);
        assert_eq!(replaced_bundle_assets[0].asset_id, replacement_id);

        db.replace_assets(&replaced_bundle_id, &[])?;
        assert!(
            db.get_assets(UntypedAssetFilter {
                bundle: Some(replaced_bundle_id),
                ..Default::default()
            })?
            .is_empty()
        );
        assert_eq!(db.get_asset(&retained_id)?.asset_id, retained_id);

        Ok(())
    }

    #[test]
    fn tags_are_upserted_and_filter_assets() -> AssetResult<()> {
        let db = AssetIndexDb::open_in_memory()?;
        let first_bundle_id = BundleId::new(Uuid::from_u128(50));
        let second_bundle_id = BundleId::new(Uuid::from_u128(51));
        let last_modified = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();

        for bundle in [
            AssetBundleMetadata {
                bundle_id: first_bundle_id,
                name: "First".to_string(),
                last_modified,
            },
            AssetBundleMetadata {
                bundle_id: second_bundle_id,
                name: "Second".to_string(),
                last_modified,
            },
        ] {
            db.upsert_bundle(&bundle)?;
        }

        let first_id = UntypedAssetId::new(Uuid::from_u128(60));
        let second_id = UntypedAssetId::new(Uuid::from_u128(61));
        let third_id = UntypedAssetId::new(Uuid::from_u128(62));
        for asset in [
            AssetMetadata {
                asset_id: first_id,
                ty: TestAsset::TYPE_NAME.to_string(),
                bundle_id: first_bundle_id,
                relative_path: "a.asset".to_string(),
                revision: 0,
                last_modified,
                in_memory: false,
            },
            AssetMetadata {
                asset_id: second_id,
                ty: OtherAsset::TYPE_NAME.to_string(),
                bundle_id: first_bundle_id,
                relative_path: "b.asset".to_string(),
                revision: 0,
                last_modified,
                in_memory: false,
            },
            AssetMetadata {
                asset_id: third_id,
                ty: TestAsset::TYPE_NAME.to_string(),
                bundle_id: second_bundle_id,
                relative_path: "c.asset".to_string(),
                revision: 0,
                last_modified,
                in_memory: false,
            },
        ] {
            db.add_asset(&asset)?;
        }

        let mut tag = Tag::new("Selected".to_string());
        tag.add_asset(first_id);
        tag.add_asset(second_id);
        db.upsert_tag(&tag, last_modified)?;

        let tagged = db.get_assets(UntypedAssetFilter {
            tag: Some(tag.id().clone()),
            ..Default::default()
        })?;
        assert_eq!(
            tagged
                .iter()
                .map(|asset| asset.asset_id)
                .collect::<Vec<_>>(),
            vec![first_id, second_id]
        );

        let typed = db.get_assets(
            AssetFilter::<TestAsset>::new()
                .with_tag(tag.id().clone())
                .into_untyped(),
        )?;
        assert_eq!(typed.len(), 1);
        assert_eq!(typed[0].asset_id, first_id);

        tag.remove_asset(&first_id);
        tag.add_asset(third_id);
        tag.set_name("Changed".to_string());
        db.upsert_tag(&tag, last_modified)?;

        let unchanged = db.get_assets(UntypedAssetFilter {
            tag: Some(tag.id().clone()),
            ..Default::default()
        })?;
        assert_eq!(
            unchanged
                .iter()
                .map(|asset| asset.asset_id)
                .collect::<Vec<_>>(),
            vec![first_id, second_id]
        );

        db.upsert_tag(&tag, Utc.with_ymd_and_hms(2026, 4, 2, 0, 0, 0).unwrap())?;

        let changed = db.get_assets(UntypedAssetFilter {
            tag: Some(tag.id().clone()),
            ..Default::default()
        })?;
        assert_eq!(
            changed
                .iter()
                .map(|asset| asset.asset_id)
                .collect::<Vec<_>>(),
            vec![second_id, third_id]
        );

        let typed_in_second_bundle = db.get_assets(
            AssetFilter::<TestAsset>::new()
                .with_tag(tag.id().clone())
                .with_bundle(second_bundle_id)
                .into_untyped(),
        )?;
        assert_eq!(typed_in_second_bundle.len(), 1);
        assert_eq!(typed_in_second_bundle[0].asset_id, third_id);

        Ok(())
    }

    #[test]
    fn asset_revision_lifecycle() -> AssetResult<()> {
        let db = AssetIndexDb::open_in_memory()?;
        let bundle_id = BundleId::new(Uuid::from_u128(70));
        let initial_last_modified = Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
        db.upsert_bundle(&AssetBundleMetadata {
            bundle_id,
            name: "Revisions".to_string(),
            last_modified: initial_last_modified,
        })?;

        let first_id = UntypedAssetId::new(Uuid::from_u128(80));
        let second_id = UntypedAssetId::new(Uuid::from_u128(81));
        db.add_asset(&AssetMetadata {
            asset_id: first_id,
            ty: TestAsset::TYPE_NAME.to_string(),
            bundle_id,
            relative_path: "first.asset".to_string(),
            revision: 7,
            last_modified: initial_last_modified,
            in_memory: false,
        })?;
        db.add_asset(&AssetMetadata {
            asset_id: second_id,
            ty: TestAsset::TYPE_NAME.to_string(),
            bundle_id,
            relative_path: "second.asset".to_string(),
            revision: 0,
            last_modified: initial_last_modified,
            in_memory: false,
        })?;

        assert_eq!(db.update_asset(&first_id)?, 8);
        assert_eq!(db.update_asset(&first_id)?, 8);
        let first = db.get_asset(&first_id)?;
        assert_eq!(first.revision, 8);
        assert!(first.in_memory);
        assert_eq!(first.relative_path, "");

        let revision_count = db.conn.lock().query_row(
            "SELECT COUNT(*) FROM asset_revisions WHERE asset_id = ?1",
            params![first_id],
            |row| row.get::<_, u32>(0),
        )?;
        assert_eq!(revision_count, 2);

        let written_at = Utc.with_ymd_and_hms(2026, 5, 2, 0, 0, 0).unwrap();
        assert_eq!(
            db.write_asset(&first_id, "first.rev8.asset", written_at)?,
            8
        );
        let written = db.get_asset(&first_id)?;
        assert_eq!(written.revision, 8);
        assert_eq!(written.relative_path, "first.rev8.asset");
        assert_eq!(written.last_modified, written_at);
        assert!(!written.in_memory);
        assert_eq!(
            db.conn.lock().query_row(
                "SELECT COUNT(*) FROM asset_revisions WHERE asset_id = ?1",
                params![first_id],
                |row| row.get::<_, u32>(0),
            )?,
            2
        );

        assert_eq!(db.update_asset(&first_id)?, 9);
        db.revert_asset(&first_id)?;
        let reverted = db.get_asset(&first_id)?;
        assert_eq!(reverted.revision, 8);
        assert_eq!(reverted.relative_path, "first.rev8.asset");
        assert!(!reverted.in_memory);

        assert_eq!(db.update_asset(&first_id)?, 9);
        assert_eq!(db.update_asset(&second_id)?, 1);
        db.revert_all_assets()?;

        let first = db.get_asset(&first_id)?;
        let second = db.get_asset(&second_id)?;
        assert_eq!(first.revision, 8);
        assert!(!first.in_memory);
        assert_eq!(second.revision, 0);
        assert!(!second.in_memory);

        assert_eq!(db.update_asset(&second_id)?, 1);
        db.conn.lock().execute(
            r#"
INSERT INTO asset_revisions (asset_id, revision, relative_path, last_modified, in_memory)
VALUES (?1, 2, ?2, ?3, 0);
            "#,
            params![second_id, "second.rev2.asset", written_at],
        )?;
        assert!(
            db.write_asset(&second_id, "second.rev1.asset", written_at)
                .is_err()
        );
        let latest = db.get_asset(&second_id)?;
        assert_eq!(latest.revision, 2);
        assert_eq!(latest.relative_path, "second.rev2.asset");
        assert!(!latest.in_memory);
        let older_in_memory = db.conn.lock().query_row(
            "SELECT in_memory FROM asset_revisions WHERE asset_id = ?1 AND revision = 1",
            params![second_id],
            |row| row.get::<_, i64>(0),
        )?;
        assert_eq!(older_in_memory, 1);

        Ok(())
    }
}
