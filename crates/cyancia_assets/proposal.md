# cyancia_assets Proposal (DB-Managed Revision)

## 目标

将 `cyancia_assets` 调整为“**数据库管理版本状态**”的模型，核心约束如下：

- URL 保持统一：`bundle_id:path`
- 资产当前状态由 DB 字段直接描述：`revision + physical_location`
- `physical_location` 仅表示“当前有效内容在哪一层”，不再由 runtime 隐式推导 latest
- 多窗口共享同一套 runtime/index，保证一致可见性

---

## 1. URL 规范

### 1.1 语法

```text
<bundle_id>:<relative_path>
```

- `bundle_id`：UUID（稳定标识一个 bundle）
- `relative_path`：
  - 使用 `/`
  - 不允许 `..`
  - 不允许绝对路径

### 1.2 示例

- `550e8400-e29b-41d4-a716-446655440000:textures/sand.ctt`
- `1dbf819c-4e8f-48b7-b1dc-34a8f8edb3b6:materials/rock.mat`

### 1.3 解析结果模型

```rust
pub struct AssetUrl {
    pub source: BundleId,
    pub path: RelativeAssetPath,
}

pub struct BundleId(pub Uuid);
```

---

## 2. 版本与位置模型（DB 为真相源）

### 2.1 核心字段

每个资产至少维护以下“当前态”字段：

- `revision`：整数版本号（单资产单调递增）
- `physical_location`：当前有效内容所在层

建议定义：

```rust
#[repr(i16)]
pub enum PhysicalLocation {
    Memory = 0,         // 运行期覆盖（未落盘）
    LocalModified = 1,  // 已落盘本地覆盖
    BundleBase = 2,     // 未修改，仍使用包体内内容
}
```

> 说明：你提的 `0/1/2` 方案可行，建议以 enum 包一层，避免魔法数字散落在业务代码里。

### 2.2 状态迁移约定

- `update`（内存改动）：`revision += 1`，`physical_location = 0`
- `write`（写入本地）：`revision` 不变，`physical_location = 1`
- `revert`（回退到包体）：`revision += 1`，`physical_location = 2`

可选策略（按需开启）：

- 若 `write` 过程中做了规范化变换（例如重编码），允许 `revision += 1`

---

## 3. 存储层职责

### 3.1 Runtime 职责

- 负责按 `physical_location` 读取真实内容（memory/local/bundle）
- 负责执行 update/write/revert 并推动 DB 状态迁移

### 3.2 DB 职责

- 维护资产当前态（`revision`、`physical_location`、`content_hash`、`updated_at`）
- 可选维护历史版本（审计/回溯）
- 提供查询与并发控制（CAS）

---

## 4. 最小表结构

```sql
CREATE TABLE bundles (
    bundle_id TEXT PRIMARY KEY,
    content_hash TEXT,
    filename TEXT
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
```

### 4.1 可选：历史版本表

若需要审计/回退，可增加：

```sql
CREATE TABLE asset_revisions (
    asset_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    physical_location INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(asset_id, revision),
    FOREIGN KEY(asset_id) REFERENCES assets(asset_id),
    CHECK (physical_location IN (0, 1, 2))
);
```

---

## 5. API 草案（最小可落地）

### 5.1 索引行模型

```rust
pub struct AssetIndexRow {
    pub asset_id: UntypedId,
    pub bundle_id: BundleId,
    pub asset_type: String,
    pub relative_path: RelativeAssetPath,

    pub revision: i64,
    pub physical_location: PhysicalLocation,

    pub content_hash: String,
    pub updated_at: OffsetDateTime,
}
```

### 5.2 Index DB

```rust
pub struct AssetIndexDb {
    // e.g. rusqlite::Connection / sqlx::SqlitePool
}

impl AssetIndexDb {
    pub fn upsert(&self, row: &AssetIndexRow) -> Result<(), AssetError>;

    pub fn get_by_id(&self, asset_id: &UntypedId) -> Result<Option<AssetIndexRow>, AssetError>;
    pub fn get_by_url(&self, url: &AssetUrl) -> Result<Option<AssetIndexRow>, AssetError>;
    pub fn list_by_type(&self, asset_type: &str) -> Result<Vec<AssetIndexRow>, AssetError>;

    pub fn update_state_by_id(
        &self,
        asset_id: &UntypedId,
        expected_revision: i64,
        next_revision: i64,
        next_location: PhysicalLocation,
        next_hash: &str,
    ) -> Result<AssetIndexRow, AssetError>;

    pub fn update_state_by_url(
        &self,
        url: &AssetUrl,
        expected_revision: i64,
        next_revision: i64,
        next_location: PhysicalLocation,
        next_hash: &str,
    ) -> Result<AssetIndexRow, AssetError>;
}
```

> `expected_revision` 用于 CAS，避免多窗口并发覆盖。

### 5.3 Resolver + Handle

```rust
pub struct AssetResolver {
    bundles: HashMap<BundleId, Arc<dyn AssetSource>>,
}

pub struct AssetHandle<T: Asset> {
    asset_id: UntypedId,
    url: AssetUrl,
    _marker: PhantomData<T>,
}

impl<T: Asset> AssetHandle<T> {
    pub fn read(&self) -> Result<Arc<T>, AssetError>;
    pub fn update(&mut self, new_data: T) -> Result<(), AssetError>; // -> location=0
    pub fn write(&mut self) -> Result<(), AssetError>;               // -> location=1
    pub fn revert(&mut self) -> Result<(), AssetError>;              // -> location=2
}
```

---

## 6. SQL 草案（状态更新）

### 6.1 get_by_url

```sql
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
```

### 6.2 CAS 更新（按 id）

```sql
UPDATE assets
SET
    revision = ?,
    physical_location = ?,
    content_hash = ?,
    updated_at = ?
WHERE asset_id = ?
  AND revision = ?
RETURNING
    asset_id,
    bundle_id,
    type,
    relative_path,
    revision,
    physical_location,
    content_hash,
    updated_at;
```

参数顺序建议：

1. `next_revision`
2. `next_location`
3. `next_hash`
4. `now`
5. `asset_id`
6. `expected_revision`

---

## 7. 多窗口一致性模型

### 7.1 共享服务注入

```rust
pub struct AppServices {
    pub assets: Arc<RwLock<AssetRuntime>>,
}
```

### 7.2 事件通知

```rust
pub enum AssetEvent {
    Updated {
        url: AssetUrl,
        revision: i64,
        physical_location: PhysicalLocation,
    },
    Saved {
        url: AssetUrl,
        revision: i64,
        physical_location: PhysicalLocation,
    },
    Reverted {
        url: AssetUrl,
        revision: i64,
        physical_location: PhysicalLocation,
    },
    Conflict {
        url: AssetUrl,
        expected_revision: i64,
        actual_revision: i64,
    },
}
```

---

## 8. 渐进实施计划

### Phase 1

- 固定 URL 语义：`bundle_id:path`
- 扩展 `assets` 表：`revision`、`physical_location`
- 接入 `get_by_id/get_by_url/list_by_type`

### Phase 2

- `update/write/revert` 全部改为 CAS 更新
- `AssetHandle` 与 DB 状态迁移打通
- 多窗口事件广播与冲突提示（`Conflict`）

### Phase 3

- 可选接入 `asset_revisions` 历史表
- 增强诊断（hash 漂移、路径漂移、bundle 扫描）

### Phase 4

- 按需扩展索引字段（作者、标签、统计等）
- 保持 URL 与 runtime API 稳定
