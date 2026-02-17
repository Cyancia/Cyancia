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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::BundleId;
    use atomicow::CowArc;

    async fn create_initialized_db() -> AssetIndexDb {
        let db = AssetIndexDb::connect(&"sqlite::memory:").await.unwrap();
        db.initialize_tables().await.unwrap();
        db
    }

    async fn insert_bundle(db: &AssetIndexDb, bundle_id: &BundleId) {
        sqlx::query(
            r#"
INSERT INTO bundles (filename, content_hash)
VALUES (?, ?)
            "#,
        )
        .bind(&**bundle_id)
        .bind("bundle-hash")
        .execute(&db.pool)
        .await
        .unwrap();
    }

    fn sample_metadata(
        bundle_id: BundleId,
        asset_type: &str,
        relative_path: &str,
        content_hash: &str,
    ) -> AssetMetadata {
        AssetMetadata {
            bundle_id,
            asset_type: asset_type.to_string(),
            relative_path: relative_path.to_string(),
            content_hash: content_hash.to_string(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_connect() {
        let connected = AssetIndexDb::connect(&"sqlite://?mode=memory").await;
        assert!(connected.is_ok());
    }

    #[tokio::test]
    async fn test_initialize_tables() {
        let db = create_initialized_db().await;
        db.initialize_tables().await.unwrap();

        let table_count: (i64,) = sqlx::query_as(
            r#"
SELECT COUNT(*)
FROM sqlite_master
WHERE type = 'table'
  AND name IN ('bundles', 'assets')
            "#,
        )
        .fetch_one(&db.pool)
        .await
        .unwrap();

        assert_eq!(table_count.0, 2);
    }

    #[tokio::test]
    async fn test_upsert() {
        let db = create_initialized_db().await;
        let bundle_id = BundleId::new("bundle-upsert".to_string());
        insert_bundle(&db, &bundle_id).await;

        db.upsert_asset(sample_metadata(
            bundle_id.clone(),
            "image",
            "textures/hero.png",
            "hash-v1",
        ))
        .await
        .unwrap();

        db.upsert_asset(sample_metadata(
            bundle_id.clone(),
            "image",
            "textures/hero.png",
            "hash-v2",
        ))
        .await
        .unwrap();

        let url = AssetUrl::new(bundle_id, "textures/hero.png".into());
        let metadata = db.get_by_url(&url).await.unwrap().unwrap();
        assert_eq!(metadata.content_hash, "hash-v2");
    }

    #[tokio::test]
    async fn test_upsert_many_assets() {
        let db = create_initialized_db().await;
        let bundle_id = BundleId::new("bundle-upsert-many".to_string());
        insert_bundle(&db, &bundle_id).await;

        db.upsert_many_assets(vec![
            sample_metadata(bundle_id.clone(), "image", "a.png", "hash-a1"),
            sample_metadata(bundle_id.clone(), "image", "b.png", "hash-b1"),
            sample_metadata(bundle_id.clone(), "image", "a.png", "hash-a2"),
        ])
        .await
        .unwrap();

        let assets = db.all_by_type("image").await.unwrap();
        assert_eq!(assets.len(), 2);

        let asset_a = assets
            .iter()
            .find(|asset| asset.relative_path == "a.png")
            .unwrap();
        assert_eq!(asset_a.content_hash, "hash-a2");
    }

    #[tokio::test]
    async fn test_get_by_url() {
        let db = create_initialized_db().await;
        let bundle_id = BundleId::new("bundle-get".to_string());
        insert_bundle(&db, &bundle_id).await;

        db.upsert_asset(sample_metadata(
            bundle_id.clone(),
            "shader",
            "std/default.wgsl",
            "shader-hash",
        ))
        .await
        .unwrap();

        let existing_url = AssetUrl::new(bundle_id.clone(), "std/default.wgsl".into());
        let found = db.get_by_url(&existing_url).await.unwrap();
        assert!(found.is_some());

        let missing_url = AssetUrl::new(bundle_id, "std/missing.wgsl".into());
        let missing = db.get_by_url(&missing_url).await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_all_by_type() {
        let db = create_initialized_db().await;
        let bundle_id = BundleId::new("bundle-list".to_string());
        insert_bundle(&db, &bundle_id).await;

        db.upsert_asset(sample_metadata(
            bundle_id.clone(),
            "image",
            "b.png",
            "hash-b",
        ))
        .await
        .unwrap();
        db.upsert_asset(sample_metadata(
            bundle_id.clone(),
            "image",
            "a.png",
            "hash-a",
        ))
        .await
        .unwrap();
        db.upsert_asset(sample_metadata(bundle_id, "shader", "x.wgsl", "hash-x"))
            .await
            .unwrap();

        let images = db.all_by_type("image").await.unwrap();
        assert_eq!(images.len(), 2);
        assert_eq!(images[0].relative_path, "a.png");
        assert_eq!(images[1].relative_path, "b.png");
    }

    #[tokio::test]
    async fn test_update_by_url() {
        let db = create_initialized_db().await;
        let bundle_id = BundleId::new("bundle-update".to_string());
        insert_bundle(&db, &bundle_id).await;

        db.upsert_asset(sample_metadata(
            bundle_id.clone(),
            "image",
            "textures/background.png",
            "before",
        ))
        .await
        .unwrap();

        let url = AssetUrl::new(bundle_id, "textures/background.png".into());
        let before = db.get_by_url(&url).await.unwrap().unwrap();

        db.update_by_url(&url, "after".to_string()).await.unwrap();

        let after = db.get_by_url(&url).await.unwrap().unwrap();
        assert_eq!(after.content_hash, "after");
        assert!(after.updated_at >= before.updated_at);
    }
}
