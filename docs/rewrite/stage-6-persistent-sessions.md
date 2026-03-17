# Stage 6 Persistent Sessions

## 目标

阶段六解决阶段四遗留的最后一个明显运行时短板：

- 把管理会话从内存存储切换到数据库持久存储。
- 让服务重启后仍能识别已有登录态。
- 为过期会话增加持续清理任务。
- 把新的 session 配置暴露给本地开发和容器部署。

## 本阶段完成

### Session 存储切换

- `backend/src/app.rs` 不再直接创建 `MemoryStore`。
- 新增 `backend/src/session.rs`，统一负责：
  - 初始化 SQLite session store
  - 迁移 session 表
  - 构建 `SessionManagerLayer`
  - 启动过期会话清理任务
- 主进程现在在启动阶段初始化应用数据表和 session 表，然后再绑定 HTTP 服务。

### 运行配置

- `ServerConfig` 新增：
  - `SESSION_TTL_MINUTES`
  - `SESSION_CLEANUP_INTERVAL_SECS`
- 默认会话有效期为 `7` 天。
- 默认清理周期为 `300` 秒。

### 生命周期管理

- `main.rs` 为 Axum 服务接入了优雅退出。
- 收到 `SIGINT` / `SIGTERM` 时会停止服务并中止 session 清理任务。

### 测试

- 原有集成测试继续覆盖登录、用户管理、排序和公共聚合。
- 新增“同一数据库上的 session 跨 app rebuild 仍有效”测试，直接验证 SQLite 持久会话能力。

## 关键文件

- `backend/src/session.rs`
- `backend/src/config.rs`
- `backend/src/main.rs`
- `backend/src/app.rs`
- `backend/tests/http_flow.rs`
- `compose.yml`
- `README.md`

## 验证结果

本阶段完成后应通过：

```bash
cargo check --workspace
cargo test -p submora-core -p submora
```

## 当前结果

- 新运行时已经具备持久登录态，不再依赖进程内内存 session。
- 阶段五的仓库切换结果保留不变。
- 阶段六主要完成的是运行时状态持久化，而不是新的功能面扩张。
