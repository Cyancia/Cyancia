use std::{collections::HashMap, fs::File, path::Path};

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    asset::{AssetId, AssetMetadata, AssetUrl},
    bundle::{AssetBundleMetadata, BundleId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleStatus {
    UpToDate,
    Outdated,
}

pub struct AssetIndexDb {
    pool: SqlitePool,
}

impl AssetIndexDb {
    pub async fn connect(path: impl AsRef<Path>) -> sqlx::Result<Self> {
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

    async fn initialize_tables(&self) -> sqlx::Result<()> {
        sqlx::query(
            r#"
CREATE TABLE IF NOT EXISTS assets (
    asset_id TEXT,
    ty TEXT NOT NULL,
    bundle_id TEXT NOT NULL,
    relative_path TEXT,
    revision INTEGER NOT NULL,
    in_memory BOOLEAN NOT NULL,

    UNIQUE (asset_id, revision),
    UNIQUE (bundle_id, relative_path),
    FOREIGN KEY (bundle_id) REFERENCES bundles(bundle_id)

    CHECK (
        (in_memory = 1 AND relative_path IS NULL)
        OR
        (in_memory = 0 AND relative_path IS NOT NULL)
    )
);

CREATE TABLE IF NOT EXISTS bundles (
    bundle_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    last_modified TEXT
);
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_bundle(&self, bundle: &AssetBundleMetadata) -> sqlx::Result<BundleStatus> {
        let same = sqlx::query_scalar::<_, u32>(
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

        Ok(if same.is_none() {
            BundleStatus::UpToDate
        } else {
            BundleStatus::Outdated
        })
    }

    pub async fn upsert_assets(
        &self,
        bundle: &BundleId,
        assets: &[AssetMetadata],
    ) -> sqlx::Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
CREATE TEMP TABLE manifest_assets (
    asset_id TEXT NOT NULL,
    ty TEXT NOT NULL,
    bundle_id TEXT NOT NULL,
    relative_path TEXT,
    revision INTEGER NOT NULL,
    in_memory BOOLEAN NOT NULL DEFAULT 0,
    PRIMARY KEY (asset_id, revision),
    UNIQUE (bundle_id, relative_path)
);
        "#,
        )
        .execute(&mut *tx)
        .await?;

        for asset in assets {
            sqlx::query(
                r#"
INSERT INTO manifest_assets(asset_id, ty, bundle_id, relative_path, revision, in_memory)
VALUES (?, ?, ?, ?, ?, ?);
            "#,
            )
            .bind(&asset.asset_id)
            .bind(&asset.ty)
            .bind(&asset.bundle_id)
            .bind(&asset.relative_path)
            .bind(asset.revision)
            .bind(asset.in_memory)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            r#"
DELETE FROM assets AS a
WHERE a.bundle_id = ?1
    AND NOT EXISTS (
        SELECT 1
        FROM manifest_assets AS m
        WHERE m.bundle_id = ?1
            AND m.asset_id = a.asset_id
            AND m.revision = a.revision
    );

INSERT INTO assets (
    asset_id, ty, bundle_id, relative_path, revision, in_memory
)
SELECT
    m.asset_id, m.ty, m.bundle_id, m.relative_path, m.revision, m.in_memory
FROM manifest_assets AS m
LEFT JOIN assets AS a
    ON a.bundle_id = ?1
    AND a.asset_id  = m.asset_id
    AND a.revision  = m.revision
WHERE m.bundle_id = ?1
    AND a.asset_id IS NULL;

DROP TABLE manifest_assets;
        "#,
        )
        .bind(bundle)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn get_asset(&self, id: &AssetId) -> sqlx::Result<AssetMetadata> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
SELECT
    asset_id,
    ty,
    bundle_id,
    relative_path,
    revision,
    in_memory
FROM assets
WHERE asset_id = ?
ORDER BY revision DESC
LIMIT 1
        "#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn get_assets_by_type(&self, asset_type: &str) -> sqlx::Result<Vec<AssetMetadata>> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
WITH ordered AS (
    SELECT
        asset_id,
        ty,
        bundle_id,
        relative_path,
        revision,
        in_memory,
        ROW_NUMBER() OVER (PARTITION BY asset_id ORDER BY revision DESC) AS ord
    FROM assets
    WHERE ty = ?
)
SELECT
    asset_id,
    ty,
    bundle_id,
    relative_path,
    revision,
    in_memory
FROM ordered
WHERE ord = 1
ORDER BY relative_path ASC
        "#,
        )
        .bind(asset_type)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_assets_by_bundle(
        &self,
        bundle_id: &BundleId,
    ) -> sqlx::Result<Vec<AssetMetadata>> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
WITH ordered AS (
    SELECT
        asset_id,
        ty,
        bundle_id,
        relative_path,
        revision,
        in_memory,
        ROW_NUMBER() OVER (PARTITION BY asset_id ORDER BY revision DESC) AS ord
    FROM assets
    WHERE bundle_id = ?
)
SELECT
    asset_id,
    ty,
    bundle_id,
    relative_path,
    revision,
    in_memory
FROM ordered
WHERE ord = 1
ORDER BY relative_path ASC
        "#,
        )
        .bind(bundle_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn update_asset(&self, id: &AssetId, new_hash: i64) -> sqlx::Result<u32> {
        sqlx::query_scalar::<_, u32>(
            r#"
WITH latest AS (
    SELECT
        asset_id,
        ty,
        bundle_id,
        revision,
        in_memory
    FROM assets
    WHERE asset_id = ?
    ORDER BY revision DESC
    LIMIT 1
)
INSERT INTO assets (
    asset_id,
    ty,
    bundle_id,
    relative_path,
    revision,
    in_memory
)
SELECT
    asset_id,
    ty,
    bundle_id,
    ? = NULL,
    revision + 1 AS revision,
    true AS in_memory
FROM latest
RETURNING revision
        "#,
        )
        .bind(id)
        .bind(new_hash)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn write_asset(&self, id: &AssetId, new_path: &str) -> sqlx::Result<u32> {
        sqlx::query_scalar::<_, u32>(
            r#"
WITH latest AS (
    SELECT revision
    FROM assets
    WHERE asset_id = ?
    ORDER BY revision DESC
    LIMIT 1
)
UPDATE assets
SET in_memory = false, relative_path = ?
WHERE asset_id = ? AND revision = (SELECT revision FROM latest) AND in_memory = true
RETURNING revision
        "#,
        )
        .bind(id)
        .bind(new_path)
        .bind(id)
        .fetch_one(&self.pool)
        .await
    }

    pub async fn revert_asset(&self, id: &Uuid) -> sqlx::Result<()> {
        sqlx::query(
            r#"
DELETE FROM assets WHERE in_memory = true AND asset_id = ?
        "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn revert_all_assets(&self) -> sqlx::Result<()> {
        sqlx::query(
            r#"
DELETE FROM assets WHERE in_memory = true
        "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_bundle(&self, id: &BundleId) -> sqlx::Result<AssetBundleMetadata> {
        sqlx::query_as::<_, AssetBundleMetadata>(
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
        .await
    }
}
