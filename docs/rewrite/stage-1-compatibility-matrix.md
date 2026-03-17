# Stage 1 Compatibility Matrix

## 使用说明

本清单用于后续重写开发、联调和切换验收。  
状态含义：

- `Must Keep`: 必须保持兼容。
- `Can Improve`: 可以优化实现，但外部语义不能破坏。
- `Do Not Preserve`: 不要求继承旧缺陷。

## 路由与接口

| 项目 | 当前行为 | 状态 | 备注 |
| --- | --- | --- | --- |
| `GET /` | 返回管理后台入口 HTML | Must Keep | 新前端可改为 Dioxus 构建产物 |
| `GET /healthz` | 返回 `200` + `ok` | Must Keep | 用于容器健康检查 |
| `POST /api/auth/login` | 登录并写入 Session Cookie | Must Keep | 继续使用 Cookie 会话 |
| `POST /api/auth/logout` | 清空当前 Session | Must Keep | 路径保持不变 |
| `GET /api/auth/me` | 返回当前登录用户 | Must Keep | 未登录返回 `401` |
| `PUT /api/auth/account` | 更新管理员账号 | Must Keep | 可加强校验 |
| `GET /api/users` | 返回按排序后的用户列表 | Must Keep | 返回结构可继续保持轻量 |
| `POST /api/users` | 创建用户 | Must Keep | 需真正执行上限约束 |
| `DELETE /api/users/{username}` | 删除用户 | Must Keep | 首版保留基于用户名的外部语义 |
| `GET /api/users/{username}/links` | 获取链接列表 | Must Keep | 后端内部可改为 `id + source` 结构 |
| `PUT /api/users/{username}/links` | 整体覆盖链接列表 | Must Keep | 可在内部拆成规范化存储 |
| `PUT /api/users/order` | 保存用户顺序 | Must Keep | 需要更严格的输入校验 |
| `GET /{username}` | 返回聚合纯文本 | Must Keep | 对客户端是最关键兼容面 |

## 会话与安全

| 项目 | 当前行为 | 状态 | 备注 |
| --- | --- | --- | --- |
| Session Cookie | 服务端设置，前端 `credentials: include` | Must Keep | 可更换实现，不换交互模式 |
| 管理接口未登录 | 返回 `401` JSON 错误 | Must Keep | 前端据此回到登录页 |
| 安全响应头 | 默认开启 | Can Improve | 可以更系统化配置 |
| 限流 | 全局内存限流 | Can Improve | 重写后可拆为登录/公共接口双策略 |
| SSRF 防护 | 初始 URL 解析校验 | Do Not Preserve | 新实现必须更严格 |

## 前端交互

| 项目 | 当前行为 | 状态 | 备注 |
| --- | --- | --- | --- |
| 登录页 | 用户名/密码登录 | Must Keep | UI 可重做 |
| 用户列表 | 查看、编辑、删除、复制、打开 | Must Keep | 信息布局可调整 |
| 链接编辑 | 列表视图和文本视图 | Must Keep | 首版允许先做等价体验 |
| 用户排序 | 支持排序保存 | Must Keep | 交互可从拖拽换成按钮式，但最终建议保留拖拽 |
| 账号设置 | 修改用户名和密码 | Must Keep | 应补强校验 |
| Toast/错误提示 | 有全局反馈 | Can Improve | 反馈样式不要求兼容 |

## 数据与部署

| 项目 | 当前行为 | 状态 | 备注 |
| --- | --- | --- | --- |
| SQLite | 默认本地文件数据库 | Must Keep | 首版仍用 SQLite |
| 默认端口 | `8080` | Must Keep | 切换成本最低 |
| 健康检查 | `/healthz` | Must Keep | Docker 依赖 |
| Docker 单容器部署 | 后端同时托管页面和 API | Must Keep | 生产入口保持单服务 |
| 环境变量 | `HOST` `PORT` `COOKIE_SECURE` `LOG_FILE` `LOG_LEVEL` `DB_MAX_CONNECTIONS` `FETCH_TIMEOUT_SECS` `CONCURRENT_LIMIT` `MAX_LINKS_PER_USER` `MAX_USERS` | Can Improve | 首版建议兼容现有变量名 |

## 当前缺陷处理策略

| 缺陷 | 处理策略 |
| --- | --- |
| 链接顺序在保存时被去重逻辑打乱 | Do Not Preserve，重写时必须修复 |
| 前端错误响应处理不一致 | Do Not Preserve，统一错误契约 |
| 改密未强制验证当前密码 | Do Not Preserve，重写时加强 |
| 大响应体限制不严格 | Do Not Preserve，改为流式限制 |
| 重定向 SSRF 未复检 | Do Not Preserve，重写时修复 |

## 后续阶段输入

阶段二和阶段三的设计、开发、测试，都以本清单为最小兼容范围。  
任何打算变更 `Must Keep` 项目的提议，都必须先修改本文件并明确迁移方案。
