# Stage 7 Security And Observability

阶段七在阶段六完成统一运行时和持久会话之后，继续补齐两个缺口：

1. 管理面缺少针对浏览器环境的主动防护。
2. 公共抓取链路缺少可回溯的诊断数据。

本阶段目标不是再做一次架构迁移，而是在当前 Dioxus + Axum + SQLite runtime 上补完安全边界和可观测性闭环。

## 完成项

### 1. CSRF 防护接入管理端

- 新增 `GET /api/auth/csrf`，基于 session 下发并缓存 CSRF token。
- 所有管理写接口都要求 `x-csrf-token`：
  - `POST /api/auth/login`
  - `POST /api/auth/logout`
  - `PUT /api/auth/account`
  - `POST /api/users`
  - `DELETE /api/users/{username}`
  - `PUT /api/users/{username}/links`
  - `PUT /api/users/order`
- Dioxus 前端统一在 `frontend/src/api.rs` 中自动拉取、缓存并在 403 后重取 token。

### 2. 登录限流

- `backend/src/security.rs` 增加内存限流器。
- 限流 key 由 `x-forwarded-for` / `x-real-ip` 与用户名拼接生成。
- 支持三个参数：
  - `LOGIN_MAX_ATTEMPTS`
  - `LOGIN_WINDOW_SECS`
  - `LOGIN_LOCKOUT_SECS`
- 连续失败超过阈值后返回 `429 Too Many Requests`。

### 3. sqlx migrations 替换手写 schema

- `backend/src/db.rs` 不再维护内嵌 schema 初始化逻辑。
- 改为 `sqlx::migrate!()` 加 `backend/migrations/0001_initial.sql`。
- 当前迁移创建三张表：
  - `admins`
  - `users`
  - `fetch_diagnostics`

### 4. 公共抓取诊断

- 公共聚合入口改为 `fetch_and_merge_for_user(...)`。
- 每条源链接都会记录一条诊断：
  - `status`
  - `detail`
  - `http_status`
  - `content_type`
  - `body_bytes`
  - `redirect_count`
  - `is_html`
  - `fetched_at`
- 诊断数据写入 SQLite 表 `fetch_diagnostics`。
- 管理 API 新增 `GET /api/users/{username}/diagnostics`。
- Dioxus 控制台新增诊断列表、手动刷新和主动触发公共路由按钮。

### 5. CI

- 新增 GitHub Actions workflow：
  - `cargo fmt --all -- --check`
  - `cargo check --workspace`
  - `cargo check -p submora-web --target wasm32-unknown-unknown`
  - `cargo test -p submora-core -p submora`

## 验证

本地已通过：

```bash
cargo fmt --all
cargo check --workspace
cargo test -p submora-core -p submora
```

阶段七新增集成测试覆盖：

- 写接口必须携带 CSRF
- 登录失败达到阈值后被限流
- 诊断接口在抓取前返回 `pending`
- 公共聚合执行后可查询到写入的诊断数据
- 同一 SQLite 数据库上的 session 可跨 app rebuild 保留
