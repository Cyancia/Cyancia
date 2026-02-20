`cyancia_assets` 应当将根目录下的全部 asset 扫描，解析成对应的 `AssetBundle` ，并且由 `AssetRegistry` 进行管理。

```rust
pub struct AssetRegistry {
    bundles: RwLock<HashMap<BundleId, AssetBundleCache>>,
    index_db: Arc<AssetIndexDB>,
}

impl AssetRegistry {
    pub fn add_bundle(&mut self, bundle: AssetBundle) {
        // 创建 AssetBundleCache
        // 更新 index_db
    }
    pub fn handle(&self, url: &AssetUrl) -> Option<AssetHandle> {
        // 获取 bundle_id 对应的 bundle
        // 从 bundle 处拿到 id
        // 创建 AssetHandle
    }
    pub fn all_of_type(&self, ty: &str) -> Vec<AssetHandle>;
}
```

```rust
pub trait Asset {
    const TYPE: &'static str; // 该 asset 的类型，例如 texture, brush_preset 等等
    fn hash(&self) -> u64;
}
```

`AssetBundle` 是一个抽象的 asset 集合，并且是只读的，对于被修改了的 asset，`AssetBundleCache` 会正确处理，因此只需要提供读取接口即可。

```rust
pub trait AssetBundle {
    fn metadata(&self) -> AssetBundleMetadata;
    fn manifest(&self) -> HashMap<AssetId, PathBuf> {
        // 返回该 bundle 内全部 asset 的 id 和相对路径
    }
    fn read(&self, path: &Path) -> Option<Arc<dyn Asset>>;
}
```

具体实现之一为 `AssetDirectory`，其本质为一个散装各类 asset 的目录

```rust
pub struct AssetDirectory {
    root: PathBuf,
    assets: HashMap<AssetId, Arc<dyn Asset>>,
}

impl AssetDirectory {
    pub fn new(root: PathBuf) -> Self {
        // 读取 <root>/manifest.toml ，其中记录所有 asset 的 id
        // 扫描整个目录，读取全部 asset
        // 如果出现了不存在 manifest.toml 中的 asset，则应当随机生成一个 AssetId 并更新 manifest.toml
        // 如果 manifest.toml 中的 asset 不存在，则应当删除该 asset 的记录
    }
}
```

之二为 `StandardAssetBundle` ，其本质为一个 `zip`

```rust
// .csb
pub struct StandardAssetBundle {
    path: PathBuf,
    archive: ZipArchive<File>,
}

impl StandardAssetBundle {
    pub fn new(path: PathBuf) -> Self {
        // 打开 zip 文件，暂时不读取
    }
}

impl AssetBundle for StandardAssetBundle {
    fn metadata(&self) -> AssetBundleMetadata {
        // 读取 zip 文件中的 metadata.toml
    }
    fn read(&self, path: &Path) -> Option<Arc<dyn Asset>> {
        // 直接读取目标处的 asset
    }
    fn read_all(&self) -> HashMap<AssetId, Arc<dyn Asset>> {
        // 读取 manifest.toml ，根据该清单一条条读取 zip 文件中的 asset
    }
}
```

```rust
pub struct BundleId(Uuid); // 被写入 AssetBundle 元数据的 id
```

`AssetBundleCache` 是一个已经被读取的 `AssetBundle` ，包含其全部的 asset ，同时其需要处理读取被修改了的 asset

```rust
pub struct AssetBundleCache {
    metadata: AssetBundleMetadata,
    bundle: Arc<dyn AssetBundle>,
    // 在原 bundle 中每一个 asset 路径对应的 id
    path_to_id: RwLock<HashMap<PathBuf, AssetId>>,
    // 保存最新版本的 asset
    assets: RwLock<HashMap<AssetId, Arc<dyn Asset>>>,
}

impl AssetBundleCache {
    pub fn new(bundle: Arc<dyn AssetBundle>) -> Self {
        // 读取 bundle 的 metadata 和 manifest
        // 检查 index_db 内 asset_id 与 manifest 是否一致
        // 如果 manifest 多出来东西了，就读取多出来的 asset 并且更新 index_db
        // 如果 manifest 少了东西，就删除 index_db 内对应的记录
    }
    pub fn get(&self, id: AssetId) -> Option<Arc<dyn Asset>> {
        // 尝试从 assets 中获取 asset，如果没有，则从 bundle 中获取 asset
    }
    pub fn update(&self, id: AssetId, asset: Arc<dyn Asset>) {
        // 将 asset 更新进内存中，暂时不写入文件系统
    }
    // 将 asset 写入文件系统中，返回新的 relative_path
    pub fn write(&self, id: AssetId) -> Result<PathBuf, AssetError> {
        // 将 asset 写入本地文件系统，路径为 <bundle_id>.modified/<original_relative_path>/<asset_filename>_<revision>.<asset_extension>
    }
    // 将 asset 退回至原 bundle 中的最新版本
    pub fn revert(&self, id: AssetId) -> Result<(), AssetError>;
    pub fn get_id(&self, path: &Path) -> Option<AssetId>;
}
```

```rust
pub struct AssetUrl {
    bundle_id: BundleId,
    relative_path: PathBuf,
}
```

`AssetHandle` 是一个 asset 的引用，也是操作 asset 的唯一接口，不可以越过 handle 直接操作，否则会导致数据库内容和文件系统内容不一致

```rust
pub struct AssetHandle<T: Asset> {
    id: AssetId,
    bundle: AssetBundleCache,
    index_db: Arc<AssetIndexDB>,
    _marker: PhantomData<T>,
}

impl<T: Asset> AssetHandle<T> {
    pub fn get(&self) -> Arc<T> {
        // 从 bundle 中获取 asset，每次获取的都是最新版的
    }
    pub fn metadata(&self) -> AssetMetadata {
        // 从 index_db 中获取 asset 的 metadata，每次获取的都是最新版的
    }
    pub fn update(&self, asset: Arc<T>) {
        // 将 asset 更新进入 bundle
        // 更新 index_db 中的 content_hash 和 revision
    }
    pub fn write(&self) -> Result<(), AssetError> {
        // 将 asset 写入文件系统
        // 更新 index_db 中的对应记录的 in_memory
    }
    pub fn revert(&self) -> Result<(), AssetError> {
        // 将 asset 退回至原 bundle 中的最新版本
        // 删除全部 index_db 中的 in_memory = true 的记录
        // 注意，如果 asset 被多次 update，存在多条 in_memory = true 的记录，那么需要全部删除
    }
}
```

```rust
pub struct AssetMetadata {
    pub asset_id: Uuid,
    pub ty: String,
    pub bundle_id: BundleId,
    pub relative_path: String,
    pub content_hash: u64,
    pub revision: u64,
    pub in_memory: bool,
}

pub struct AssetBundleMetadata {
    pub bundle_id: BundleId,
    pub name: String,
}
```

`AssetIndexDB` 是一个数据库，管理所有 asset 的元数据

```sql
CREATE TABLE IF NOT EXISTS assets (
    asset_id TEXT, -- 每一个 asset 具有的唯一标识符，不管该资产后期如何变更，这个 id 应当保持不变
    ty TEXT NOT NULL, -- asset 的类型，例如 texture, brush_preset ，该类型应当唯一，由 Asset::TYPE 提供
    bundle_id TEXT NOT NULL, -- 该 asset 所属的 bundle 的 id
    relative_path TEXT NOT NULL, -- 该 asset 在 bundle 中的相对路径，注意，一个 asset 可能因为版本修改，导致其 relative_path 发生变动，但是 asset_id 应当保持不变
    content_hash INTEGER NOT NULL, -- 该 asset 的内容 hash，应该由 Asset::hash() 提供
    revision INTEGER NOT NULL, -- 该 asset 的版本号，每当 asset 的内容发生变更时，revision 应当增加
    in_memory BOOLEAN NOT NULL, -- 该 asset 的变动是否仅存在于内存中，如果是，则该 asset 会在下次启动时被删除

    UNIQUE KEY (bundle_id, relative_path),
    FOREIGN KEY (bundle_id) REFERENCES bundles(bundle_id)
)

CREATE TABLE IF NOT EXISTS bundles (
    bundle_id TEXT PRIMARY KEY, -- 每一个 bundle 具有的唯一标识符
    name TEXT NOT NULL, -- bundle 的名称
)
```

```rust
pub struct AssetIndexDB {
    pool: SqlitePool,
}

impl AssetIndexDB {
```

```rust
    pub fn upsert_asset(&self, asset: &AssetMetadata) -> Result<(), AssetError>;
```

```sql
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
ON CONFLICT(bundle_id, relative_path) DO UPDATE SET
    asset_id = excluded.asset_id,
    ty = excluded.ty,
    content_hash = excluded.content_hash,
    revision = excluded.revision,
    in_memory = excluded.in_memory
```

```rust
    pub fn upsert_bundle(&self, bundle: &AssetBundleMetadata) -> Result<(), AssetError>;
```

```sql
INSERT INTO bundles (
    bundle_id,
    name
)
VALUES (?, ?)
ON CONFLICT(bundle_id) DO UPDATE SET
    name = excluded.name
```

```rust
    // 返回 (数据库中多出来的，数据库中缺失的)
    pub fn diff_assets(&self, bundle_id: BundleId, assets: impl IntoIterator<Item = AssetId>) -> Result<(HashSet<AssetId>, HashSet<AssetId>), AssetError> {
        let in_db: HashSet<AssetId> = ... ;
        for asset_id in assets {
            if in_db.remove(&asset_id).is_none() {
                unindexed.insert(asset_id);
            }
        }

        Ok((in_db, unindexed))
    }
```

```sql
SELECT asset_id FROM assets WHERE bundle_id = ?
```

```rust
    pub fn get_asset(&self, asset_id: &Uuid) -> Result<AssetMetadata, AssetError>;
```

```sql
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
```

```rust
    pub fn update_asset(&self, asset_id: &Uuid, new_path: &str, content_hash: u64) -> Result<u64, AssetError>;
```

```sql
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
    ? AS relative_path,
    ? AS content_hash,
    revision + 1 AS revision,
    true AS in_memory
FROM latest
RETURNING revision
```

```rust
    pub fn write_asset(&self, asset_id: &Uuid) -> Result<u64, AssetError>;
```

```sql
WITH latest AS (
    SELECT revision
    FROM assets
    WHERE asset_id = ?
    ORDER BY revision DESC
    LIMIT 1
)
UPDATE assets
SET in_memory = false
WHERE asset_id = ? AND revision = (SELECT revision FROM latest)
RETURNING revision
```

```rust
    pub fn revert_asset(&self, asset_id: &Uuid) -> Result<(), AssetError>;
```

```sql
DELETE FROM assets WHERE in_memory = true AND asset_id = ?
```

```rust
    pub fn revert_all(&self) -> Result<(), AssetError>;
```

```sql
DELETE FROM assets WHERE in_memory = true
```

```rust
    pub fn get_bundle(&self, bundle_id: &BundleId) -> Result<AssetBundleMetadata, AssetError>;
}
```

```sql
SELECT
    bundle_id,
    name
FROM bundles
WHERE bundle_id = ?
```
