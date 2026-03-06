//! 认证处理器模块
//!
//! 处理用户登录、登出和账号管理。

use axum::{
    extract::State,
    response::{IntoResponse, Json},
    Json as ResponseJson,
};
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use tower_sessions::Session;
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use std::sync::Arc;

use crate::error::AppError;
use crate::error::AppResult;
use crate::state::AppState;
use crate::utils::is_valid_username;

/// 登录请求负载
#[derive(Deserialize)]
pub struct LoginPayload {
    pub username: String,
    pub password: String,
}

/// 更新账号请求负载
#[derive(Deserialize)]
pub struct UpdateAccountPayload {
    /// 当前密码（用于验证）
    pub current_password: Option<String>,
    pub new_username: String,
    pub new_password: String,
}

/// 验证登录请求负载
fn validate_login_payload(payload: &LoginPayload) -> AppResult<()> {
    // 用户名验证
    if payload.username.is_empty() || payload.username.len() > 64 {
        return Err(AppError::ValidationError {
            field: "username".to_string(),
            message: "Username must be 1-64 characters".to_string(),
        });
    }
    if !is_valid_username(&payload.username) {
        return Err(AppError::ValidationError {
            field: "username".to_string(),
            message: "Username contains invalid characters".to_string(),
        });
    }

    // 密码验证
    if payload.password.is_empty() || payload.password.len() > 128 {
        return Err(AppError::ValidationError {
            field: "password".to_string(),
            message: "Password must be 1-128 characters".to_string(),
        });
    }

    Ok(())
}

/// 验证更新账号请求负载
fn validate_update_payload(payload: &UpdateAccountPayload) -> AppResult<()> {
    let new_username = payload.new_username.trim();
    let new_password = payload.new_password.trim();

    // 验证当前密码（如果提供）
    if let Some(ref pwd) = payload.current_password
        && (pwd.is_empty() || pwd.len() > 128)
    {
        return Err(AppError::ValidationError {
            field: "current_password".to_string(),
            message: "Current password must be 1-128 characters".to_string(),
        });
    }

    // 新用户名验证
    if new_username.is_empty() || new_username.len() > 64 {
        return Err(AppError::ValidationError {
            field: "new_username".to_string(),
            message: "Username must be 1-64 characters".to_string(),
        });
    }
    if !is_valid_username(new_username) {
        return Err(AppError::ValidationError {
            field: "new_username".to_string(),
            message: "Username contains invalid characters".to_string(),
        });
    }

    // 新密码验证
    if new_password.is_empty() || new_password.len() > 128 {
        return Err(AppError::ValidationError {
            field: "new_password".to_string(),
            message: "Password must be 1-128 characters".to_string(),
        });
    }

    Ok(())
}

/// 用户登录
///
/// 接受用户名和密码，验证成功后创建会话。
pub async fn login(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(payload): Json<LoginPayload>,
) -> AppResult<impl IntoResponse> {
    validate_login_payload(&payload)?;

    let row = sqlx::query("SELECT password_hash FROM admins WHERE username = $1")
        .bind(&payload.username)
        .fetch_optional(&state.db)
        .await?;

    if let Some(r) = row {
        let hash_str: String = r.get(0);
        let parsed_hash = PasswordHash::new(&hash_str).map_err(|e| {
            tracing::error!("Invalid password hash stored in DB: {}", e);
            AppError::InternalError("Auth error".to_string())
        })?;

        if Argon2::default()
            .verify_password(payload.password.as_bytes(), &parsed_hash)
            .is_ok()
        {
            session.insert("user_id", payload.username.clone()).await.map_err(|e| {
                tracing::error!("Failed to attach identity: {}", e);
                AppError::InternalError("Login session error".to_string())
            })?;
            tracing::info!(username = %payload.username, "User logged in");
            return Ok(ResponseJson(json!({ "message": "Logged in" })));
        }
    }

    tracing::warn!(username = %payload.username, "Failed login attempt");
    Err(AppError::Unauthorized)
}

/// 用户登出
///
/// 清除当前会话。
pub async fn logout(session: Session) -> impl IntoResponse {
    let _ = session.flush().await;
    tracing::info!("User logged out");
    ResponseJson("Logged out")
}

/// 获取当前登录用户信息
///
/// 返回当前会话中的用户名。
pub async fn get_me(session: Session) -> AppResult<impl IntoResponse> {
    let username: Option<String> = session.get("user_id").await?;
    match username {
        Some(u) => Ok(ResponseJson(json!({ "username": u }))),
        None => Err(AppError::Unauthorized),
    }
}

/// 更新管理员账号
///
/// 允许修改用户名和密码。如果用户名未改变，修改密码需要提供当前密码验证。
pub async fn update_account(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(payload): Json<UpdateAccountPayload>,
) -> AppResult<impl IntoResponse> {
    let current_user: Option<String> = session.get("user_id").await?;
    let current_user = current_user.ok_or(AppError::Unauthorized)?;

    validate_update_payload(&payload)?;

    let new_username = payload.new_username.trim().to_string();
    let new_password = payload.new_password.trim();

    // 如果用户名没变但密码变了，验证当前密码
    if new_username == current_user && !new_password.is_empty() {
        let row = sqlx::query("SELECT password_hash FROM admins WHERE username = $1")
            .bind(&current_user)
            .fetch_optional(&state.db)
            .await?;

        if let Some(r) = row {
            let hash_str: String = r.get(0);
            let parsed_hash = PasswordHash::new(&hash_str).map_err(|e| {
                tracing::error!("Invalid password hash stored in DB: {}", e);
                AppError::InternalError("Auth error".to_string())
            })?;

            if let Some(ref current_pwd) = payload.current_password
                && Argon2::default()
                    .verify_password(current_pwd.as_bytes(), &parsed_hash)
                    .is_err()
            {
                return Err(AppError::ValidationError {
                    field: "current_password".to_string(),
                    message: "Current password is incorrect".to_string(),
                });
            }
        }
    }

    // Hash new password
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(new_password.as_bytes(), &salt)
        .map_err(|e| AppError::InternalError(format!("Hash error: {}", e)))?
        .to_string();

    sqlx::query("UPDATE admins SET username = $1, password_hash = $2 WHERE username = $3")
        .bind(&new_username)
        .bind(&password_hash)
        .bind(&current_user)
        .execute(&state.db)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e
                && db_err.message().contains("UNIQUE constraint")
            {
                tracing::warn!(new_username = %new_username, "Username already exists");
                return AppError::ValidationError {
                    field: "new_username".to_string(),
                    message: "Username already exists".to_string(),
                };
            }
            tracing::error!("Update account error: {}", e);
            AppError::InternalError("Failed to update account".to_string())
        })?;

    let _ = session.flush().await;
    tracing::info!(old_username = %current_user, new_username = %new_username, "Account updated");
    Ok(ResponseJson("Account updated, please login again"))
}
