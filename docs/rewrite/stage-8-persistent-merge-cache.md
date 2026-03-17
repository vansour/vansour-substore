# Stage 8 Persistent Merge Cache

阶段八的目标是解决阶段七之后最明显的运行时成本问题：

1. 公共路由每次请求都会同步抓取全部上游源。
2. 管理台虽然能看到诊断，但无法直接管理聚合结果本身的缓存生命周期。

因此本阶段没有继续扩张协议面，而是给当前 Dioxus + Axum + SQLite runtime 补上持久 merged cache。

## 完成项

### 1. SQLite 持久缓存

- 新增 migration：`backend/migrations/0002_user_cache_snapshots.sql`
- 新增表：`user_cache_snapshots`
- 每条缓存快照记录：
  - `content`
  - `line_count`
  - `body_bytes`
  - `generated_at`
  - `expires_at`

缓存内容和元信息都保存在 SQLite，因此服务重启后仍可直接命中。

### 2. 公共路由优先命中缓存

- `GET /{username}` 现在先检查缓存快照。
- 如果 snapshot 仍在 TTL 内，则直接返回缓存内容。
- 如果没有可用快照，则重新抓取、重新合并，并把新结果写回 SQLite。
- 响应会附带缓存标记头：
  - `x-substore-cache`
  - `x-substore-generated-at`
  - `x-substore-expires-at`

### 3. 管理端缓存接口

新增三个接口：

- `GET /api/users/{username}/cache`
- `POST /api/users/{username}/cache/refresh`
- `DELETE /api/users/{username}/cache`

其中：

- `GET` 用于查看当前 snapshot 状态和元信息。
- `POST refresh` 会主动执行一次抓取和合并，然后刷新缓存。
- `DELETE` 用于手动清理缓存。

### 4. 失效策略

缓存不会无限期存在，当前策略分成两层：

1. TTL 失效
   - 由 `CACHE_TTL_SECS` 控制。
   - snapshot 到期后，下次重建会覆盖旧内容。

2. 配置变更主动失效
   - `PUT /api/users/{username}/links` 会清理该用户缓存。
   - `DELETE /api/users/{username}` 会连同缓存一起清理。

这样既能减少重复抓取，又不会在源列表变更后继续返回旧聚合结果。

### 5. 管理台缓存面板

阶段八在 Dioxus 管理台新增 `Merged Cache` 面板，支持：

- 查看缓存状态：`fresh` / `expired` / `empty`
- 查看行数、字节数、生成时间、过期时间
- 手动刷新缓存
- 手动清理缓存
- 继续打开公共路由查看真实输出

### 6. 测试覆盖

阶段八新增和扩展的集成测试覆盖：

- 首次访问公共路由返回 `miss`
- 同一用户再次访问命中 `hit`
- 管理端缓存刷新接口可以主动建立 snapshot
- 手动清理缓存后状态回到 `empty`
- 更新链接后原有 snapshot 会被失效

## 验证

本地已通过：

```bash
cargo fmt --all
cargo check --workspace
cargo check -p submora-web --target wasm32-unknown-unknown
cargo test -p submora-core -p submora
```
