//! 用户管理处理器模块
//!
//! 处理用户的创建、删除、链接管理和排序。

use axum::{
    extract::{Path, State},
    response::Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tower_sessions::Session;
use url::Url;
use std::sync::Arc;

use crate::error::AppError;
use crate::error::AppResult;
use crate::state::AppState;
use crate::utils::is_valid_username;

/// 检查用户是否已认证
async fn require_auth(session: Session) -> AppResult<()> {
    let user_id: Option<String> = session.get("user_id").await?;
    user_id.ok_or(AppError::Unauthorized)?;
    Ok(())
}

/// URL 最大长度限制
const MAX_URL_LENGTH: usize = 2048;

/// 验证 URL 格式
///
/// 检查 URL 是否符合安全规范：
/// - 长度在 1-2048 字符之间
/// - 必须使用 http 或 https 协议
/// - 必须包含有效的主机名
fn is_valid_url(url_str: &str) -> AppResult<()> {
    let trimmed = url_str.trim();

    if trimmed.is_empty() {
        return Err(AppError::ValidationError {
            field: "url".to_string(),
            message: "cannot be empty".to_string(),
        });
    }
    if trimmed.len() > MAX_URL_LENGTH {
        return Err(AppError::ValidationError {
            field: "url".to_string(),
            message: "is too long (max 2048 characters)".to_string(),
        });
    }

    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(AppError::ValidationError {
            field: "url".to_string(),
            message: "must start with http:// or https://".to_string(),
        });
    }

    match Url::parse(trimmed) {
        Ok(url) => {
            if url.scheme() != "http" && url.scheme() != "https" {
                return Err(AppError::ValidationError {
                    field: "url".to_string(),
                    message: "only http and https schemes are allowed".to_string(),
                });
            }

            if url.host_str().is_none() || url.host_str().map(|h| h.is_empty()).unwrap_or(false) {
                return Err(AppError::ValidationError {
                    field: "url".to_string(),
                    message: "must have a valid host".to_string(),
                });
            }

            Ok(())
        }
        Err(_) => Err(AppError::ValidationError {
            field: "url".to_string(),
            message: format!("invalid format: {}", trimmed),
        }),
    }
}

/// 创建用户请求负载
#[derive(Deserialize)]
pub struct CreateUserPayload {
    pub username: String,
}

/// 链接集合请求负载
#[derive(Deserialize)]
pub struct LinksPayload {
    pub links: Vec<String>,
}

/// 用户排序请求负载
#[derive(Deserialize, Serialize)]
pub struct OrderPayload {
    pub order: Vec<String>,
}

/// 列出所有用户
///
/// 返回按 rank 排序的用户名列表。
pub async fn list_users(
    _session: Session,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<String>>> {
    require_auth(_session).await?;

    let rows = sqlx::query("SELECT username FROM users ORDER BY rank ASC")
        .fetch_all(&state.db)
        .await?;

    let list: Vec<String> = rows.iter().map(|r| r.get("username")).collect();
    Ok(Json(list))
}

/// 创建新用户
///
/// 创建一个新用户并分配下一个可用的 rank。
pub async fn create_user(
    _session: Session,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateUserPayload>,
) -> AppResult<(StatusCode, Json<String>)> {
    require_auth(_session).await?;

    let username = payload.username.trim().to_string();

    if username.is_empty() || username.len() > 64 {
        return Err(AppError::ValidationError {
            field: "username".to_string(),
            message: "must be 1-64 characters".to_string(),
        });
    }
    if !is_valid_username(&username) {
        return Err(AppError::ValidationError {
            field: "username".to_string(),
            message: "contains invalid characters".to_string(),
        });
    }

    let max_rank_res = sqlx::query("SELECT MAX(rank) FROM users")
        .fetch_one(&state.db)
        .await;

    let next_rank: i64 = match max_rank_res {
        Ok(row) => row.try_get::<i64, _>(0).unwrap_or(0) + 1,
        Err(_) => 1,
    };

    let result = sqlx::query("INSERT INTO users (username, links, rank) VALUES ($1, '[]', $2)")
        .bind(&username)
        .bind(next_rank)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => {
            tracing::info!(%username, "user created");
            Ok((StatusCode::CREATED, Json(username)))
        }
        Err(e) => {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.message().contains("UNIQUE constraint")
            {
                return Err(AppError::ValidationError {
                    field: "username".to_string(),
                    message: "already exists".to_string(),
                });
            }
            Err(AppError::DbError(e))
        }
    }
}

/// 删除用户
///
/// 删除指定用户及其所有关联数据。
pub async fn delete_user(
    _session: Session,
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> AppResult<Json<&'static str>> {
    require_auth(_session).await?;

    if !is_valid_username(&username) {
        return Err(AppError::ValidationError {
            field: "username".to_string(),
            message: "invalid format".to_string(),
        });
    }

    let res = sqlx::query("DELETE FROM users WHERE username = $1")
        .bind(&username)
        .execute(&state.db)
        .await?;

    if res.rows_affected() > 0 {
        tracing::info!(%username, "user deleted");
        Ok(Json("deleted"))
    } else {
        Err(AppError::NotFound("User not found".into()))
    }
}

/// 获取用户的链接列表
///
/// 返回指定用户的所有订阅链接。
pub async fn get_links(
    _session: Session,
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    require_auth(_session).await?;

    if !is_valid_username(&username) {
        return Err(AppError::ValidationError {
            field: "username".to_string(),
            message: "invalid format".to_string(),
        });
    }

    let row = sqlx::query("SELECT links FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&state.db)
        .await?;

    match row {
        Some(r) => {
            let links: serde_json::Value = r.get("links");
            Ok(Json(links))
        }
        None => Err(AppError::NotFound("User not found".into())),
    }
}

/// 设置用户的链接列表
///
/// 更新指定用户的订阅链接，会自动去重和验证 URL 格式。
pub async fn set_links(
    _session: Session,
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
    Json(payload): Json<LinksPayload>,
) -> AppResult<Json<Vec<String>>> {
    require_auth(_session).await?;

    if !is_valid_username(&username) {
        return Err(AppError::ValidationError {
            field: "username".to_string(),
            message: "invalid format".to_string(),
        });
    }

    if payload.links.len() > state.user_limits.max_links_per_user {
        return Err(AppError::ValidationError {
            field: "links".to_string(),
            message: format!("maximum {} allowed", state.user_limits.max_links_per_user),
        });
    }

    for link in &payload.links {
        is_valid_url(link)?;
    }

    let unique_links: Vec<String> = payload.links
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let links_value = serde_json::to_value(&unique_links)
        .unwrap_or(serde_json::json!([]));

    let res = sqlx::query("UPDATE users SET links = $1 WHERE username = $2")
        .bind(&links_value)
        .bind(&username)
        .execute(&state.db)
        .await?;

    if res.rows_affected() > 0 {
        tracing::info!(username = %username, link_count = unique_links.len(), "Links updated");
        Ok(Json(unique_links))
    } else {
        Err(AppError::NotFound("User not found".into()))
    }
}

/// 设置用户排序顺序
///
/// 更新所有用户的 rank 值以改变显示顺序。
pub async fn set_user_order(
    _session: Session,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<OrderPayload>,
) -> AppResult<Json<Vec<String>>> {
    require_auth(_session).await?;

    let order = &payload.order;

    if order.is_empty() {
        return Err(AppError::ValidationError {
            field: "order".to_string(),
            message: "must not be empty".to_string(),
        });
    }

    if order.len() > state.user_limits.max_users {
        return Err(AppError::ValidationError {
            field: "order".to_string(),
            message: format!("maximum {} users allowed", state.user_limits.max_users),
        });
    }

    for username in order {
        if username.is_empty() || username.len() > 64 || !is_valid_username(username) {
            return Err(AppError::ValidationError {
                field: "order.username".to_string(),
                message: format!("invalid: {}", username),
            });
        }
    }

    let mut tx = state.db.begin().await?;

    for (i, username) in order.iter().enumerate() {
        let result = sqlx::query("UPDATE users SET rank = $1 WHERE username = $2")
            .bind(i as i64)
            .bind(username)
            .execute(&mut *tx)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!("User not found: {}", username)));
        }
    }

    tx.commit().await?;

    tracing::info!(user_count = order.len(), "User order updated");
    Ok(Json(order.clone()))
}
