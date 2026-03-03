use std::{fs::File, marker::PhantomData, path::Path};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::{
    asset::{Asset, AssetId, AssetMetadata},
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
    conn: Connection,
}

impl AssetIndexDb {
    pub fn connect(path: impl AsRef<Path>) -> AssetResult<Self> {
        let path = path.as_ref();
        if !path.exists() {
            File::create(path)?;
        }

        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let db = Self { conn };
        db.initialize_tables()?;
        db.revert_all_assets()?;
        Ok(db)
    }

    fn initialize_tables(&self) -> AssetResult<()> {
        self.conn.execute_batch(
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
        let result = self.conn.query_row(
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
        let tx = self.conn.unchecked_transaction()?;

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
        let needs_update = {
            let result = self.conn.query_row(
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

        let tx = self.conn.unchecked_transaction()?;

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

    pub fn add_asset(&self, asset: &AssetMetadata) -> AssetResult<AssetId> {
        let asset_id = asset.asset_id;
        let tx = self.conn.unchecked_transaction()?;

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

    pub fn get_asset(&self, id: &AssetId) -> AssetResult<AssetMetadata> {
        let asset = self.conn.query_row(
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
        let mut stmt = self.conn.prepare(
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
            params![
                filter.ty,
                filter.tag.as_ref().map(|t| t),
                filter.bundle.as_ref().map(|b| b),
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

    pub fn update_asset(&self, id: &AssetId) -> AssetResult<u32> {
        let revision = self.conn.query_row(
            r#"
WITH latest AS (
    SELECT asset_id, revision
    FROM asset_revisions
    WHERE asset_id = ?1
    ORDER BY revision DESC
    LIMIT 1
)
INSERT INTO asset_revisions (
    asset_id,
    relative_path,
    revision,
    last_modified,
    in_memory
)
SELECT
    asset_id,
    NULL AS relative_path,
    revision + 1 AS revision,
    ?2 AS last_modified,
    1 AS in_memory
FROM latest
RETURNING revision;
            "#,
            params![id, Utc::now()],
            |row| row.get::<_, u32>(0),
        )?;

        Ok(revision)
    }

    pub fn write_asset(
        &self,
        id: &AssetId,
        new_path: &str,
        last_modified: DateTime<Utc>,
    ) -> AssetResult<u32> {
        let revision = self.conn.query_row(
            r#"
WITH latest AS (
    SELECT revision
    FROM asset_revisions
    WHERE asset_id = ?1
    ORDER BY revision DESC
    LIMIT 1
)
UPDATE asset_revisions
SET in_memory = 0, relative_path = ?2, last_modified = ?3
WHERE asset_id = ?1
  AND revision = (SELECT revision FROM latest)
  AND in_memory = 1
RETURNING revision;
            "#,
            params![id, new_path, last_modified],
            |row| row.get::<_, u32>(0),
        )?;

        Ok(revision)
    }

    pub fn revert_asset(&self, id: &Uuid) -> AssetResult<()> {
        self.conn.execute(
            "DELETE FROM asset_revisions WHERE in_memory = 1 AND asset_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn revert_all_assets(&self) -> AssetResult<()> {
        self.conn
            .execute("DELETE FROM asset_revisions WHERE in_memory = 1", [])?;
        Ok(())
    }

    pub fn get_bundle(&self, id: &BundleId) -> AssetResult<AssetBundleMetadata> {
        let bundle = self.conn.query_row(
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
