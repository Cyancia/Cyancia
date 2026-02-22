use std::{collections::HashMap, fs::File, marker::PhantomData, path::Path};

use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
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
    pool: SqlitePool,
}

impl AssetIndexDb {
    pub async fn connect(path: impl AsRef<Path>) -> AssetResult<Self> {
        let path = path.as_ref();
        if !path.exists() {
            File::create(path)?;
        }

        let database_url = format!("sqlite://{}", path.display());
        let pool = SqlitePool::connect(&database_url).await?;
        let db = Self { pool };
        db.initialize_tables().await?;
        db.revert_all_assets().await?;
        Ok(db)
    }

    async fn initialize_tables(&self) -> AssetResult<()> {
        sqlx::query(
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
    in_memory BOOLEAN NOT NULL,

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
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn upsert_bundle(&self, bundle: &AssetBundleMetadata) -> AssetResult<ItemStatus> {
        let none_if_latest = sqlx::query_scalar::<_, u32>(
            r#"
INSERT INTO bundles (bundle_id, name, last_modified)
VALUES (?, ?, ?)
ON CONFLICT(bundle_id) DO UPDATE SET
    name = excluded.name,
    last_modified = excluded.last_modified
WHERE bundles.last_modified IS NOT excluded.last_modified
RETURNING 0;
            "#,
        )
        .bind(&bundle.bundle_id)
        .bind(&bundle.name)
        .bind(&bundle.last_modified)
        .fetch_optional(&self.pool)
        .await?;

        Ok(if none_if_latest.is_none() {
            ItemStatus::UpToDate
        } else {
            ItemStatus::Outdated
        })
    }

    pub async fn replace_assets(
        &self,
        bundle: &BundleId,
        assets: &[AssetMetadata],
    ) -> AssetResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
DELETE FROM assets WHERE bundle_id = ?
        "#,
        )
        .bind(bundle)
        .execute(&mut *tx)
        .await?;

        for asset in assets {
            sqlx::query(
                r#"
INSERT INTO assets (asset_id, ty, bundle_id) VALUES (?, ?, ?) ON CONFLICT DO NOTHING;
INSERT INTO asset_revisions (asset_id, revision, relative_path, last_modified, in_memory)
VALUES (?, ?, ?, ?, ?);
            "#,
            )
            .bind(&asset.asset_id)
            .bind(&asset.ty)
            .bind(asset.bundle_id)
            .bind(&asset.asset_id)
            .bind(asset.revision)
            .bind(&asset.relative_path)
            .bind(&asset.last_modified)
            .bind(asset.in_memory)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    pub async fn upsert_tag(&self, tag: &Tag, last_modified: DateTime<Utc>) -> AssetResult<()> {
        let none_if_latest = sqlx::query_scalar::<_, u32>(
            r#"
INSERT INTO tags (tag_id, name, last_modified)
VALUES (?, ?, ?)
ON CONFLICT(tag_id) DO UPDATE SET
    name = excluded.name,
    last_modified = excluded.last_modified
WHERE tags.last_modified IS NOT excluded.last_modified
RETURNING 0;
        "#,
        )
        .bind(tag.id())
        .bind(tag.name())
        .bind(last_modified.to_rfc3339())
        .fetch_optional(&self.pool)
        .await?;

        if none_if_latest.is_none() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
DELETE FROM asset_tags WHERE tag_id = ?
        "#,
        )
        .bind(tag.id())
        .execute(&mut *tx)
        .await?;

        for asset_id in tag.assets() {
            println!("Associating asset {} with tag {}", asset_id, tag.name());
            sqlx::query(
                r#"
INSERT INTO asset_tags (asset_id, tag_id) VALUES (?, ?)
            "#,
            )
            .bind(asset_id)
            .bind(tag.id())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    pub async fn add_asset(&self, asset: &AssetMetadata) -> AssetResult<AssetId> {
        let asset_id = asset.asset_id;
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
INSERT INTO assets (asset_id, ty, bundle_id) VALUES (?, ?, ?) ON CONFLICT DO NOTHING;
            "#,
        )
        .bind(&asset.asset_id)
        .bind(&asset.ty)
        .bind(asset.bundle_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
INSERT INTO asset_revisions (asset_id, revision, relative_path, last_modified, in_memory)
VALUES (?, ?, ?, ?, ?);
            "#,
        )
        .bind(&asset.asset_id)
        .bind(asset.revision)
        .bind(&asset.relative_path)
        .bind(&asset.last_modified)
        .bind(asset.in_memory)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(asset_id)
    }

    pub async fn get_asset(&self, id: &AssetId) -> AssetResult<AssetMetadata> {
        let asset = sqlx::query_as::<_, AssetMetadata>(
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
WHERE r.asset_id = ?
ORDER BY r.revision DESC
LIMIT 1
        "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(asset)
    }

    pub async fn get_assets(&self, filter: UntypedAssetFilter) -> AssetResult<Vec<AssetMetadata>> {
        let assets = sqlx::query_as::<_, AssetMetadata>(
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
        )
        .bind(filter.ty)
        .bind(filter.tag)
        .bind(filter.bundle)
        .fetch_all(&self.pool)
        .await?;

        Ok(assets)
    }

    pub async fn update_asset(&self, id: &AssetId) -> AssetResult<u32> {
        let revision = sqlx::query_scalar::<_, u32>(
            r#"
WITH latest AS (
    SELECT
        asset_id,
        revision,
        last_modified,
        in_memory
    FROM asset_revisions
    WHERE asset_id = ?
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
    NULL as relative_path,
    revision + 1 AS revision,
    ? AS last_modified,
    true AS in_memory
FROM latest
RETURNING revision;
        "#,
        )
        .bind(id)
        .bind(Utc::now())
        .fetch_one(&self.pool)
        .await?;

        Ok(revision)
    }

    pub async fn write_asset(
        &self,
        id: &AssetId,
        new_path: &str,
        last_modified: DateTime<Utc>,
    ) -> AssetResult<u32> {
        let revision = sqlx::query_scalar::<_, u32>(
            r#"
WITH latest AS (
    SELECT revision
    FROM asset_revisions
    WHERE asset_id = ?1
    ORDER BY revision DESC
    LIMIT 1
)
UPDATE asset_revisions
SET in_memory = false, relative_path = ?2, last_modified = ?3
WHERE asset_id = ?1 AND revision = (SELECT revision FROM latest) AND in_memory = true
RETURNING revision;
        "#,
        )
        .bind(id)
        .bind(new_path)
        .bind(last_modified)
        .fetch_one(&self.pool)
        .await?;

        Ok(revision)
    }

    pub async fn revert_asset(&self, id: &Uuid) -> AssetResult<()> {
        sqlx::query(
            r#"
DELETE FROM asset_revisions WHERE in_memory = true AND asset_id = ?
        "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn revert_all_assets(&self) -> AssetResult<()> {
        sqlx::query(
            r#"
DELETE FROM asset_revisions WHERE in_memory = true
        "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_bundle(&self, id: &BundleId) -> AssetResult<AssetBundleMetadata> {
        let bundle = sqlx::query_as::<_, AssetBundleMetadata>(
            r#"
SELECT
    bundle_id,
    name
FROM bundles
WHERE bundle_id = ?
        "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;

        Ok(bundle)
    }
}
