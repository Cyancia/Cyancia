use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    asset::{AssetMetadata, AssetUrl},
    bundle::AssetBundleMetadata,
};

pub struct AssetIndexDb {
    pool: SqlitePool,
}

impl AssetIndexDb {
    pub async fn connect(database_url: &str) -> sqlx::Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        Ok(Self { pool })
    }

    pub async fn initialize_tables(&self) -> sqlx::Result<()> {
        sqlx::query(
            r#"
CREATE TABLE IF NOT EXISTS assets (
    asset_id TEXT,
    ty TEXT NOT NULL,
    bundle_id TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    content_hash INTEGER NOT NULL,
    revision INTEGER NOT NULL,
    in_memory BOOLEAN NOT NULL,

    UNIQUE KEY (bundle_id, relative_path),
    FOREIGN KEY (bundle_id) REFERENCES bundles(bundle_id)
)

CREATE TABLE IF NOT EXISTS bundles (
    bundle_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
)
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_bundle(&self, bundle: &AssetBundleMetadata) -> sqlx::Result<()> {
        sqlx::query(
            r#"
INSERT INTO bundles (
    bundle_id,
    name
)
VALUES (?, ?)
            "#,
        )
        .bind(&bundle.bundle_id)
        .bind(&bundle.name)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_asset(&self, asset: &AssetMetadata) -> sqlx::Result<()> {
        sqlx::query(
            r#"
INSERT INTO assets (
    asset_id,
    ty,
    bundle_id,
    relative_path,
    content_hash,
    revision,
    in_memory
)
VALUES (?, ?, ?, ?, ?, ?, ? = false)
        "#,
        )
        .bind(&asset.asset_id)
        .bind(&asset.ty)
        .bind(&asset.bundle_id)
        .bind(&asset.relative_path)
        .bind(asset.content_hash)
        .bind(asset.revision)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_asset(&self, id: &Uuid) -> sqlx::Result<AssetMetadata> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
SELECT
    asset_id,
    ty,
    bundle_id,
    relative_path,
    content_hash,
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

    pub async fn all_by_type(&self, asset_type: &str) -> sqlx::Result<Vec<AssetMetadata>> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
SELECT
    bundle_id,
    type AS asset_type,
    relative_path,
    content_hash,
    updated_at
FROM assets
WHERE type = ?
ORDER BY relative_path ASC
        "#,
        )
        .bind(asset_type)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn update_asset(&self, id: &Uuid, new_hash: i64) -> sqlx::Result<u32> {
        sqlx::query_scalar::<_, u32>(
            r#"
WITH latest AS (
    SELECT
        asset_id,
        ty,
        content_hash,
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
    content_hash,
    revision,
    in_memory
)
SELECT
    asset_id,
    ty,
    bundle_id,
    ? = NULL,
    ? AS content_hash,
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

    pub async fn write_asset(&self, id: &Uuid, new_path: &str) -> sqlx::Result<u32> {
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

    pub async fn get_bundle(&self, id: &Uuid) -> sqlx::Result<AssetBundleMetadata> {
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
