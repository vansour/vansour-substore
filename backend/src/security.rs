use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{Json, http::HeaderMap};
use submora_shared::auth::CsrfTokenResponse;
use tokio::sync::Mutex;
use tower_sessions::Session;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};

const CSRF_SESSION_KEY: &str = "csrf_token";
pub const CSRF_HEADER: &str = "x-csrf-token";

#[derive(Clone, Debug)]
pub struct LoginRateLimiter {
    state: Arc<Mutex<HashMap<String, LoginAttemptState>>>,
    max_attempts: usize,
    window: Duration,
    lockout: Duration,
}

#[derive(Clone, Debug)]
struct LoginAttemptState {
    failures: Vec<Instant>,
    locked_until: Option<Instant>,
}

#[derive(Clone, Debug)]
pub struct PublicRateLimiter {
    state: Arc<Mutex<HashMap<String, Vec<Instant>>>>,
    max_requests: usize,
    window: Duration,
}

impl LoginRateLimiter {
    pub fn new(max_attempts: usize, window_secs: u64, lockout_secs: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            max_attempts,
            window: Duration::from_secs(window_secs),
            lockout: Duration::from_secs(lockout_secs),
        }
    }

    pub async fn check(&self, key: &str) -> ApiResult<()> {
        let mut state = self.state.lock().await;
        prune_expired(&mut state, self.window);

        if let Some(entry) = state.get_mut(key) {
            entry
                .failures
                .retain(|attempt| attempt.elapsed() <= self.window);

            if let Some(locked_until) = entry.locked_until {
                if locked_until > Instant::now() {
                    let retry_after_secs = locked_until
                        .saturating_duration_since(Instant::now())
                        .as_secs()
                        .max(1);
                    return Err(ApiError::too_many_requests(format!(
                        "too many login attempts, retry in {retry_after_secs}s"
                    )));
                }

                entry.locked_until = None;
            }

            if entry.failures.is_empty() {
                state.remove(key);
            }
        }

        Ok(())
    }

    pub async fn record_failure(&self, key: &str) {
        let mut state = self.state.lock().await;
        prune_expired(&mut state, self.window);

        let now = Instant::now();
        let entry = state.entry(key.to_string()).or_insert(LoginAttemptState {
            failures: Vec::new(),
            locked_until: None,
        });
        entry
            .failures
            .retain(|attempt| attempt.elapsed() <= self.window);
        entry.failures.push(now);

        if entry.failures.len() >= self.max_attempts {
            entry.failures.clear();
            entry.locked_until = Some(now + self.lockout);
        }
    }

    pub async fn record_success(&self, key: &str) {
        let mut state = self.state.lock().await;
        state.remove(key);
    }
}

impl PublicRateLimiter {
    pub fn new(max_requests: usize, window_secs: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            max_requests,
            window: Duration::from_secs(window_secs),
        }
    }

    pub async fn check_and_record(&self, key: &str) -> ApiResult<()> {
        let mut state = self.state.lock().await;
        prune_public_requests(&mut state, self.window);

        let now = Instant::now();
        let entry = state.entry(key.to_string()).or_default();
        entry.retain(|attempt| attempt.elapsed() <= self.window);

        if entry.len() >= self.max_requests {
            let retry_after_secs = entry
                .first()
                .map(|attempt| {
                    self.window
                        .saturating_sub(attempt.elapsed())
                        .as_secs()
                        .max(1)
                })
                .unwrap_or(1);
            return Err(ApiError::too_many_requests(format!(
                "too many public requests, retry in {retry_after_secs}s"
            )));
        }

        entry.push(now);
        Ok(())
    }
}

pub fn login_rate_limit_key(
    headers: &HeaderMap,
    username: &str,
    peer_addr: Option<SocketAddr>,
    trust_proxy_headers: bool,
) -> String {
    let client_ip = request_client_ip(headers, peer_addr, trust_proxy_headers)
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    format!("{client_ip}:{}", username.trim().to_ascii_lowercase())
}

pub fn request_client_ip(
    headers: &HeaderMap,
    peer_addr: Option<SocketAddr>,
    trust_proxy_headers: bool,
) -> Option<IpAddr> {
    let peer_ip = peer_addr.map(|addr| addr.ip());
    if !trust_proxy_headers {
        return peer_ip;
    }

    forwarded_ip(headers).or(peer_ip)
}

pub async fn csrf_token(session: Session) -> ApiResult<Json<CsrfTokenResponse>> {
    let token = get_or_create_csrf_token(&session).await?;
    Ok(Json(CsrfTokenResponse { token }))
}

pub async fn verify_csrf(session: &Session, headers: &HeaderMap) -> ApiResult<()> {
    let Some(expected) = session.get::<String>(CSRF_SESSION_KEY).await? else {
        return Err(ApiError::forbidden("missing csrf token in session"));
    };

    let Some(actual) = headers
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(ApiError::forbidden("missing csrf token header"));
    };

    if actual.trim() != expected {
        return Err(ApiError::forbidden("invalid csrf token"));
    }

    Ok(())
}

async fn get_or_create_csrf_token(session: &Session) -> ApiResult<String> {
    if let Some(token) = session.get::<String>(CSRF_SESSION_KEY).await? {
        return Ok(token);
    }

    let token = Uuid::new_v4().simple().to_string();
    session.insert(CSRF_SESSION_KEY, token.clone()).await?;
    Ok(token)
}

fn prune_expired(state: &mut HashMap<String, LoginAttemptState>, window: Duration) {
    state.retain(|_, entry| {
        entry.failures.retain(|attempt| attempt.elapsed() <= window);
        let still_locked = entry
            .locked_until
            .map(|deadline| deadline > Instant::now())
            .unwrap_or(false);
        still_locked || !entry.failures.is_empty()
    });
}

fn prune_public_requests(state: &mut HashMap<String, Vec<Instant>>, window: Duration) {
    state.retain(|_, attempts| {
        attempts.retain(|attempt| attempt.elapsed() <= window);
        !attempts.is_empty()
    });
}

fn forwarded_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse().ok())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(|value| value.parse().ok())
        })
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::http::HeaderMap;

    use super::login_rate_limit_key;

    fn peer_addr() -> SocketAddr {
        "198.51.100.10:12345".parse().expect("valid peer addr")
    }

    #[test]
    fn uses_peer_ip_when_proxy_headers_are_untrusted() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.8".parse().expect("header"));

        let key = login_rate_limit_key(&headers, "Admin", Some(peer_addr()), false);
        assert_eq!(key, "198.51.100.10:admin");
    }

    #[test]
    fn uses_forwarded_ip_when_proxy_headers_are_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "203.0.113.8, 198.51.100.2".parse().expect("header"),
        );

        let key = login_rate_limit_key(&headers, "Admin", Some(peer_addr()), true);
        assert_eq!(key, "203.0.113.8:admin");
    }

    #[test]
    fn ignores_invalid_proxy_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "not-an-ip".parse().expect("header"));
        headers.insert("x-real-ip", "203.0.113.9".parse().expect("header"));

        let key = login_rate_limit_key(&headers, "Admin", Some(peer_addr()), true);
        assert_eq!(key, "203.0.113.9:admin");
    }
}
