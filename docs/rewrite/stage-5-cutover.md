# Stage 5 Cutover

## 目标

阶段五结束重写迁移，把仓库从“双轨并存”推进到“单主线交付”：

- 删除旧版根目录实现和静态前端。
- 把最终运行包名统一为 `submora`。
- 把根 `Cargo.toml` 收口为纯 workspace 清单。
- 更新仓库入口文档和默认开发命令。

## 本阶段完成

### 仓库切换

- 根 `Cargo.toml` 已移除旧根包定义与遗留依赖，只保留 workspace 配置。
- `backend` 包名已从 `submora-server` 切换为 `submora`。
- 默认本地运行命令已变为：

```bash
cargo run -p submora
```

### 旧实现清理

以下遗留内容已物理删除：

- 根目录 `src/`
- 根目录 `web/`
- `docker-entrypoint.sh`

阶段五后，仓库不再保留旧运行时入口，避免新旧代码路径继续并存。

### 交付入口统一

- `Dockerfile` 现在直接构建并复制 `submora` 二进制。
- README、容器命令和校验命令已全部切换到新的最终包名。
- 当前阶段号已推进到 `5`。

## 关键文件

- `Cargo.toml`
- `backend/Cargo.toml`
- `backend/src/main.rs`
- `backend/src/app.rs`
- `frontend/src/components/console.rs`
- `Dockerfile`
- `README.md`

## 验证要求

本阶段完成后应继续通过：

```bash
cargo check --workspace
cargo test -p submora-core -p submora
```

## 当前结果

- 新架构已经成为仓库唯一主线。
- 阶段四的安全加固、UI 收口和测试保留不变。
- 阶段五主要完成的是包名、目录结构和仓库入口的最终切换。
