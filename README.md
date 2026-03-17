# Submora

GitHub: https://github.com/vansour/Submora

`Submora` 是一个面向多用户订阅聚合场景的 Rust 项目。仓库现已完成重写阶段十一：前端使用 Dioxus `0.7.3`，后端使用 Axum `0.8.8`，统一运行时在阶段十显式代理信任边界与 SSRF DNS rebinding 防护的基础上，继续补齐边缘层安全响应头与公共聚合入口限流。

## 当前架构

- 前端：`frontend`
- 后端：`backend`
- 共享协议：`packages/shared`
- 共享校验与元信息：`packages/core`
- 静态资源：`assets/`
- 构建配置：`Dioxus.toml`
- 阶段文档：`docs/rewrite/`

目录树说明：

```text
.
├── frontend/          # Dioxus 管理台
├── backend/           # Axum 服务、公共聚合路由、管理 API
├── packages/
│   ├── core/          # 共享校验、元信息、纯 Rust 公共逻辑
│   └── shared/        # 前后端共享请求/响应模型
├── assets/            # 管理台静态样式等资源
├── docs/rewrite/      # 重写阶段记录
├── .github/workflows/ # CI / reviewdog / preview / release
└── Makefile           # 常用开发命令入口
```

`packages/` 这里是 Rust 共享包目录，不是前端包管理器目录。

## 阶段十一完成内容

- 默认响应现在会统一附带安全响应头，包括 `X-Content-Type-Options`、`X-Frame-Options`、`Referrer-Policy` 和 `Permissions-Policy`。
- `GET /{username}` 公共聚合入口新增独立内存限流，与登录限流分开治理。
- 公共限流继续沿用显式代理信任边界；默认只使用真实对端地址，只有在 `TRUST_PROXY_HEADERS=true` 时才会信任转发头。
- 新增环境变量 `PUBLIC_MAX_REQUESTS` 和 `PUBLIC_WINDOW_SECS`，用于控制公共入口限流窗口。
- 阶段十一新增测试覆盖默认安全头存在，以及公共入口连续请求超过阈值后返回 `429`。

## 本地开发

前提：

- Rust stable
- `wasm32-unknown-unknown` target
- Dioxus CLI `0.7.3`

安装示例：

```bash
rustup target add wasm32-unknown-unknown
cargo install dioxus-cli --version 0.7.3
```

后端开发：

```bash
cargo run -p submora
```

前端开发：

```bash
dx serve --platform web --package submora-web
```

生产构建前端并让 Axum 托管：

```bash
dx build --platform web --package submora-web --release
cargo run -p submora
```

统一开发命令：

```bash
make check
make test
make clippy
make clippy-wasm
make e2e
make serve
make build
```

默认管理员账号：

- 用户名：`admin`
- 密码：`admin`

## 分支与发布

- 长期分支只保留 `main`。
- 功能开发建议从 `main` 切 `feat/*`、`fix/*`、`hotfix/*` 分支，再通过 PR 合并。
- PR 预览不走预览服务器，只会构建并推送 preview 镜像到 GHCR。
- 正式发布通过 tag 触发，支持：
  - `vMAJOR.MINOR.PATCH`
  - `vMAJOR.MINOR.PATCH-rc.N`
  - `vMAJOR.MINOR.PATCH-beta.N`

## GitHub Actions

当前仓库内置 4 组工作流：

- `CI`
  - 对 `main` 的 `push` 和 `pull_request` 运行。
  - 执行 `fmt`、`check`、`clippy`、Rust tests 和 Playwright E2E。
- `reviewdog`
  - 对 `main` 的 PR 运行。
  - 为 `rustfmt`、`clippy`、`clippy-wasm` 提供 PR 注释/检查反馈。
- `preview`
  - 对 `main` 的 PR 运行。
  - 对同仓、非 draft PR 构建并推送 preview 镜像到 GHCR，并在 PR 中写回镜像 tag。
  - 当前只发布 preview 镜像，不做环境部署。
- `release`
  - 在推送 `v*` tag 时运行。
  - 校验 tag commit 来自 `main`，重新执行发布级校验，推送 GHCR 镜像，并创建 GitHub Release。

## 容器部署

`Dockerfile` 会在构建阶段同时生成：

- `dist/` 的 Dioxus Web 产物
- `submora` 二进制

启动：

```bash
docker compose up -d --build
```

默认对外：

- 管理台：`http://127.0.0.1:8080`
- 聚合路由：`http://127.0.0.1:8080/{username}`

当前 [compose.yml](/root/github/Submora/compose.yml) 只保留了镜像、端口、数据卷、重启策略和日志配置；运行时环境变量默认使用：

- `Dockerfile` 中的镜像默认值
- `backend/src/config.rs` 内置默认值

如果你要覆盖这些默认值，直接在本地 `compose.yml` 增加 `environment:`，或通过 `docker compose --env-file ...` 提供环境变量即可。

## 常用环境变量

- `HOST` / `PORT`
- `WEB_DIST_DIR`
- `DATABASE_URL`
- `COOKIE_SECURE`
- `SESSION_TTL_MINUTES`
- `SESSION_CLEANUP_INTERVAL_SECS`
- `TRUST_PROXY_HEADERS`
- `LOGIN_MAX_ATTEMPTS`
- `LOGIN_WINDOW_SECS`
- `LOGIN_LOCKOUT_SECS`
- `PUBLIC_MAX_REQUESTS`
- `PUBLIC_WINDOW_SECS`
- `CACHE_TTL_SECS`
- `DB_MAX_CONNECTIONS`
- `FETCH_TIMEOUT_SECS`
- `DNS_CACHE_TTL_SECS`
- `FETCH_HOST_OVERRIDES`
- `CONCURRENT_LIMIT`
- `MAX_LINKS_PER_USER`
- `MAX_USERS`
- `ADMIN_USER`
- `ADMIN_PASSWORD`
- `CORS_ALLOW_ORIGIN`

其中很多变量都有安全默认值；`compose.yml` 默认不会显式覆盖它们。

## 关键接口

- `GET /api/auth/csrf`
- `POST /api/auth/login`
- `POST /api/auth/logout`
- `PUT /api/auth/account`
- `GET /api/users/{username}/diagnostics`
- `GET /api/users/{username}/cache`
- `POST /api/users/{username}/cache/refresh`
- `DELETE /api/users/{username}/cache`
- `GET /{username}`

## 校验命令

```bash
make check
make test
make clippy
make clippy-wasm
make e2e
```

对应展开后为：

```bash
cargo fmt --all -- --check
cargo check --workspace
cargo check -p submora-web --target wasm32-unknown-unknown
cargo test -p submora-core -p submora
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy -p submora-web --target wasm32-unknown-unknown -- -D warnings
npm run e2e
```

## 说明

- 公共聚合路由仍是 `GET /{username}`，返回 `text/plain`。
- 管理会话与 merged cache snapshot 都保存在 SQLite 中，可跨服务重启保留。
- 写接口继续沿用阶段七启用的 CSRF 校验。
- 过期 snapshot 现在会先返回旧值并在后台刷新，响应 header 的 `x-substore-cache` 可能出现 `hit`、`miss`、`stale` 和 `empty`。
- `FETCH_HOST_OVERRIDES` 可用于显式静态解析上游 host，主要用于测试和内网联调；默认留空，不会改变公网抓取边界。
- 当前 Rust 集成测试和 Playwright E2E 都使用本地 upstream fixture，不依赖公网可达性。
- 阶段记录见 `docs/rewrite/stage-1-baseline.md` 到 `docs/rewrite/stage-11-edge-hardening.md`。
