# Stage 10 SSRF And Trust Boundary

阶段十的目标是补齐阶段七安全收口里仍然遗留的两个边界问题：

- 登录限流不再默认信任任意代理头。
- 出站订阅抓取不再只校验 DNS 结果，而是强制连接到刚刚校验过的地址，缩小 DNS rebinding 窗口。

## 完成项

### 1. 显式代理信任边界

- `backend/src/config.rs` 新增 `TRUST_PROXY_HEADERS` 配置，默认值为 `false`。
- `backend/src/routes/auth.rs` 通过 `ConnectInfo<SocketAddr>` 获取真实对端地址。
- `backend/src/security.rs` 的登录限流 key 现在默认使用真实 peer IP；只有在 `TRUST_PROXY_HEADERS=true` 时，才会优先解析 `x-forwarded-for` / `x-real-ip`。
- 非法或不可解析的转发头值会被忽略，不再污染限流 key。

### 2. DNS 校验结果绑定到实际连接

- `backend/src/subscriptions.rs` 的 `DnsResolver` 缓存从 `Vec<IpAddr>` 升级为 `Vec<SocketAddr>`。
- URL 校验现在返回 `ValidatedFetchTarget`，其中同时携带 URL、host、已验证地址列表，以及 host 是否为 IP literal。
- 对 hostname 请求，`reqwest` 会通过 `resolve_to_addrs` 把连接固定到刚刚校验过的地址集合；不会在真正发请求时再次走一遍未经约束的 DNS。
- 对 IP literal 请求，沿用共享 client 直接发送，避免不必要的额外 client 构建。

### 3. 跳转链继续保留 SSRF 防护

- `Location` 跳转目标仍然会走完整的 URL 校验。
- 新目标会重新解析、重新判定 forbidden IP，并带着新的已验证地址继续请求。
- 这样可以避免首跳安全、后跳 rebinding 或跳转到内网地址的情况。

### 4. 测试覆盖

阶段十新增测试覆盖：

- 未信任代理头时，伪造 `x-forwarded-for` 无法绕过登录锁定。
- 显式信任代理头后，限流会按转发 IP 分桶。
- hostname 请求会实际命中手工注入的已验证地址，证明 DNS override 生效。

## 关键文件

- `backend/src/config.rs`
- `backend/src/security.rs`
- `backend/src/routes/auth.rs`
- `backend/src/main.rs`
- `backend/src/subscriptions.rs`
- `backend/src/cache.rs`
- `backend/tests/http_flow.rs`
- `packages/core/src/lib.rs`
- `README.md`

## 验证

本地应通过：

```bash
cargo test -p submora
cargo check --workspace
```

阶段十完成后，登录入口和订阅抓取的信任边界都变成显式配置与显式连接约束：

- ingress 默认信任真实 peer，不信任任意转发头
- egress 默认连接已验证地址，不信任二次 DNS 解析
