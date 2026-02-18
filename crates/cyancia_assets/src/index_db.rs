use sqlx::SqlitePool;

use crate::asset::{AssetMetadata, AssetUrl};

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
CREATE TABLE bundles (
    bundle_id TEXT PRIMARY KEY,
    content_hash TEXT,
    filename TEXT,
    readonly INTEGER NOT NULL CHECK (readonly IN (0, 1))
);

CREATE UNIQUE INDEX idx_bundles_filename_unique
ON bundles(filename)
WHERE filename IS NOT NULL;

CREATE TABLE assets (
    asset_id TEXT PRIMARY KEY,
    bundle_id TEXT NOT NULL,
    type TEXT NOT NULL,
    relative_path TEXT NOT NULL,

    revision INTEGER NOT NULL DEFAULT 0,
    physical_location INTEGER NOT NULL DEFAULT 2, -- 0=memory,1=local,2=bundle

    content_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL,

    FOREIGN KEY(bundle_id) REFERENCES bundles(bundle_id),
    UNIQUE(bundle_id, relative_path),
    CHECK (physical_location IN (0, 1, 2))
);

CREATE INDEX idx_assets_bundle_path ON assets(bundle_id, relative_path);
CREATE INDEX idx_assets_type ON assets(type);
CREATE INDEX idx_assets_bundle ON assets(bundle_id);
CREATE INDEX idx_assets_revision ON assets(revision);
CREATE INDEX idx_assets_location ON assets(physical_location);
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_bundle(&self, bundle_id: &str, content_hash: &str) -> sqlx::Result<()> {
        sqlx::query(
            r#"
INSERT INTO bundles (filename, content_hash)
VALUES (?, ?)
ON CONFLICT(filename) DO UPDATE SET
    content_hash = excluded.content_hash
            "#,
        )
        .bind(bundle_id)
        .bind(content_hash)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_asset(&self, asset: AssetMetadata) -> sqlx::Result<()> {
        sqlx::query(
            r#"
INSERT INTO assets (
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at
)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(bundle_id, relative_path) DO UPDATE SET
    bundle_id = excluded.bundle_id,
    type = excluded.type,
    relative_path = excluded.relative_path,
    content_hash = excluded.content_hash,
    updated_at = excluded.updated_at
        "#,
        )
        .bind(&*asset.bundle_id)
        .bind(&asset.asset_type)
        .bind(&asset.relative_path)
        .bind(&asset.content_hash)
        .bind(asset.updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn upsert_many_assets(
        &self,
        assets: impl IntoIterator<Item = AssetMetadata>,
    ) -> sqlx::Result<()> {
        let mut tx = self.pool.begin().await?;

        for asset in assets {
            sqlx::query(
                r#"
INSERT INTO assets (
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at
)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(bundle_id, relative_path) DO UPDATE SET
    bundle_id = excluded.bundle_id,
    type = excluded.type,
    relative_path = excluded.relative_path,
    content_hash = excluded.content_hash,
    updated_at = excluded.updated_at
                "#,
            )
            .bind(&*asset.bundle_id)
            .bind(&asset.asset_type)
            .bind(&asset.relative_path)
            .bind(&asset.content_hash)
            .bind(asset.updated_at)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    pub async fn get(&self, url: &AssetUrl) -> sqlx::Result<AssetMetadata> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
SELECT
    asset_id,
    bundle_id,
    type,
    relative_path,
    revision,
    physical_location,
    content_hash,
    updated_at
FROM assets
WHERE bundle_id = ?
  AND relative_path = ?
LIMIT 1;
        "#,
        )
        .bind(&*url.source())
        .bind(url.path_str())
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

    pub async fn update(
        &self,
        url: &AssetUrl,
        expected_old_hash: i64,
        new_hash: i64,
    ) -> sqlx::Result<Option<AssetMetadata>> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
latest AS (
    SELECT
        asset_id,
        bundle_id,
        type,
        relative_path,
        revision,
        content_hash
    FROM assets
    WHERE bundle_id = ?
      AND relative_path = ?
    ORDER BY revision DESC
    LIMIT 1
)
INSERT INTO assets (
    asset_id,
    bundle_id,
    type,
    relative_path,
    revision,
    physical_location,
    content_hash,
    updated_at
)
SELECT
    asset_id,
    bundle_id,
    type,
    relative_path,
    revision + 1,
    0,
    ?,
    ?
FROM latest
WHERE content_hash = ?
RETURNING
    asset_id,
    bundle_id,
    type,
    relative_path,
    revision,
    physical_location,
    content_hash,
    updated_at;
        "#,
        )
        .bind(new_hash)
        .bind(chrono::Utc::now())
        .bind(&*url.source())
        .bind(url.path_str())
        .bind(expected_old_hash)
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn write(&self, url: &AssetUrl) -> sqlx::Result<AssetMetadata> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
WITH latest AS (
    SELECT
        a.rowid AS target_rowid,
        a.physical_location,
        b.readonly
    FROM assets a
    JOIN bundles b ON b.bundle_id = a.bundle_id
    WHERE a.bundle_id = ?
      AND a.relative_path = ?
    ORDER BY a.revision DESC
    LIMIT 1
),
target AS (
    SELECT
        target_rowid,
        CASE WHEN readonly = 1 THEN 1 ELSE 2 END AS next_location
    FROM latest
    WHERE physical_location = 0
)
UPDATE assets
SET
    physical_location = (SELECT next_location FROM target),
WHERE rowid = (SELECT target_rowid FROM target)
RETURNING
    asset_id,
    bundle_id,
    type,
    relative_path,
    revision,
    physical_location,
    content_hash,
    updated_at;
        "#,
        )
        .bind(&*url.source())
        .bind(url.path_str())
        .fetch_one(&self.pool)
        .await
    }

    pub async fn discard_in_memory_assets(&self) -> sqlx::Result<u32> {
        sqlx::query_scalar::<_, u32>(
            r#"
DELETE FROM assets
WHERE physical_location = 0;
SELECT changes() AS deleted_rows;
        "#,
        )
        .fetch_one(&self.pool)
        .await
    }
}
