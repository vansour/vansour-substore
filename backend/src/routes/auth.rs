use std::{net::SocketAddr, sync::Arc};

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::HeaderMap,
};
use sqlx::Row;
use tower_sessions::Session;

use crate::{
    error::{ApiError, ApiResult, message_response},
    security,
    state::AppState,
};
use submora_core::{is_strong_password, is_valid_password_length, is_valid_username};
use submora_shared::{
    api::ApiMessage,
    auth::{CsrfTokenResponse, CurrentUserResponse, LoginRequest, UpdateAccountRequest},
};

const SESSION_KEY: &str = "user_id";

fn validate_login(payload: &LoginRequest) -> ApiResult<()> {
    if !is_valid_username(payload.username.trim()) {
        return Err(ApiError::validation("username", "invalid username"));
    }
    if !is_valid_password_length(payload.password.trim()) {
        return Err(ApiError::validation(
            "password",
            "password must be 1-128 characters",
        ));
    }
    Ok(())
}

fn validate_account_update(payload: &UpdateAccountRequest) -> ApiResult<()> {
    let username = payload.new_username.trim();
    let password = payload.new_password.trim();

    if !is_valid_username(username) {
        return Err(ApiError::validation("new_username", "invalid username"));
    }
    if !is_valid_password_length(password) {
        return Err(ApiError::validation(
            "new_password",
            "password must be 1-128 characters",
        ));
    }
    if !is_strong_password(password) {
        return Err(ApiError::validation(
            "new_password",
            "password must include letters, numbers, and symbols",
        ));
    }
    let Some(current_password) = payload.current_password.as_deref() else {
        return Err(ApiError::validation(
            "current_password",
            "current password is required",
        ));
    };
    if !is_valid_password_length(current_password.trim()) {
        return Err(ApiError::validation(
            "current_password",
            "password must be 1-128 characters",
        ));
    }

    Ok(())
}

pub async fn csrf_token(session: Session) -> ApiResult<Json<CsrfTokenResponse>> {
    security::csrf_token(session).await
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    session: Session,
    Json(payload): Json<LoginRequest>,
) -> ApiResult<Json<ApiMessage>> {
    validate_login(&payload)?;
    security::verify_csrf(&session, &headers).await?;

    let login_key = security::login_rate_limit_key(
        &headers,
        payload.username.trim(),
        Some(peer_addr),
        state.config.trust_proxy_headers,
    );
    state.login_rate_limiter.check(&login_key).await?;

    let row = sqlx::query("SELECT password_hash FROM admins WHERE username = $1")
        .bind(payload.username.trim())
        .fetch_optional(&state.db)
        .await?;

    let Some(row) = row else {
        state.login_rate_limiter.record_failure(&login_key).await;
        return Err(ApiError::unauthorized());
    };

    let hash: String = row.get("password_hash");
    let parsed_hash = PasswordHash::new(&hash)
        .map_err(|_| ApiError::internal("invalid password hash in database"))?;

    if Argon2::default()
        .verify_password(payload.password.trim().as_bytes(), &parsed_hash)
        .is_err()
    {
        state.login_rate_limiter.record_failure(&login_key).await;
        return Err(ApiError::unauthorized());
    }

    state.login_rate_limiter.record_success(&login_key).await;
    session
        .insert(SESSION_KEY, payload.username.trim().to_string())
        .await?;
    Ok(message_response("Logged in"))
}

pub async fn logout(headers: HeaderMap, session: Session) -> ApiResult<Json<ApiMessage>> {
    security::verify_csrf(&session, &headers).await?;
    session.flush().await?;
    Ok(message_response("Logged out"))
}

pub async fn me(session: Session) -> ApiResult<Json<CurrentUserResponse>> {
    let Some(username) = session.get::<String>(SESSION_KEY).await? else {
        return Err(ApiError::unauthorized());
    };

    Ok(Json(CurrentUserResponse { username }))
}

pub async fn update_account(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    session: Session,
    Json(payload): Json<UpdateAccountRequest>,
) -> ApiResult<Json<ApiMessage>> {
    validate_account_update(&payload)?;
    security::verify_csrf(&session, &headers).await?;

    let Some(current_user) = session.get::<String>(SESSION_KEY).await? else {
        return Err(ApiError::unauthorized());
    };

    let row = sqlx::query("SELECT password_hash FROM admins WHERE username = $1")
        .bind(&current_user)
        .fetch_optional(&state.db)
        .await?;

    let Some(row) = row else {
        return Err(ApiError::unauthorized());
    };

    let current_hash: String = row.get("password_hash");
    let parsed_hash = PasswordHash::new(&current_hash)
        .map_err(|_| ApiError::internal("invalid password hash in database"))?;

    let current_password = payload.current_password.unwrap_or_default();
    if Argon2::default()
        .verify_password(current_password.trim().as_bytes(), &parsed_hash)
        .is_err()
    {
        return Err(ApiError::validation(
            "current_password",
            "current password is incorrect",
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.new_password.trim().as_bytes(), &salt)
        .map_err(|error| ApiError::internal(format!("failed to hash password: {error}")))?
        .to_string();

    let result = sqlx::query("UPDATE admins SET username = $1, password_hash = $2, updated_at = strftime('%s', 'now') WHERE username = $3")
        .bind(payload.new_username.trim())
        .bind(password_hash)
        .bind(&current_user)
        .execute(&state.db)
        .await;

    match result {
        Ok(_) => {
            session.flush().await?;
            Ok(message_response("Account updated, please login again"))
        }
        Err(sqlx::Error::Database(db_error)) if db_error.message().contains("UNIQUE") => Err(
            ApiError::validation("new_username", "username already exists"),
        ),
        Err(error) => Err(ApiError::from(error)),
    }
}
