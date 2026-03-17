# Stage 2 Scaffold

## 目标

阶段二负责把新架构骨架真正搭起来，但不迁移旧业务逻辑。

本阶段交付：

- Cargo workspace。
- `backend` Axum 骨架。
- `frontend` Dioxus 0.7.3 骨架。
- `packages/shared` 共享 DTO。
- `packages/core` 共享基础常量与校验逻辑。

## 目录结构

```text
backend/      # 新 Axum 服务骨架
frontend/     # 新 Dioxus 前端骨架
packages/
  shared/     # 前后端共享协议
  core/       # 共享常量、校验和纯逻辑
assets/       # Dioxus Web 静态资源
docs/rewrite/ # 阶段文档
```

## 当前状态

### 新后端

- 暴露了未来需要兼容的路由占位。
- `GET /healthz` 已可用。
- `GET /api/meta/app` 已可用。
- `/api/auth/*` 与 `/api/users*` 已保留为新后端路由表的一部分。
- `GET /{username}` 已保留为兼容路径，但当前仍是占位实现。

### 新前端

- 已建立 Dioxus 0.7.3 Web 应用入口。
- 已建立 Router 和基础页面骨架。
- 已建立共享应用壳层和基础样式。
- 页面目前以架构占位和兼容说明为主，还未接真实 API。

## 开发命令

### 检查新后端

```bash
cargo check -p submora-server
```

### 检查共享 crate

```bash
cargo check -p submora-core -p submora-shared
```

### 运行新后端

```bash
cargo run -p submora-server
```

### 启动新前端

```bash
dx serve
```

`Dioxus.toml` 已指向 `submora-web` 子包。

## 阶段边界

阶段二不包含：

- 旧数据迁移。
- 真实登录流程。
- 真实用户和链接 CRUD。
- 真实聚合抓取。
- 真实静态资源托管整合。

这些都属于后续实现阶段。
