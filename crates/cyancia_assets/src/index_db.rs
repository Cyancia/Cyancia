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
create table if not exists bundles
(
    filename TEXT primary key,
    content_hash TEXT
)
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
create table if not exists assets
(
	bundle_id TEXT not null references bundles,
	type TEXT not null,
	relative_path TEXT not null,
	content_hash TEXT not null,
	updated_at TEXT not null,
	unique (bundle_id, relative_path)
)
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
        .bind(asset.updated_at.to_rfc3339())
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
            .bind(asset.updated_at.to_rfc3339())
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    pub async fn get_by_url(&self, url: &AssetUrl) -> sqlx::Result<Option<AssetMetadata>> {
        sqlx::query_as::<_, AssetMetadata>(
            r#"
SELECT
    bundle_id,
    type AS asset_type,
    relative_path,
    content_hash,
    updated_at
FROM assets
WHERE bundle_id = ?
  AND relative_path = ?
LIMIT 1
        "#,
        )
        .bind(&*url.source())
        .bind(url.path_str())
        .fetch_optional(&self.pool)
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

    pub async fn update_by_url(&self, url: &AssetUrl, content_hash: String) -> sqlx::Result<()> {
        sqlx::query(
            r#"
UPDATE assets
SET
    content_hash = ?,
    updated_at = ?
WHERE bundle_id = ?
    AND relative_path = ?
        "#,
        )
        .bind(content_hash)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&*url.source())
        .bind(url.path_str())
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
