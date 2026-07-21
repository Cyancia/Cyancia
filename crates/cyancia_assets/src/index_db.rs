use std::{fs::File, marker::PhantomData, path::Path};

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::{
    asset::{Asset, AssetMetadata, UntypedAssetId},
    bundle::{AssetBundleMetadata, BundleId},
    error::{AssetError, AssetResult},
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
    is_deleted: bool,
    _marker: PhantomData<T>,
}

impl<T: Asset> Default for AssetFilter<T> {
    fn default() -> Self {
        Self {
            tag: Default::default(),
            bundle: Default::default(),
            is_deleted: false,
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

    pub fn with_deleted(mut self, is_deleted: bool) -> Self {
        self.is_deleted = is_deleted;
        self
    }

    pub fn into_untyped(self) -> UntypedAssetFilter {
        UntypedAssetFilter {
            ty: Some(T::TYPE_NAME.to_string()),
            tag: self.tag,
            bundle: self.bundle,
            is_deleted: self.is_deleted,
        }
    }
}

#[derive(Default)]
pub struct UntypedAssetFilter {
    pub ty: Option<String>,
    pub tag: Option<TagId>,
    pub bundle: Option<BundleId>,
    pub is_deleted: bool,
}

#[derive(Default)]
pub struct TagFilter {
    pub asset_ty: Option<Option<String>>,
    pub is_deleted: bool,
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
    is_deleted INTEGER NOT NULL,

    FOREIGN KEY (bundle_id) REFERENCES bundles(bundle_id) ON DELETE CASCADE,
    CHECK (is_deleted IN (0, 1))
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
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    asset_ty TEXT,
    last_modified TEXT NOT NULL,
    is_deleted INTEGER NOT NULL,

    CHECK (is_deleted IN (0, 1))
);

CREATE TABLE IF NOT EXISTS asset_tags (
    asset_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (asset_id, tag_id),
    FOREIGN KEY (asset_id) REFERENCES assets(asset_id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
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

    // This is only intended use by asset store to update a out-dated bundle
    pub fn replace_assets(&self, bundle: &BundleId, assets: &[AssetMetadata]) -> AssetResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        tx.execute(
            "UPDATE assets SET is_deleted = 1 WHERE bundle_id = ?1",
            params![bundle],
        )?;

        for asset in assets {
            tx.execute(
                r#"
DELETE FROM asset_revisions
WHERE asset_id = ?1
  AND EXISTS (
      SELECT 1 FROM assets
      WHERE asset_id = ?1 AND is_deleted = 1
  );
                "#,
                params![asset.asset_id],
            )?;
            tx.execute(
                r#"
INSERT INTO assets (asset_id, ty, bundle_id, is_deleted)
VALUES (?1, ?2, ?3, 0)
ON CONFLICT(asset_id) DO UPDATE SET
    ty = excluded.ty,
    bundle_id = excluded.bundle_id,
    is_deleted = 0;
                "#,
                params![asset.asset_id, asset.ty, asset.bundle_id,],
            )?;
            tx.execute(
                r#"
INSERT INTO asset_revisions (asset_id, revision, relative_path, last_modified, in_memory)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(asset_id, revision) DO UPDATE SET
    relative_path = excluded.relative_path,
    last_modified = excluded.last_modified,
    in_memory = excluded.in_memory;
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
        let tx = conn.transaction()?;

        let stored_tag = tx
            .query_row(
                "SELECT asset_ty, last_modified, is_deleted FROM tags WHERE id = ?1",
                params![tag.id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, DateTime<Utc>>(1)?,
                        row.get::<_, bool>(2)?,
                    ))
                },
            )
            .optional()?;

        match stored_tag {
            None => {
                tx.execute(
                    r#"
INSERT INTO tags (id, name, asset_ty, last_modified, is_deleted)
VALUES (?1, ?2, ?3, ?4, 0);
                    "#,
                    params![tag.id, tag.name, tag.asset_ty, last_modified],
                )?;
            }
            Some((stored_asset_ty, stored_last_modified, is_deleted)) => {
                if is_deleted {
                    tx.commit()?;
                    return Ok(());
                }

                if stored_asset_ty != tag.asset_ty {
                    return Err(AssetError::TagAssetTypeChanged {
                        tag_id: tag.id.clone(),
                        current_asset_ty: stored_asset_ty,
                        new_asset_ty: tag.asset_ty.clone(),
                    }
                    .into());
                }

                if stored_last_modified == last_modified {
                    tx.commit()?;
                    return Ok(());
                }

                tx.execute(
                    r#"
UPDATE tags
SET name = ?2, last_modified = ?3
WHERE id = ?1;
                    "#,
                    params![tag.id, tag.name, last_modified],
                )?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    pub fn get_tag(&self, tag_id: TagId) -> AssetResult<Tag> {
        let conn = self.conn.lock();
        let tag = conn.query_row(
            "SELECT id, name, asset_ty FROM tags WHERE id = ?1 AND is_deleted = 0",
            params![tag_id],
            |row| {
                Ok(Tag {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    asset_ty: row.get(2)?,
                })
            },
        )?;
        Ok(tag)
    }

    pub fn get_tags(&self, filter: TagFilter) -> AssetResult<Vec<Tag>> {
        let (filter_kind, asset_ty) = match filter.asset_ty {
            None => (0, None),
            Some(None) => (1, None),
            Some(Some(asset_ty)) => (2, Some(asset_ty)),
        };

        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            r#"
SELECT id, name, asset_ty
FROM tags
WHERE is_deleted = ?3
    AND (
        ?1 = 0
        OR (?1 = 1 AND asset_ty IS NULL)
        OR (?1 = 2 AND asset_ty = ?2)
    )
ORDER BY name ASC, id ASC;
            "#,
        )?;
        let rows = stmt.query_map(params![filter_kind, asset_ty, filter.is_deleted], |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
                asset_ty: row.get(2)?,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_tag(&self, tag_id: &TagId) -> AssetResult<()> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "UPDATE tags SET is_deleted = 1 WHERE id = ?1 AND is_deleted = 0",
            params![tag_id],
        )?;
        if deleted == 0 {
            return Err(AssetError::TagNotFound(tag_id.clone()).into());
        }

        Ok(())
    }

    pub fn add_asset(&self, asset: &AssetMetadata) -> AssetResult<UntypedAssetId> {
        let asset_id = asset.asset_id;
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        tx.execute(
            r#"
DELETE FROM asset_revisions
WHERE asset_id = ?1
  AND EXISTS (
      SELECT 1 FROM assets
      WHERE asset_id = ?1 AND is_deleted = 1
  );
            "#,
            params![asset.asset_id],
        )?;

        tx.execute(
            r#"
INSERT INTO assets (asset_id, ty, bundle_id, is_deleted)
VALUES (?1, ?2, ?3, 0)
ON CONFLICT(asset_id) DO UPDATE SET
    ty = excluded.ty,
    bundle_id = excluded.bundle_id,
    is_deleted = 0;
            "#,
            params![asset.asset_id, asset.ty, asset.bundle_id,],
        )?;

        tx.execute(
            r#"
INSERT INTO asset_revisions (asset_id, revision, relative_path, last_modified, in_memory)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(asset_id, revision) DO UPDATE SET
    relative_path = excluded.relative_path,
    last_modified = excluded.last_modified,
    in_memory = excluded.in_memory;
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
    AND a.is_deleted = 0
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
    AND a.is_deleted = ?4
    AND (?1 IS NULL OR a.ty = ?1)
    AND (
        ?2 IS NULL
        OR EXISTS (
            SELECT 1
            FROM asset_tags at
            JOIN tags t ON t.id = at.tag_id
            WHERE at.asset_id = a.asset_id
                AND at.tag_id = ?2
                AND (t.is_deleted = 0 OR ?4 = 1)
        )
    )
    AND (?3 IS NULL OR a.bundle_id = ?3)
ORDER BY l.relative_path ASC;
            "#,
        )?;

        let rows = stmt.query_map(
            params![
                filter.ty,
                filter.tag.as_ref(),
                filter.bundle.as_ref(),
                filter.is_deleted,
            ],
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
SELECT r.revision, r.in_memory
FROM asset_revisions r
JOIN assets a USING (asset_id)
WHERE r.asset_id = ?1
    AND a.is_deleted = 0
ORDER BY r.revision DESC
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
    SELECT r.revision, r.in_memory
    FROM asset_revisions r
    JOIN assets a USING (asset_id)
    WHERE r.asset_id = ?1
      AND a.is_deleted = 0
    ORDER BY r.revision DESC
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

    pub fn delete_asset(&self, asset_id: &UntypedAssetId) -> AssetResult<()> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "UPDATE assets SET is_deleted = 1 WHERE asset_id = ?1 AND is_deleted = 0",
            params![asset_id],
        )?;
        if deleted == 0 {
            return Err(AssetError::AssetNotFound(*asset_id).into());
        }

        Ok(())
    }

    pub fn add_tag_to_asset(&self, asset_id: &UntypedAssetId, tag_id: &TagId) -> AssetResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        let asset_ty = tx
            .query_row(
                "SELECT ty FROM assets WHERE asset_id = ?1 AND is_deleted = 0",
                params![asset_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| AssetError::AssetNotFound(*asset_id))?;
        let tag_asset_ty = tx
            .query_row(
                "SELECT asset_ty FROM tags WHERE id = ?1 AND is_deleted = 0",
                params![tag_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| AssetError::TagNotFound(tag_id.clone()))?;

        if let Some(expected_ty) = tag_asset_ty
            && asset_ty != expected_ty
        {
            return Err(AssetError::InvalidTagAssetType {
                tag_id: tag_id.clone(),
                asset_id: *asset_id,
                asset_ty,
                expected_ty,
            }
            .into());
        }

        let inserted = tx.execute(
            r#"
INSERT INTO asset_tags (asset_id, tag_id)
VALUES (?1, ?2)
ON CONFLICT DO NOTHING;
            "#,
            params![asset_id, tag_id],
        )?;
        if inserted == 0 {
            return Err(AssetError::TagAlreadyAssigned {
                asset_id: *asset_id,
                tag_id: tag_id.clone(),
            }
            .into());
        }

        tx.commit()?;
        Ok(())
    }

    pub fn remove_tag_from_asset(
        &self,
        asset_id: &UntypedAssetId,
        tag_id: &TagId,
    ) -> AssetResult<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        let tag_exists = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM tags WHERE id = ?1 AND is_deleted = 0)",
            params![tag_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !tag_exists {
            return Err(AssetError::TagNotFound(tag_id.clone()).into());
        }

        let deleted = tx.execute(
            "DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
            params![asset_id, tag_id],
        )?;
        if deleted == 0 {
            return Err(AssetError::TagNotAssigned {
                asset_id: *asset_id,
                tag_id: tag_id.clone(),
            }
            .into());
        }

        tx.commit()?;
        Ok(())
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
    fn tags_are_upserted_queried_and_filtered() -> AssetResult<()> {
        let db = AssetIndexDb::open_in_memory()?;
        let last_modified = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
        let mut tag = Tag::new(
            "Original".to_string(),
            Some(TestAsset::TYPE_NAME.to_string()),
        );

        db.upsert_tag(&tag, last_modified)?;

        let stored = db.conn.lock().query_row(
            "SELECT name, asset_ty, last_modified, is_deleted FROM tags WHERE id = ?1",
            params![tag.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, DateTime<Utc>>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            },
        )?;
        assert_eq!(stored.0, "Original");
        assert_eq!(stored.1.as_deref(), Some(TestAsset::TYPE_NAME));
        assert_eq!(stored.2, last_modified);
        assert!(!stored.3);

        tag.name = "Ignored".to_string();
        db.upsert_tag(&tag, last_modified)?;
        let stored_name = db.conn.lock().query_row(
            "SELECT name FROM tags WHERE id = ?1",
            params![tag.id],
            |row| row.get::<_, String>(0),
        )?;
        assert_eq!(stored_name, "Original");

        let updated_at = Utc.with_ymd_and_hms(2026, 4, 2, 0, 0, 0).unwrap();
        db.upsert_tag(&tag, updated_at)?;
        let stored = db.conn.lock().query_row(
            "SELECT name, asset_ty, last_modified FROM tags WHERE id = ?1",
            params![tag.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, DateTime<Utc>>(2)?,
                ))
            },
        )?;
        assert_eq!(stored.0, "Ignored");
        assert_eq!(stored.1.as_deref(), Some(TestAsset::TYPE_NAME));
        assert_eq!(stored.2, updated_at);

        let changed_asset_ty_tag: Tag = toml::from_str(&format!(
            r#"
id = "{}"
name = "Changed type"
asset_ty = "{}"
            "#,
            tag.id,
            OtherAsset::TYPE_NAME,
        ))?;
        assert!(
            db.upsert_tag(
                &changed_asset_ty_tag,
                Utc.with_ymd_and_hms(2026, 4, 3, 0, 0, 0).unwrap(),
            )
            .is_err()
        );

        let stored = db.conn.lock().query_row(
            "SELECT name, asset_ty, last_modified FROM tags WHERE id = ?1",
            params![tag.id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, DateTime<Utc>>(2)?,
                ))
            },
        )?;
        assert_eq!(stored.0, "Ignored");
        assert_eq!(stored.1.as_deref(), Some(TestAsset::TYPE_NAME));
        assert_eq!(stored.2, updated_at);

        let untyped_tag = Tag::new("All assets".to_string(), None);
        let other_asset_tag = Tag::new(
            "Other assets".to_string(),
            Some(OtherAsset::TYPE_NAME.to_string()),
        );
        db.upsert_tag(&untyped_tag, updated_at)?;
        db.upsert_tag(&other_asset_tag, updated_at)?;

        let queried = db.get_tag(tag.id.clone())?;
        assert_eq!(queried.id, tag.id);
        assert_eq!(queried.name, "Ignored");
        assert_eq!(queried.asset_ty.as_deref(), Some(TestAsset::TYPE_NAME));

        assert_eq!(
            db.get_tags(TagFilter::default())?
                .iter()
                .map(|tag| tag.name.as_str())
                .collect::<Vec<_>>(),
            vec!["All assets", "Ignored", "Other assets"]
        );

        let untyped = db.get_tags(TagFilter {
            asset_ty: Some(None),
            ..Default::default()
        })?;
        assert_eq!(untyped.len(), 1);
        assert_eq!(untyped[0].id, untyped_tag.id);

        let typed = db.get_tags(TagFilter {
            asset_ty: Some(Some(TestAsset::TYPE_NAME.to_string())),
            ..Default::default()
        })?;
        assert_eq!(typed.len(), 1);
        assert_eq!(typed[0].id, tag.id);

        assert!(
            db.get_tags(TagFilter {
                asset_ty: Some(Some("missing_asset".to_string())),
                ..Default::default()
            })?
            .is_empty()
        );

        Ok(())
    }

    #[test]
    fn tag_asset_association_lifecycle() -> AssetResult<()> {
        let db = AssetIndexDb::open_in_memory()?;
        let bundle_id = BundleId::new(Uuid::from_u128(63));
        let last_modified = Utc.with_ymd_and_hms(2026, 4, 5, 0, 0, 0).unwrap();
        db.upsert_bundle(&AssetBundleMetadata {
            bundle_id,
            name: "Tagged assets".to_string(),
            last_modified,
        })?;

        let test_asset_id = UntypedAssetId::new(Uuid::from_u128(64));
        let other_asset_id = UntypedAssetId::new(Uuid::from_u128(65));
        for (asset_id, ty) in [
            (test_asset_id, TestAsset::TYPE_NAME),
            (other_asset_id, OtherAsset::TYPE_NAME),
        ] {
            db.add_asset(&AssetMetadata {
                asset_id,
                ty: ty.to_string(),
                bundle_id,
                relative_path: format!("{asset_id}.asset"),
                revision: 0,
                last_modified,
                in_memory: false,
            })?;
        }

        let typed_tag = Tag::new(
            "Test assets".to_string(),
            Some(TestAsset::TYPE_NAME.to_string()),
        );
        let untyped_tag = Tag::new("Any assets".to_string(), None);
        db.upsert_tag(&typed_tag, last_modified)?;
        db.upsert_tag(&untyped_tag, last_modified)?;

        db.add_tag_to_asset(&test_asset_id, &typed_tag.id)?;
        assert!(db.add_tag_to_asset(&test_asset_id, &typed_tag.id).is_err());
        assert!(db.add_tag_to_asset(&other_asset_id, &typed_tag.id).is_err());
        db.add_tag_to_asset(&other_asset_id, &untyped_tag.id)?;

        let tagged = db.get_assets(UntypedAssetFilter {
            tag: Some(typed_tag.id.clone()),
            ..Default::default()
        })?;
        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0].asset_id, test_asset_id);

        db.remove_tag_from_asset(&test_asset_id, &typed_tag.id)?;
        assert!(
            db.remove_tag_from_asset(&test_asset_id, &typed_tag.id)
                .is_err()
        );
        assert!(
            db.get_assets(UntypedAssetFilter {
                tag: Some(typed_tag.id.clone()),
                ..Default::default()
            })?
            .is_empty()
        );

        db.delete_tag(&untyped_tag.id)?;

        assert!(db.get_tag(untyped_tag.id.clone()).is_err());
        assert!(
            db.get_tags(TagFilter::default())?
                .iter()
                .all(|tag| tag.id != untyped_tag.id)
        );
        assert_eq!(
            db.get_tags(TagFilter {
                is_deleted: true,
                ..Default::default()
            })?
            .iter()
            .map(|tag| tag.id.clone())
            .collect::<Vec<_>>(),
            vec![untyped_tag.id.clone()]
        );
        assert_eq!(
            db.get_tags(TagFilter {
                asset_ty: Some(None),
                is_deleted: true,
            })?[0]
                .id,
            untyped_tag.id
        );
        assert!(
            db.get_tags(TagFilter {
                asset_ty: Some(Some(TestAsset::TYPE_NAME.to_string())),
                is_deleted: true,
            })?
            .is_empty()
        );
        assert!(
            db.get_assets(UntypedAssetFilter {
                tag: Some(untyped_tag.id.clone()),
                ..Default::default()
            })?
            .is_empty()
        );
        db.delete_asset(&other_asset_id)?;
        assert_eq!(
            db.get_assets(UntypedAssetFilter {
                tag: Some(untyped_tag.id.clone()),
                is_deleted: true,
                ..Default::default()
            })?[0]
                .asset_id,
            other_asset_id
        );
        assert!(
            db.add_tag_to_asset(&test_asset_id, &untyped_tag.id)
                .is_err()
        );
        assert!(
            db.remove_tag_from_asset(&other_asset_id, &untyped_tag.id)
                .is_err()
        );
        assert!(db.delete_tag(&untyped_tag.id).is_err());

        let mut rediscovered_tag = untyped_tag.clone();
        rediscovered_tag.name = "Rediscovered tag".to_string();
        db.upsert_tag(
            &rediscovered_tag,
            Utc.with_ymd_and_hms(2026, 4, 6, 0, 0, 0).unwrap(),
        )?;
        assert!(db.get_tag(untyped_tag.id.clone()).is_err());

        let (association_count, is_deleted) = db.conn.lock().query_row(
            r#"
SELECT
    (SELECT COUNT(*) FROM asset_tags WHERE tag_id = ?1),
    (SELECT is_deleted FROM tags WHERE id = ?1);
            "#,
            params![untyped_tag.id],
            |row| Ok((row.get::<_, u32>(0)?, row.get::<_, bool>(1)?)),
        )?;
        assert_eq!(association_count, 1);
        assert!(is_deleted);

        Ok(())
    }

    #[test]
    fn deleted_assets_are_hidden_and_can_be_restored() -> AssetResult<()> {
        let db = AssetIndexDb::open_in_memory()?;
        let bundle_id = BundleId::new(Uuid::from_u128(68));
        let last_modified = Utc.with_ymd_and_hms(2026, 4, 7, 0, 0, 0).unwrap();
        db.upsert_bundle(&AssetBundleMetadata {
            bundle_id,
            name: "Deleted assets".to_string(),
            last_modified,
        })?;

        let asset_id = UntypedAssetId::new(Uuid::from_u128(69));
        let asset = AssetMetadata {
            asset_id,
            ty: TestAsset::TYPE_NAME.to_string(),
            bundle_id,
            relative_path: "original.asset".to_string(),
            revision: 5,
            last_modified,
            in_memory: false,
        };
        db.add_asset(&asset)?;
        let tag = Tag::new(
            "Restored tag".to_string(),
            Some(TestAsset::TYPE_NAME.to_string()),
        );
        db.upsert_tag(&tag, last_modified)?;
        db.add_tag_to_asset(&asset_id, &tag.id)?;

        db.delete_asset(&asset_id)?;

        assert!(db.get_asset(&asset_id).is_err());
        assert!(db.get_assets(UntypedAssetFilter::default())?.is_empty());
        assert!(
            db.get_assets(UntypedAssetFilter {
                tag: Some(tag.id.clone()),
                ..Default::default()
            })?
            .is_empty()
        );
        assert!(db.update_asset(&asset_id).is_err());
        assert!(db.add_tag_to_asset(&asset_id, &tag.id).is_err());
        assert!(db.delete_asset(&asset_id).is_err());

        let deleted = db.get_assets(UntypedAssetFilter {
            is_deleted: true,
            ..Default::default()
        })?;
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].asset_id, asset_id);
        assert_eq!(deleted[0].relative_path, "original.asset");
        assert_eq!(deleted[0].revision, 5);
        assert_eq!(
            db.get_assets(UntypedAssetFilter {
                tag: Some(tag.id.clone()),
                is_deleted: true,
                ..Default::default()
            })?[0]
                .asset_id,
            asset_id
        );
        assert!(
            db.get_assets(UntypedAssetFilter {
                ty: Some(OtherAsset::TYPE_NAME.to_string()),
                is_deleted: true,
                ..Default::default()
            })?
            .is_empty()
        );

        let association_count = db.conn.lock().query_row(
            "SELECT COUNT(*) FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
            params![asset_id, tag.id],
            |row| row.get::<_, u32>(0),
        )?;
        assert_eq!(association_count, 1);

        let restored = AssetMetadata {
            relative_path: "restored.asset".to_string(),
            revision: 0,
            ..asset
        };
        db.add_asset(&restored)?;

        let stored = db.get_asset(&asset_id)?;
        assert_eq!(stored.revision, 0);
        assert_eq!(stored.relative_path, "restored.asset");
        assert!(
            db.get_assets(UntypedAssetFilter {
                is_deleted: true,
                ..Default::default()
            })?
            .is_empty()
        );
        let tagged = db.get_assets(UntypedAssetFilter {
            tag: Some(tag.id.clone()),
            ..Default::default()
        })?;
        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0].asset_id, asset_id);

        db.delete_asset(&asset_id)?;
        db.replace_assets(&bundle_id, std::slice::from_ref(&restored))?;
        assert_eq!(db.get_asset(&asset_id)?.revision, 0);

        db.replace_assets(&bundle_id, &[])?;
        assert!(db.get_asset(&asset_id).is_err());

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
