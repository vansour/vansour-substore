//! 错误处理模块
//!
//! 定义应用错误类型和统一的错误响应格式。

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// 应用结果类型别名
pub type AppResult<T> = Result<T, AppError>;

/// 应用错误类型
#[derive(Debug)]
pub enum AppError {
    /// 数据库错误
    DbError(sqlx::Error),
    /// 内部错误
    InternalError(String),
    /// 验证错误
    ValidationError { field: String, message: String },
    /// 未授权
    Unauthorized,
    /// 资源未找到
    NotFound(String),
}

impl std::error::Error for AppError {}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::DbError(e) => write!(f, "Database error: {}", e),
            AppError::InternalError(msg) => write!(f, "Internal server error: {}", msg),
            AppError::ValidationError { field, message } => {
                write!(f, "Validation error on field '{}': {}", field, message)
            }
            AppError::Unauthorized => write!(f, "Unauthorized"),
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::DbError(err)
    }
}

impl From<tower_sessions::session::Error> for AppError {
    fn from(err: tower_sessions::session::Error) -> Self {
        AppError::InternalError(format!("Session error: {}", err))
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_msg) = match self {
            AppError::DbError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": "internal", "message": e.to_string() }),
            ),
            AppError::InternalError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({ "error": "internal", "message": msg }),
            ),
            AppError::ValidationError { field, message } => (
                StatusCode::BAD_REQUEST,
                json!({ "error": "validation", "field": field, "message": message }),
            ),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, json!({ "error": "unauthorized" })),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, json!({ "error": "not_found", "message": msg })),
        };
        (status, Json(error_msg)).into_response()
    }
}
