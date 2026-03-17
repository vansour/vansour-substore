# Stage 3 Implementation

## 目标

阶段三把阶段二的骨架推进到“第一版可用系统”：

- 新 Axum 服务从占位实现升级为真实认证、用户管理和公共聚合服务。
- 新 Dioxus 前端从页面壳升级为可操作的管理控制台。
- `shared` 与 `core` crate 开始承载真实共享协议和校验逻辑。

## 本阶段已完成

### 新后端

新后端位于 `backend`，已经具备以下能力：

- SQLite 数据库连接与自动建表。
- 默认管理员自动初始化。
- Cookie Session 登录、登出、查询当前用户、更新账号。
- 用户列表、创建用户、删除用户。
- 读取与保存用户链接列表。
- 用户顺序更新。
- `GET /{username}` 真实聚合输出。
- `/assets` 静态目录托管。
- 当 `dist/index.html` 存在时，`/`、`/login`、`/account`、`/users/{username}` 可直接回退到前端构建产物。
- 为 Dioxus 开发态预留了跨端口 CORS。

### 新前端

新前端位于 `frontend`，已经具备以下能力：

- Dioxus 0.7.3 Router 路由结构。
- wasm API 客户端。
- 登录。
- 当前会话查询。
- 用户列表查看。
- 创建用户。
- 用户选择与链接文本编辑。
- 保存链接。
- 删除用户。
- 基本排序（Up/Down）。
- 管理员账号更新。

## 关键实现文件

- `backend/src/main.rs`
- `backend/src/app.rs`
- `backend/src/routes/auth.rs`
- `backend/src/routes/users.rs`
- `backend/src/routes/public.rs`
- `backend/src/db.rs`
- `backend/src/subscriptions.rs`
- `frontend/src/api.rs`
- `frontend/src/app.rs`
- `frontend/src/components/console.rs`
- `packages/shared/src/lib.rs`
- `packages/core/src/lib.rs`

## 当前约束

阶段三仍然保留以下限制：

- Session 仍是内存存储，不跨服务实例共享。
- 聚合抓取逻辑仍是从旧实现迁移过来的基础版本，尚未完成更严格的流式体积限制与重定向复检。
- Dioxus 前端虽然已经可用，但样式和交互仍是最小实现，不是最终产品形态。
- 前端构建产物默认不会自动生成到 `dist/`，需要显式执行 Dioxus 构建。

## 验证结果

本阶段完成后，以下命令已通过：

```bash
cargo check --workspace
```

## 下一阶段建议

阶段四建议优先处理：

- 把 Dioxus 构建产物正式接入 Axum 的默认运行流程。
- 强化聚合抓取的 SSRF / 重定向 / 响应体限制。
- 为前端补充更完整的交互和视觉设计。
- 增加端到端测试与迁移测试。
