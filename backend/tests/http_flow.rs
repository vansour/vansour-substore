use std::{
    collections::{HashMap, HashSet},
    fs,
    net::SocketAddr,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Method, Request, StatusCode, header},
    response::Response,
    routing::get,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use submora::{app, config::ServerConfig, db, session, state::AppState, subscriptions};
use submora_shared::{
    api::{ApiErrorBody, ApiMessage},
    auth::{CsrfTokenResponse, CurrentUserResponse},
    users::{UserCacheStatusResponse, UserDiagnosticsResponse, UserLinksResponse, UserSummary},
};
use tempfile::TempDir;
use tokio::sync::Semaphore;
use tower::ServiceExt;

struct TestContext {
    config: ServerConfig,
    _tempdir: TempDir,
    fixture_feed_url: String,
    _upstream: UpstreamFixture,
}

impl TestContext {
    async fn new() -> Self {
        let tempdir = TempDir::new().expect("tempdir should be created");
        let upstream = UpstreamFixture::start().await;
        let config = test_config(tempdir.path());
        let mut config = config;
        upstream.apply_to_config(&mut config);
        seed_dist_assets(&config.web_dist_dir);

        Self {
            config,
            _tempdir: tempdir,
            fixture_feed_url: upstream.feed_url.clone(),
            _upstream: upstream,
        }
    }

    async fn app(&self) -> TestApp {
        TestApp::from_config(self.config.clone()).await
    }
}

struct UpstreamFixture {
    feed_url: String,
    override_key: String,
    override_addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl UpstreamFixture {
    async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fixture listener should bind");
        let addr = listener.local_addr().expect("fixture listener addr");

        let app = Router::new()
            .route("/healthz", get(|| async { "ok" }))
            .route(
                "/feed",
                get(|| async { "fixture-line-1\nfixture-line-2\nfixture-line-3\n" }),
            );

        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("fixture server should stay available");
        });

        Self {
            feed_url: format!("http://fixture.invalid:{}/feed", addr.port()),
            override_key: format!("fixture.invalid:{}", addr.port()),
            override_addr: addr,
            task,
        }
    }

    fn apply_to_config(&self, config: &mut ServerConfig) {
        config
            .fetch_host_overrides
            .insert(self.override_key.clone(), vec![self.override_addr]);
    }
}

impl Drop for UpstreamFixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct TestApp {
    db: SqlitePool,
    router: Router,
}

struct RequestOptions<'a> {
    cookie: Option<&'a str>,
    csrf: Option<&'a str>,
    body: Option<Value>,
    headers: &'a [(&'a str, &'a str)],
    peer_addr: SocketAddr,
}

impl TestApp {
    async fn from_config(config: ServerConfig) -> Self {
        db::prepare_database_dir(&config.database_url).expect("database directory should exist");
        let pool = SqlitePoolOptions::new()
            .max_connections(config.db_max_connections)
            .connect(&config.database_url)
            .await
            .expect("sqlite should connect");
        db::run_migrations(&pool)
            .await
            .expect("migrations should initialize");
        db::ensure_admin(&pool, &config.admin_user, &config.admin_password)
            .await
            .expect("admin should be created");

        let session_store = session::build_session_store(pool.clone())
            .await
            .expect("session store should initialize");
        let session_layer = session::build_session_layer(session_store, &config);

        let state = Arc::new(AppState {
            db: pool.clone(),
            client: subscriptions::build_fetch_client(config.fetch_timeout_secs)
                .expect("fetch client should be built"),
            dns_resolver: Arc::new(subscriptions::DnsResolver::with_overrides(
                config.dns_cache_ttl_secs,
                config.fetch_host_overrides.clone(),
            )),
            pinned_client_pool: Arc::new(subscriptions::PinnedClientPool::new(
                config.fetch_timeout_secs,
            )),
            fetch_semaphore: Arc::new(Semaphore::new(config.concurrent_limit)),
            refreshing_snapshots: Arc::new(Mutex::new(HashSet::new())),
            login_rate_limiter: submora::security::LoginRateLimiter::new(
                config.login_max_attempts,
                config.login_window_secs,
                config.login_lockout_secs,
            ),
            public_rate_limiter: submora::security::PublicRateLimiter::new(
                config.public_max_requests,
                config.public_window_secs,
            ),
            config,
        });

        Self {
            db: pool,
            router: app::build_router(state).layer(session_layer),
        }
    }

    async fn request(
        &self,
        method: Method,
        uri: &str,
        cookie: Option<&str>,
        csrf: Option<&str>,
        body: Option<Value>,
    ) -> Response {
        self.request_with_options(
            method,
            uri,
            RequestOptions {
                cookie,
                csrf,
                body,
                headers: &[],
                peer_addr: test_peer_addr(),
            },
        )
        .await
    }

    async fn request_with_options(
        &self,
        method: Method,
        uri: &str,
        options: RequestOptions<'_>,
    ) -> Response {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(cookie) = options.cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        if let Some(csrf) = options.csrf {
            builder = builder.header("x-csrf-token", csrf);
        }
        for (name, value) in options.headers {
            builder = builder.header(*name, *value);
        }

        let mut request = if let Some(body) = options.body {
            builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request should build")
        } else {
            builder.body(Body::empty()).expect("request should build")
        };
        request
            .extensions_mut()
            .insert(ConnectInfo(options.peer_addr));

        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("router should respond")
    }
}

fn test_config(root: &Path) -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".parse().expect("valid host"),
        port: 0,
        web_dist_dir: root.join("dist"),
        database_url: format!("sqlite://{}?mode=rwc", root.join("substore.db").display()),
        cookie_secure: false,
        session_ttl_minutes: 60,
        session_cleanup_interval_secs: 60,
        trust_proxy_headers: false,
        login_max_attempts: 5,
        login_window_secs: 60,
        login_lockout_secs: 300,
        public_max_requests: 60,
        public_window_secs: 60,
        cache_ttl_secs: 300,
        db_max_connections: 1,
        fetch_timeout_secs: 5,
        dns_cache_ttl_secs: 30,
        fetch_host_overrides: HashMap::new(),
        concurrent_limit: 4,
        max_links_per_user: 20,
        max_users: 20,
        admin_user: "admin".to_string(),
        admin_password: "admin".to_string(),
        cors_allow_origin: vec!["http://localhost:8081".to_string()],
    }
}

fn test_peer_addr() -> SocketAddr {
    "198.51.100.10:45678".parse().expect("valid peer addr")
}

fn seed_dist_assets(dist_dir: &Path) {
    let assets_dir = dist_dir.join("assets");
    fs::create_dir_all(&assets_dir).expect("assets directory should exist");
    fs::write(
        dist_dir.join("index.html"),
        "<!doctype html><html><body>phase-11 frontend shell</body></html>",
    )
    .expect("index html should be written");
    fs::write(assets_dir.join("test.txt"), "asset-ok").expect("asset should be written");
}

async fn issue_csrf(app: &TestApp, cookie: Option<&str>) -> (String, String) {
    let response = app
        .request(Method::GET, "/api/auth/csrf", cookie, None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = merged_cookie(cookie, &response);
    let payload: CsrfTokenResponse = response_json(response).await;
    (cookie, payload.token)
}

async fn response_text(response: Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should be readable");
    String::from_utf8(body.to_vec()).expect("response should be utf-8")
}

async fn response_json<T: DeserializeOwned>(response: Response) -> T {
    serde_json::from_str(&response_text(response).await).expect("response should parse as json")
}

fn response_cookie(response: &Response) -> Option<String> {
    response
        .headers()
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::to_string)
}

fn merged_cookie(existing: Option<&str>, response: &Response) -> String {
    response_cookie(response)
        .or_else(|| existing.map(str::to_string))
        .expect("session cookie should be present")
}

fn response_header(response: &Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
}

async fn wait_for_cache_refresh(
    app: &TestApp,
    cookie: &str,
    username: &str,
    previous_generated_at: Option<i64>,
    previous_expires_at: Option<i64>,
) -> UserCacheStatusResponse {
    let mut last_status = None;

    for _ in 0..80 {
        let response = app
            .request(
                Method::GET,
                &format!("/api/users/{username}/cache"),
                Some(cookie),
                None,
                None,
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        let status: UserCacheStatusResponse = response_json(response).await;
        // A background refresh may finish on a slow runner and become expired again
        // before the next poll when the cache TTL is very short.
        let snapshot_advanced = status.generated_at != previous_generated_at
            || status.expires_at != previous_expires_at;
        if status.generated_at.is_some() && status.expires_at.is_some() && snapshot_advanced {
            return status;
        }

        last_status = Some(status);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    panic!("cache refresh did not complete in time; last status: {last_status:?}");
}

#[tokio::test]
async fn serves_dist_assets_and_management_flow() {
    let context = TestContext::new().await;
    let app = context.app().await;
    let fixture_feed_url = context.fixture_feed_url.clone();

    let index = app.request(Method::GET, "/", None, None, None).await;
    assert_eq!(index.status(), StatusCode::OK);
    assert!(
        response_text(index)
            .await
            .contains("phase-11 frontend shell")
    );

    let asset = app
        .request(Method::GET, "/assets/test.txt", None, None, None)
        .await;
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(response_text(asset).await, "asset-ok");

    let unauthorized = app
        .request(Method::GET, "/api/users", None, None, None)
        .await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let (cookie, csrf) = issue_csrf(&app, None).await;
    let login = app
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(login.status(), StatusCode::OK);
    let login_message: ApiMessage = response_json(login).await;
    assert_eq!(login_message.message, "Logged in");

    let me = app
        .request(Method::GET, "/api/auth/me", Some(&cookie), None, None)
        .await;
    assert_eq!(me.status(), StatusCode::OK);
    let current_user: CurrentUserResponse = response_json(me).await;
    assert_eq!(current_user.username, "admin");

    let create_user = app
        .request(
            Method::POST,
            "/api/users",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "demo",
            })),
        )
        .await;
    assert_eq!(create_user.status(), StatusCode::OK);
    let created: UserSummary = response_json(create_user).await;
    assert_eq!(created.username, "demo");

    let save_links = app
        .request(
            Method::PUT,
            "/api/users/demo/links",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "links": [
                    "http://127.0.0.1/feed",
                    "http://127.0.0.1/feed"
                ],
            })),
        )
        .await;
    assert_eq!(save_links.status(), StatusCode::BAD_REQUEST);
    let save_links_error: ApiErrorBody = response_json(save_links).await;
    assert!(save_links_error.message.contains("unsafe target"));

    let save_public_links = app
        .request(
            Method::PUT,
            "/api/users/demo/links",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "links": [
                    fixture_feed_url.clone(),
                    fixture_feed_url.clone()
                ],
            })),
        )
        .await;
    assert_eq!(save_public_links.status(), StatusCode::OK);
    let links: UserLinksResponse = response_json(save_public_links).await;
    assert_eq!(links.links, vec![fixture_feed_url.clone()]);

    let diagnostics_before = app
        .request(
            Method::GET,
            "/api/users/demo/diagnostics",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(diagnostics_before.status(), StatusCode::OK);
    let pending: UserDiagnosticsResponse = response_json(diagnostics_before).await;
    assert_eq!(pending.diagnostics.len(), 1);
    assert_eq!(pending.diagnostics[0].status, "pending");

    let cache_before = app
        .request(
            Method::GET,
            "/api/users/demo/cache",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(cache_before.status(), StatusCode::OK);
    let empty_cache: UserCacheStatusResponse = response_json(cache_before).await;
    assert_eq!(empty_cache.state, "empty");

    let public_feed = app.request(Method::GET, "/demo", None, None, None).await;
    assert_eq!(public_feed.status(), StatusCode::OK);
    assert_eq!(
        response_header(&public_feed, "x-substore-cache").as_deref(),
        Some("miss")
    );
    let public_text = response_text(public_feed).await;
    assert!(!public_text.contains("<!--"));

    let cached_feed = app.request(Method::GET, "/demo", None, None, None).await;
    assert_eq!(cached_feed.status(), StatusCode::OK);
    assert_eq!(
        response_header(&cached_feed, "x-substore-cache").as_deref(),
        Some("hit")
    );
    assert_eq!(response_text(cached_feed).await, public_text);

    let diagnostics_after = app
        .request(
            Method::GET,
            "/api/users/demo/diagnostics",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(diagnostics_after.status(), StatusCode::OK);
    let diagnostics: UserDiagnosticsResponse = response_json(diagnostics_after).await;
    assert_eq!(diagnostics.diagnostics.len(), 1);
    assert_ne!(diagnostics.diagnostics[0].status, "pending");
    assert!(diagnostics.diagnostics[0].detail.is_some());

    let cache_after = app
        .request(
            Method::GET,
            "/api/users/demo/cache",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(cache_after.status(), StatusCode::OK);
    let cache_status: UserCacheStatusResponse = response_json(cache_after).await;
    assert_eq!(cache_status.state, "fresh");
    assert!(cache_status.generated_at.is_some());
    assert!(cache_status.expires_at.is_some());

    let logout = app
        .request(
            Method::POST,
            "/api/auth/logout",
            Some(&cookie),
            Some(&csrf),
            None,
        )
        .await;
    assert_eq!(logout.status(), StatusCode::OK);
    let logout_message: ApiMessage = response_json(logout).await;
    assert_eq!(logout_message.message, "Logged out");
}

#[tokio::test]
async fn mutating_routes_require_csrf() {
    let context = TestContext::new().await;
    let app = context.app().await;

    let forbidden_login = app
        .request(
            Method::POST,
            "/api/auth/login",
            None,
            None,
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(forbidden_login.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn default_security_headers_are_applied() {
    let context = TestContext::new().await;
    let app = context.app().await;

    let response = app
        .request(Method::GET, "/api/meta/app", None, None, None)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_header(&response, "x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(
        response_header(&response, "x-frame-options").as_deref(),
        Some("DENY")
    );
    assert_eq!(
        response_header(&response, "referrer-policy").as_deref(),
        Some("no-referrer")
    );
    assert_eq!(
        response_header(&response, "permissions-policy").as_deref(),
        Some("camera=(), microphone=(), geolocation=()")
    );
}

#[tokio::test]
async fn unsafe_links_are_rejected_before_persistence() {
    let context = TestContext::new().await;
    let app = context.app().await;
    let (cookie, csrf) = issue_csrf(&app, None).await;

    let login = app
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(login.status(), StatusCode::OK);

    let create_user = app
        .request(
            Method::POST,
            "/api/users",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "unsafe-demo",
            })),
        )
        .await;
    assert_eq!(create_user.status(), StatusCode::OK);

    let save_links = app
        .request(
            Method::PUT,
            "/api/users/unsafe-demo/links",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "links": ["http://127.0.0.1/feed"],
            })),
        )
        .await;
    assert_eq!(save_links.status(), StatusCode::BAD_REQUEST);
    let error: ApiErrorBody = response_json(save_links).await;
    assert!(error.message.contains("links: unsafe target"));

    let links_after = app
        .request(
            Method::GET,
            "/api/users/unsafe-demo/links",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(links_after.status(), StatusCode::OK);
    let links: UserLinksResponse = response_json(links_after).await;
    assert!(links.links.is_empty());

    let cache_after = app
        .request(
            Method::GET,
            "/api/users/unsafe-demo/cache",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(cache_after.status(), StatusCode::OK);
    let cache: UserCacheStatusResponse = response_json(cache_after).await;
    assert_eq!(cache.state, "empty");
}

#[tokio::test]
async fn login_rate_limit_blocks_repeated_failures() {
    let context = TestContext::new().await;
    let app = context.app().await;
    let (cookie, csrf) = issue_csrf(&app, None).await;

    for _ in 0..5 {
        let response = app
            .request(
                Method::POST,
                "/api/auth/login",
                Some(&cookie),
                Some(&csrf),
                Some(json!({
                    "username": "admin",
                    "password": "wrong-password",
                })),
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let limited = app
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "wrong-password",
            })),
        )
        .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn spoofed_forwarded_for_does_not_bypass_lockout_when_untrusted() {
    let tempdir = TempDir::new().expect("tempdir should be created");
    let mut config = test_config(tempdir.path());
    config.login_max_attempts = 2;
    seed_dist_assets(&config.web_dist_dir);
    let app = TestApp::from_config(config).await;
    let (cookie, csrf) = issue_csrf(&app, None).await;

    for forwarded_ip in ["203.0.113.10", "203.0.113.11"] {
        let response = app
            .request_with_options(
                Method::POST,
                "/api/auth/login",
                RequestOptions {
                    cookie: Some(&cookie),
                    csrf: Some(&csrf),
                    body: Some(json!({
                        "username": "admin",
                        "password": "wrong-password",
                    })),
                    headers: &[("x-forwarded-for", forwarded_ip)],
                    peer_addr: test_peer_addr(),
                },
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let limited = app
        .request_with_options(
            Method::POST,
            "/api/auth/login",
            RequestOptions {
                cookie: Some(&cookie),
                csrf: Some(&csrf),
                body: Some(json!({
                    "username": "admin",
                    "password": "wrong-password",
                })),
                headers: &[("x-forwarded-for", "203.0.113.12")],
                peer_addr: test_peer_addr(),
            },
        )
        .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn trusted_forwarded_for_partitions_login_rate_limit_keys() {
    let tempdir = TempDir::new().expect("tempdir should be created");
    let mut config = test_config(tempdir.path());
    config.login_max_attempts = 2;
    config.trust_proxy_headers = true;
    seed_dist_assets(&config.web_dist_dir);
    let app = TestApp::from_config(config).await;
    let (cookie, csrf) = issue_csrf(&app, None).await;

    for _ in 0..2 {
        let response = app
            .request_with_options(
                Method::POST,
                "/api/auth/login",
                RequestOptions {
                    cookie: Some(&cookie),
                    csrf: Some(&csrf),
                    body: Some(json!({
                        "username": "admin",
                        "password": "wrong-password",
                    })),
                    headers: &[("x-forwarded-for", "203.0.113.10")],
                    peer_addr: test_peer_addr(),
                },
            )
            .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    let limited = app
        .request_with_options(
            Method::POST,
            "/api/auth/login",
            RequestOptions {
                cookie: Some(&cookie),
                csrf: Some(&csrf),
                body: Some(json!({
                    "username": "admin",
                    "password": "wrong-password",
                })),
                headers: &[("x-forwarded-for", "203.0.113.10")],
                peer_addr: test_peer_addr(),
            },
        )
        .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);

    let different_forwarded_ip = app
        .request_with_options(
            Method::POST,
            "/api/auth/login",
            RequestOptions {
                cookie: Some(&cookie),
                csrf: Some(&csrf),
                body: Some(json!({
                    "username": "admin",
                    "password": "wrong-password",
                })),
                headers: &[("x-forwarded-for", "203.0.113.11")],
                peer_addr: test_peer_addr(),
            },
        )
        .await;
    assert_eq!(different_forwarded_ip.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_route_rate_limit_blocks_repeated_requests() {
    let tempdir = TempDir::new().expect("tempdir should be created");
    let mut config = test_config(tempdir.path());
    config.public_max_requests = 2;
    seed_dist_assets(&config.web_dist_dir);
    let app = TestApp::from_config(config).await;

    let links = serde_json::to_value(Vec::<String>::new()).expect("links should encode");
    sqlx::query("INSERT INTO users (username, links, rank, config_version) VALUES ($1, $2, 0, 1)")
        .bind("public-limit-demo")
        .bind(links)
        .execute(&app.db)
        .await
        .expect("user should insert");

    for _ in 0..2 {
        let response = app
            .request(Method::GET, "/public-limit-demo", None, None, None)
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let limited = app
        .request(Method::GET, "/public-limit-demo", None, None, None)
        .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    let error: ApiErrorBody = response_json(limited).await;
    assert!(error.message.contains("too many public requests"));
}

#[tokio::test]
async fn order_update_requires_complete_user_list() {
    let context = TestContext::new().await;
    let app = context.app().await;
    let (cookie, csrf) = issue_csrf(&app, None).await;

    let login = app
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(login.status(), StatusCode::OK);

    for username in ["alpha", "beta"] {
        let response = app
            .request(
                Method::POST,
                "/api/users",
                Some(&cookie),
                Some(&csrf),
                Some(json!({ "username": username })),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let invalid_order = app
        .request(
            Method::PUT,
            "/api/users/order",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "order": ["alpha"],
            })),
        )
        .await;
    assert_eq!(invalid_order.status(), StatusCode::BAD_REQUEST);
    let error: ApiErrorBody = response_json(invalid_order).await;
    assert!(error.message.contains("every existing user exactly once"));

    let valid_order = app
        .request(
            Method::PUT,
            "/api/users/order",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "order": ["beta", "alpha"],
            })),
        )
        .await;
    assert_eq!(valid_order.status(), StatusCode::OK);
    let order: Vec<String> = response_json(valid_order).await;
    assert_eq!(order, vec!["beta".to_string(), "alpha".to_string()]);
}

#[tokio::test]
async fn refresh_cache_endpoint_builds_snapshot_and_link_updates_invalidate_it() {
    let context = TestContext::new().await;
    let app = context.app().await;
    let (cookie, csrf) = issue_csrf(&app, None).await;
    let fixture_feed_url = context.fixture_feed_url.clone();

    let login = app
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(login.status(), StatusCode::OK);

    let create_user = app
        .request(
            Method::POST,
            "/api/users",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "cache-demo",
            })),
        )
        .await;
    assert_eq!(create_user.status(), StatusCode::OK);

    let save_links = app
        .request(
            Method::PUT,
            "/api/users/cache-demo/links",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "links": [fixture_feed_url],
            })),
        )
        .await;
    assert_eq!(save_links.status(), StatusCode::OK);

    let refresh_cache = app
        .request(
            Method::POST,
            "/api/users/cache-demo/cache/refresh",
            Some(&cookie),
            Some(&csrf),
            None,
        )
        .await;
    assert_eq!(refresh_cache.status(), StatusCode::OK);
    let refreshed: UserCacheStatusResponse = response_json(refresh_cache).await;
    assert_eq!(refreshed.state, "fresh");
    assert!(refreshed.generated_at.is_some());
    assert!(refreshed.expires_at.is_some());

    let clear_cache = app
        .request(
            Method::DELETE,
            "/api/users/cache-demo/cache",
            Some(&cookie),
            Some(&csrf),
            None,
        )
        .await;
    assert_eq!(clear_cache.status(), StatusCode::OK);
    let clear_message: ApiMessage = response_json(clear_cache).await;
    assert_eq!(clear_message.message, "cache cleared");

    let cache_after_clear = app
        .request(
            Method::GET,
            "/api/users/cache-demo/cache",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(cache_after_clear.status(), StatusCode::OK);
    let cleared: UserCacheStatusResponse = response_json(cache_after_clear).await;
    assert_eq!(cleared.state, "empty");

    let refresh_again = app
        .request(
            Method::POST,
            "/api/users/cache-demo/cache/refresh",
            Some(&cookie),
            Some(&csrf),
            None,
        )
        .await;
    assert_eq!(refresh_again.status(), StatusCode::OK);

    let save_empty_links = app
        .request(
            Method::PUT,
            "/api/users/cache-demo/links",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "links": [],
            })),
        )
        .await;
    assert_eq!(save_empty_links.status(), StatusCode::OK);

    let cache_after_update = app
        .request(
            Method::GET,
            "/api/users/cache-demo/cache",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(cache_after_update.status(), StatusCode::OK);
    let invalidated: UserCacheStatusResponse = response_json(cache_after_update).await;
    assert_eq!(invalidated.state, "empty");
}

#[tokio::test]
async fn public_route_skips_failed_fetch_output_but_keeps_diagnostics() {
    let context = TestContext::new().await;
    let app = context.app().await;

    let links = serde_json::to_value(vec!["http://127.0.0.1/feed"]).expect("links should encode");
    sqlx::query("INSERT INTO users (username, links, rank, config_version) VALUES ($1, $2, 0, 1)")
        .bind("blocked-demo")
        .bind(links)
        .execute(&app.db)
        .await
        .expect("user should insert");

    let public_feed = app
        .request(Method::GET, "/blocked-demo", None, None, None)
        .await;
    assert_eq!(public_feed.status(), StatusCode::OK);
    assert_eq!(
        response_header(&public_feed, "x-substore-cache").as_deref(),
        Some("miss")
    );
    assert_eq!(response_text(public_feed).await, "");

    let (cookie, csrf) = issue_csrf(&app, None).await;
    let login = app
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(login.status(), StatusCode::OK);

    let diagnostics = app
        .request(
            Method::GET,
            "/api/users/blocked-demo/diagnostics",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(diagnostics.status(), StatusCode::OK);
    let diagnostics: UserDiagnosticsResponse = response_json(diagnostics).await;
    assert_eq!(diagnostics.diagnostics.len(), 1);
    assert_eq!(diagnostics.diagnostics[0].status, "blocked");
}

#[tokio::test]
async fn cache_status_ignores_snapshot_from_old_config_version() {
    let context = TestContext::new().await;
    let app = context.app().await;

    let links =
        serde_json::to_value(vec!["http://versioned.invalid/feed"]).expect("links should encode");
    sqlx::query("INSERT INTO users (username, links, rank, config_version) VALUES ($1, $2, 0, 2)")
        .bind("versioned-demo")
        .bind(links)
        .execute(&app.db)
        .await
        .expect("user should insert");

    sqlx::query(
        r#"
        INSERT INTO user_cache_snapshots (
            username,
            content,
            line_count,
            body_bytes,
            generated_at,
            expires_at,
            source_config_version
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind("versioned-demo")
    .bind("stale-content")
    .bind(1_i64)
    .bind(13_i64)
    .bind(1_i64)
    .bind(i64::MAX / 2)
    .bind(1_i64)
    .execute(&app.db)
    .await
    .expect("snapshot should insert");

    let (cookie, csrf) = issue_csrf(&app, None).await;
    let login = app
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(login.status(), StatusCode::OK);

    let cache_status = app
        .request(
            Method::GET,
            "/api/users/versioned-demo/cache",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(cache_status.status(), StatusCode::OK);
    let cache_status: UserCacheStatusResponse = response_json(cache_status).await;
    assert_eq!(cache_status.state, "empty");
}

#[tokio::test]
async fn expired_snapshot_serves_stale_content_while_refresh_runs_in_background() {
    let tempdir = TempDir::new().expect("tempdir should be created");
    let upstream = UpstreamFixture::start().await;
    let mut config = test_config(tempdir.path());
    upstream.apply_to_config(&mut config);
    config.cache_ttl_secs = 1;
    config.fetch_timeout_secs = 1;
    seed_dist_assets(&config.web_dist_dir);
    let app = TestApp::from_config(config).await;
    let (cookie, csrf) = issue_csrf(&app, None).await;
    let fixture_feed_url = upstream.feed_url.clone();

    let login = app
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(login.status(), StatusCode::OK);

    let create_user = app
        .request(
            Method::POST,
            "/api/users",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "stale-demo",
            })),
        )
        .await;
    assert_eq!(create_user.status(), StatusCode::OK);

    let save_links = app
        .request(
            Method::PUT,
            "/api/users/stale-demo/links",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "links": [fixture_feed_url],
            })),
        )
        .await;
    assert_eq!(save_links.status(), StatusCode::OK);

    let first_feed = app
        .request(Method::GET, "/stale-demo", None, None, None)
        .await;
    assert_eq!(first_feed.status(), StatusCode::OK);
    assert_eq!(
        response_header(&first_feed, "x-substore-cache").as_deref(),
        Some("miss")
    );
    let first_body = response_text(first_feed).await;

    let first_cache = app
        .request(
            Method::GET,
            "/api/users/stale-demo/cache",
            Some(&cookie),
            None,
            None,
        )
        .await;
    assert_eq!(first_cache.status(), StatusCode::OK);
    let first_status: UserCacheStatusResponse = response_json(first_cache).await;
    assert_eq!(first_status.state, "fresh");
    assert!(first_status.generated_at.is_some());
    assert!(first_status.expires_at.is_some());

    tokio::time::sleep(Duration::from_secs(2)).await;

    let stale_feed = app
        .request(Method::GET, "/stale-demo", None, None, None)
        .await;
    assert_eq!(stale_feed.status(), StatusCode::OK);
    assert_eq!(
        response_header(&stale_feed, "x-substore-cache").as_deref(),
        Some("stale")
    );
    assert_eq!(response_text(stale_feed).await, first_body);

    let refreshed = wait_for_cache_refresh(
        &app,
        &cookie,
        "stale-demo",
        first_status.generated_at,
        first_status.expires_at,
    )
    .await;
    assert_ne!(
        (refreshed.generated_at, refreshed.expires_at),
        (first_status.generated_at, first_status.expires_at)
    );
}

#[tokio::test]
async fn session_survives_app_rebuild_on_same_database() {
    let context = TestContext::new().await;
    let app_one = context.app().await;
    let (cookie, csrf) = issue_csrf(&app_one, None).await;

    let login = app_one
        .request(
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            Some(&csrf),
            Some(json!({
                "username": "admin",
                "password": "admin",
            })),
        )
        .await;
    assert_eq!(login.status(), StatusCode::OK);

    let app_two = context.app().await;
    let me = app_two
        .request(Method::GET, "/api/auth/me", Some(&cookie), None, None)
        .await;

    assert_eq!(me.status(), StatusCode::OK);
    let current_user: CurrentUserResponse = response_json(me).await;
    assert_eq!(current_user.username, "admin");
}
