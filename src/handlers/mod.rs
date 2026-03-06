//! API 请求处理器模块
//!
//! 包含所有 HTTP 请求端点的实现：
//! - `auth` - 认证相关（登录、登出、账号管理）
//! - `user` - 用户管理（CRUD 操作）
//! - `subscription` - 订阅合并和获取

pub mod auth;
pub mod subscription;
pub mod user;
