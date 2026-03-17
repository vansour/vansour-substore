use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, HeaderValue, Response, header},
    response::{IntoResponse, Json},
};
use sqlx::Row;
use submora_shared::api::AppInfoResponse;
use tracing::warn;

use crate::{cache, error::ApiError, security, state::AppState};

const CACHE_HEADER: &str = "x-substore-cache";
const GENERATED_AT_HEADER: &str = "x-substore-generated-at";
const EXPIRES_AT_HEADER: &str = "x-substore-expires-at";

pub async fn healthz() -> &'static str {
    "ok"
}

pub async fn app_info(State(state): State<Arc<AppState>>) -> Json<AppInfoResponse> {
    Json(AppInfoResponse {
        name: submora_core::APP_NAME.to_string(),
        phase: submora_core::CURRENT_PHASE,
        frontend: "dioxus-0.7.3".to_string(),
        backend: "axum-0.8.8".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        web_dist_dir: state.config.web_dist_dir.display().to_string(),
    })
}

pub async fn merged_user(
    State(state): State<Arc<AppState>>,
    ConnectInfo(peer_addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let client_ip =
        security::request_client_ip(&headers, Some(peer_addr), state.config.trust_proxy_headers)
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "unknown".to_string());
    let rate_limit_key = format!("{client_ip}:{}", username.trim().to_ascii_lowercase());
    state
        .public_rate_limiter
        .check_and_record(&rate_limit_key)
        .await?;

    let Some(mut config) = load_public_user_config(&state, &username).await? else {
        return Err(ApiError::not_found("user not found"));
    };

    if config.links.is_empty() {
        cache::clear_user_snapshot(&state.db, &username).await?;
        return Ok(text_response(String::new(), "empty", None, None));
    }

    if let Some(snapshot) = cache::load_user_snapshot(&state.db, &username).await? {
        if snapshot.source_config_version == config.config_version {
            let now = cache::now_epoch();
            if snapshot.is_fresh(now) {
                return Ok(text_response(
                    snapshot.content,
                    "hit",
                    Some(snapshot.generated_at),
                    Some(snapshot.expires_at),
                ));
            }

            spawn_stale_snapshot_refresh(
                state.clone(),
                username.clone(),
                config.links.clone(),
                config.config_version,
            );
            return Ok(text_response(
                snapshot.content,
                "stale",
                Some(snapshot.generated_at),
                Some(snapshot.expires_at),
            ));
        }

        cache::clear_user_snapshot(&state.db, &username).await?;
    }

    for _ in 0..2 {
        if let Some(snapshot) = cache::rebuild_user_snapshot(
            &state,
            &username,
            config.links.clone(),
            config.config_version,
        )
        .await?
        {
            return Ok(text_response(
                snapshot.content,
                "miss",
                Some(snapshot.generated_at),
                Some(snapshot.expires_at),
            ));
        }

        let Some(next_config) = load_public_user_config(&state, &username).await? else {
            return Err(ApiError::not_found("user not found"));
        };
        config = next_config;

        if config.links.is_empty() {
            cache::clear_user_snapshot(&state.db, &username).await?;
            return Ok(text_response(String::new(), "empty", None, None));
        }
    }

    Ok(text_response(String::new(), "empty", None, None))
}

fn text_response(
    body: String,
    cache_state: &'static str,
    generated_at: Option<i64>,
    expires_at: Option<i64>,
) -> Response<Body> {
    let mut response = Response::new(Body::from(body));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    headers.insert(CACHE_HEADER, HeaderValue::from_static(cache_state));

    if let Some(generated_at) = generated_at
        && let Ok(value) = HeaderValue::from_str(&generated_at.to_string())
    {
        headers.insert(GENERATED_AT_HEADER, value);
    }

    if let Some(expires_at) = expires_at
        && let Ok(value) = HeaderValue::from_str(&expires_at.to_string())
    {
        headers.insert(EXPIRES_AT_HEADER, value);
    }

    response
}

fn spawn_stale_snapshot_refresh(
    state: Arc<AppState>,
    username: String,
    links: Vec<String>,
    config_version: i64,
) {
    if !begin_snapshot_refresh(&state.refreshing_snapshots, &username) {
        return;
    }

    tokio::spawn(async move {
        let result = cache::rebuild_user_snapshot(&state, &username, links, config_version).await;

        finish_snapshot_refresh(&state.refreshing_snapshots, &username);

        if let Err(error) = result {
            warn!(username, error = %error, "failed to refresh stale user snapshot");
        }
    });
}

struct PublicUserConfig {
    links: Vec<String>,
    config_version: i64,
}

async fn load_public_user_config(
    state: &AppState,
    username: &str,
) -> Result<Option<PublicUserConfig>, ApiError> {
    let row = sqlx::query("SELECT links, config_version FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(&state.db)
        .await?;

    row.map(config_from_row).transpose()
}

fn config_from_row(row: sqlx::sqlite::SqliteRow) -> Result<PublicUserConfig, ApiError> {
    let value: serde_json::Value = row.get("links");
    let links = serde_json::from_value(value)
        .map_err(|error| ApiError::internal(format!("failed to decode stored links: {error}")))?;

    Ok(PublicUserConfig {
        links,
        config_version: row.get("config_version"),
    })
}

fn begin_snapshot_refresh(
    refreshing_snapshots: &Arc<Mutex<HashSet<String>>>,
    username: &str,
) -> bool {
    let mut in_flight = refreshing_snapshots
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    in_flight.insert(username.to_string())
}

fn finish_snapshot_refresh(refreshing_snapshots: &Arc<Mutex<HashSet<String>>>, username: &str) {
    let mut in_flight = refreshing_snapshots
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    in_flight.remove(username);
}
