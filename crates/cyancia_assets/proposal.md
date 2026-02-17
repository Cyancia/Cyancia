# cyancia_assets Proposal (Bundle-Scoped Index)

## 目标

为 `cyancia_assets` 设计一套可渐进演化的资产系统，核心约束如下：

- URL 统一为 `bundle_id:path`（不再暴露 `_memory` / `_local` 全局 source）
- 每个 bundle 在内部自行管理 `zip/base + local + memory` 三层覆盖
- 数据库仅维护“索引元数据”，不承担 latest 解析与分层策略
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

## 2. 存储层设计（Bundle 内部分层 + DB 索引）

### 2.1 Bundle 内部分层（由 bundle 自管）

每个 bundle 内部维护自己的覆盖链（例如）：

1. `memory`（运行期覆盖）
2. `local`（用户落盘覆盖）
3. `zip/base`（bundle 基础内容）

> 该覆盖策略只在 **单个 bundle 内** 生效，不跨 bundle。

### 2.2 数据库职责边界

数据库只存索引元数据：

- `asset_id`：稳定唯一标识
- `bundle_id`：所属 bundle
- `type`：资产类型
- `relative_path`：bundle 内相对路径
- `content_hash`：索引侧内容摘要
- `updated_at`：索引更新时间

数据库不负责：

- bundle 内部的 latest 选择
- memory/local/base 三层合并
- payload 读写

### 2.3 最小表结构

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
    content_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY(bundle_id) REFERENCES bundles(bundle_id),
    UNIQUE(bundle_id, relative_path)
);

-- 说明：asset_id 已是 PRIMARY KEY（天然唯一），无需再额外声明 UNIQUE(asset_id)

CREATE INDEX idx_assets_bundle_path ON assets(bundle_id, relative_path);
CREATE INDEX idx_assets_type ON assets(type);
CREATE INDEX idx_assets_bundle ON assets(bundle_id);
```

---

## 3. `ctt` 格式约定

`ctt` 本质是 zip 容器，建议最小结构：

```text
sand.ctt
 ├─ manifest.json
 └─ payload/
    └─ image.png
```

`manifest.json` 建议字段：

```json
{
  "format": "ctt",
  "version": 1,
  "asset_id": "...",
  "cached_logical_path": "textures/sand.ctt",
  "payload_entry": "payload/image.png",
  "content_hash": "sha256:...",
  "created_at": "...",
  "updated_at": "..."
}
```

> `asset_id` 必须稳定，不应随机生成。

---

## 4. API 草案（最小可落地）

### 4.1 Bundle 抽象（单 bundle 作用域）

```rust
pub trait AssetSource: Send + Sync {
    fn id(&self) -> BundleId;
    fn effective_hash(&self) -> String;
    fn all_assets(&self) -> Vec<AssetIndexRow>;

    fn handle_by_id<T>(&self, asset_id: &UntypedId) -> Result<Handle<T>, AssetError>;
    fn handle_by_path<T>(&self, path: &RelativeAssetPath) -> Result<Handle<T>, AssetError>;

    fn read_by_id<T>(&self, asset_id: &UntypedId) -> Result<Arc<T>, AssetError>;
    fn read_by_path<T>(&self, path: &RelativeAssetPath) -> Result<Arc<T>, AssetError>;

    fn update_by_id<T>(&self, asset_id: &UntypedId, new_data: T) -> Result<(), AssetError>;
    fn update_by_path<T>(&self, path: &RelativeAssetPath, new_data: T) -> Result<(), AssetError>;

    fn write_by_id(&self, asset_id: &UntypedId) -> Result<(), AssetError>;
    fn write_by_path(&self, path: &RelativeAssetPath) -> Result<(), AssetError>;

    fn revert_by_id(&self, asset_id: &UntypedId) -> Result<(), AssetError>;
    fn revert_by_path(&self, path: &RelativeAssetPath) -> Result<(), AssetError>;
}
```

约束：

- 分层（memory/local/base）由该 bundle 内部自行处理
- 对外仅暴露 bundle 作用域接口，不跨 bundle 混查

### 4.2 Resolver + Handle（跨 bundle 路由）

```rust
pub struct AssetResolver {
    bundles: HashMap<BundleId, Arc<dyn AssetSource>>,
}

impl AssetResolver {
    pub fn handle<T: Asset>(&self, url: AssetUrl) -> Result<AssetHandle<T>, AssetError>;
    pub fn handles_many<T: Asset>(&self, urls: &[AssetUrl]) -> Result<Vec<AssetHandle<T>>, AssetError>;
}

pub struct AssetHandle<T: Asset> {
    asset_id: UntypedId,
    url: AssetUrl,
    bundle: Arc<dyn AssetSource>,
    _marker: PhantomData<T>,
}

impl<T: Asset> AssetHandle<T> {
    pub fn read(&self) -> Result<Arc<T>, AssetError>;
    pub fn update(&mut self, new_data: T) -> Result<(), AssetError>;
    pub fn write(&mut self) -> Result<(), AssetError>;
    pub fn revert(&mut self) -> Result<(), AssetError>;
}
```

### 4.3 Index / Store（仅元数据）

```rust
pub struct AssetIndexRow {
    pub asset_id: UntypedId,
    pub bundle_id: BundleId,
    pub asset_type: String,
    pub relative_path: RelativeAssetPath,
    pub content_hash: String,
    pub updated_at: OffsetDateTime,
}

pub struct AssetIndexDb {
    // 例如: rusqlite::Connection / sqlx::SqlitePool
}

impl AssetIndexDb {
    pub fn upsert(&self, row: &AssetIndexRow) -> Result<(), AssetError>;
    pub fn get_by_id(&self, asset_id: &UntypedId) -> Result<Option<AssetIndexRow>, AssetError>;
    pub fn get_by_url(&self, url: &AssetUrl) -> Result<Option<AssetIndexRow>, AssetError>;
    pub fn list_by_type(&self, asset_type: &str) -> Result<Vec<AssetIndexRow>, AssetError>;
    pub fn update_by_id(&self, asset_id: &UntypedId, content_hash: &str) -> Result<AssetIndexRow, AssetError>;
    pub fn update_by_url(&self, url: &AssetUrl, content_hash: &str) -> Result<AssetIndexRow, AssetError>;
    pub fn write_by_id(&self, asset_id: &UntypedId) -> Result<AssetIndexRow, AssetError>;
    pub fn write_by_url(&self, url: &AssetUrl) -> Result<AssetIndexRow, AssetError>;
}
```

#### SQL 草案（AssetIndexDb）

参数约定：

- `BundleId` 绑定为其 UUID 文本
- `get_by_url/update_by_url/write_by_url` 输入 URL 必须符合 `bundle_id:path`

### 4.3.1 Bundle 变更检测基线

为避免仅依赖文件 `mtime` 带来的漏检，bundle 变更建议以 **effective hash** 为准：

- `base_hash`：zip/base 层摘要
- `local_hash`：local 覆盖层摘要
- `memory_hash`：memory 覆盖层摘要
- `effective_hash = H(base_hash, local_hash, memory_hash)`

推荐流程：

1. bundle 内部发生 `update/write/revert` 时立即重算 `effective_hash` 并广播事件
2. runtime 收到事件后刷新该 bundle 对应索引行（`updated_at` / `content_hash`）
3. 后台周期性 reconciliation（例如启动后、窗口激活后、固定间隔）兜底修正漏事件

##### upsert

```sql
INSERT INTO assets (
    asset_id,
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at
)
VALUES (?, ?, ?, ?, ?, ?)
ON CONFLICT(asset_id) DO UPDATE SET
    bundle_id = excluded.bundle_id,
    type = excluded.type,
    relative_path = excluded.relative_path,
    content_hash = excluded.content_hash,
    updated_at = excluded.updated_at;
```

##### get_by_id

```sql
SELECT
    asset_id,
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at
FROM assets
WHERE asset_id = ?
LIMIT 1;
```

##### get_by_url

```sql
SELECT
    asset_id,
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at
FROM assets
WHERE bundle_id = ?
  AND relative_path = ?
LIMIT 1;
```

##### list_by_type

```sql
SELECT
    asset_id,
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at
FROM assets
WHERE type = ?
ORDER BY relative_path ASC;
```

##### update_by_id

```sql
UPDATE assets
SET
    content_hash = ?,
    updated_at = ?
WHERE asset_id = ?
RETURNING
    asset_id,
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at;
```

##### update_by_url

```sql
UPDATE assets
SET
    content_hash = ?,
    updated_at = ?
WHERE bundle_id = ?
  AND relative_path = ?
RETURNING
    asset_id,
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at;
```

##### write_by_id

```sql
UPDATE assets
SET
    updated_at = ?
WHERE asset_id = ?
RETURNING
    asset_id,
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at;
```

##### write_by_url

```sql
UPDATE assets
SET
    updated_at = ?
WHERE bundle_id = ?
  AND relative_path = ?
RETURNING
    asset_id,
    bundle_id,
    type,
    relative_path,
    content_hash,
    updated_at;
```

### 4.4 Runtime

```rust
pub struct AssetRuntime {
    pub resolver: AssetResolver,
    pub index: Arc<AssetIndexDb>,
}

impl AssetRuntime {
    pub fn handle_by_id<T: Asset>(&self, asset_id: &UntypedId) -> Result<AssetHandle<T>, AssetError>;
    pub fn handle_by_url<T: Asset>(&self, url: AssetUrl) -> Result<AssetHandle<T>, AssetError>;
}
```

---

## 5. 多窗口一致性模型

### 5.1 共享服务注入

```rust
pub struct AppServices {
    pub assets: Arc<RwLock<AssetRuntime>>,
}
```

### 5.2 事件通知

```rust
pub enum AssetEvent {
    Updated { original: AssetUrl, updated: AssetUrl },
    Saved { original: AssetUrl, updated: AssetUrl },
    Reverted { original: AssetUrl, updated: AssetUrl },
    PathDrift {
        original: AssetUrl,
        updated: AssetUrl,
        cached_logical_path: RelativeAssetPath,
    },
}
```

---

## 6. 渐进实施计划

### Phase 1

- 固定 URL 语义：`bundle_id:path`
- 落地 `bundles/assets` 最小表结构
- 接入 `AssetIndexDb::{upsert,get_by_id,get_by_url,list_by_type}`

### Phase 2

- `AssetResolver` 接通 bundle 路由
- `AssetHandle::{read,update,write,revert}` 与 bundle 内部实现打通
- 多窗口事件广播与增量刷新

### Phase 3

- 完善导入/冲突策略（`asset_id` 冲突处理）
- 增强诊断（hash 漂移、路径漂移、bundle 变化扫描）

### Phase 4

- 按需扩展索引字段（作者、标签、统计等）
- 保持 URL 与 runtime API 稳定
