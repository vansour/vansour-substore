# Stage 9 Cache Consistency

阶段九的目标是收紧阶段八持久 merged cache 的一致性边界，避免后台刷新把旧配置重新写回公共输出，同时把抓取失败的细节留在诊断面而不是泄露到聚合结果里。

## 完成项

### 1. 用户配置版本化

- `backend/migrations/0003_config_versions.sql` 为 `users` 表增加 `config_version`。
- 每次 `PUT /api/users/{username}/links` 成功后，都会递增该用户的 `config_version`。
- `user_cache_snapshots` 新增 `source_config_version`，用于标记某条 snapshot 对应的是哪一版用户配置。

### 2. stale refresh 不再回写旧缓存

- `backend/src/cache.rs` 的 snapshot 写入改成“带版本条件的 upsert”。
- 只有当 `source_config_version` 仍然等于当前 `users.config_version` 时，新的 snapshot 才允许落库。
- 如果管理员在后台刷新期间保存了新链接，旧刷新结果会被直接丢弃，不会重新变成 `hit`。

### 3. 管理端缓存状态只看当前版本

- `GET /api/users/{username}/cache` 现在会按当前 `config_version` 过滤 snapshot。
- 旧版本 snapshot 即使仍留在表里，也不会继续显示为 `fresh` / `expired`。
- 公共路由 `GET /{username}` 也会拒绝使用版本不匹配的 snapshot。

### 4. 失败抓取不再进入公共输出

- `backend/src/subscriptions.rs` 里失败抓取不再生成 `<!-- error -->` 片段参与 merge。
- 公共输出现在只包含真正抓取成功的源内容。
- 失败、阻断、跳转等细节继续写入 `fetch_diagnostics`，留给管理端查看。

### 5. 测试覆盖

阶段九新增测试覆盖：

- 旧 `config_version` 的 snapshot 写入会被拒绝。
- 失败抓取不会再生成 merged output 片段。
- 公共路由在失败抓取场景下返回空文本，但诊断仍记录为 `blocked`。
- 管理端缓存状态会忽略旧版本 snapshot。

## 关键文件

- `backend/migrations/0003_config_versions.sql`
- `backend/src/cache.rs`
- `backend/src/routes/public.rs`
- `backend/src/routes/users.rs`
- `backend/src/subscriptions.rs`
- `backend/tests/http_flow.rs`
- `packages/core/src/lib.rs`

## 验证

本地已通过：

```bash
cargo test -p submora
```

阶段九完成后，公共聚合路由的 cache 与诊断职责进一步分离：

- cache 只服务当前配置版本
- diagnostics 继续保留失败细节
- merged output 不再夹带抓取错误注释
