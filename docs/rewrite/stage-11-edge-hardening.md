# Stage 11 Edge Hardening

阶段十一的目标是把前一阶段已经补齐的 ingress/egress 信任边界继续推到边缘层：

- 默认输出统一带上安全响应头。
- 公共聚合入口 `GET /{username}` 拥有独立于登录接口的限流策略。

## 完成项

### 1. 默认安全响应头

- `backend/src/app.rs` 统一为响应补充默认安全头。
- 当前默认包含：
  - `X-Content-Type-Options: nosniff`
  - `X-Frame-Options: DENY`
  - `Referrer-Policy: no-referrer`
  - `Permissions-Policy: camera=(), microphone=(), geolocation=()`
- 这些头会覆盖管理台 HTML、API JSON、静态资源和公共文本路由。

### 2. 公共聚合入口独立限流

- `backend/src/security.rs` 新增 `PublicRateLimiter`。
- `backend/src/routes/public.rs` 在 `GET /{username}` 入口最前面执行限流。
- 该限流与登录限流彼此独立，避免公共流量直接挤占管理员登录窗口。
- 默认 key 使用 `client_ip + username`，既保留对单个公开 feed 的防护，也避免同一访客访问不同用户时立即互相影响。

### 3. 继续沿用显式代理信任边界

- 公共入口限流与登录限流共用同一套 client IP 判定逻辑。
- 当 `TRUST_PROXY_HEADERS=false` 时，只使用真实 peer 地址。
- 当 `TRUST_PROXY_HEADERS=true` 时，才会解析 `x-forwarded-for` / `x-real-ip`。

### 4. 配置项

- `PUBLIC_MAX_REQUESTS`
- `PUBLIC_WINDOW_SECS`

默认值分别为：

- `60`
- `60`

## 测试覆盖

阶段十一新增测试覆盖：

- 默认安全响应头存在。
- 公共入口超过阈值后返回 `429 too_many_requests`。

## 关键文件

- `backend/src/app.rs`
- `backend/src/config.rs`
- `backend/src/main.rs`
- `backend/src/routes/public.rs`
- `backend/src/security.rs`
- `backend/src/state.rs`
- `backend/tests/http_flow.rs`
- `packages/core/src/lib.rs`
- `README.md`

## 验证

本地应通过：

```bash
cargo test -p submora
cargo check --workspace
```

阶段十一完成后，服务边缘层的默认姿态变成：

- 管理和公共输出默认带安全头
- 登录与公共访问各自独立限流
- client IP 仍然遵守显式代理信任配置
