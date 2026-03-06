use axum::{
    Router,
    routing::{get, post, put, delete},
    response::Html,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions, Row};
use std::sync::Arc;
use tower_sessions::{SessionManagerLayer, MemoryStore};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};

mod config;
mod error;
mod handlers;
mod log;
mod state;
mod utils;

use state::AppState;

const DB_PATH: &str = "data/substore.db";

// 初始化数据库表
async fn init_db(pool: &SqlitePool) -> std::result::Result<(), sqlx::Error> {
    // 用户表
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            username TEXT PRIMARY KEY,
            links TEXT NOT NULL DEFAULT '[]',
            rank INTEGER NOT NULL DEFAULT 0
        );
        "#,
    )
    .execute(pool)
    .await?;

    // 为 rank 字段创建索引以优化排序性能
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_users_rank ON users(rank)")
        .execute(pool)
        .await?;

    // 管理员表
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS admins (
            username TEXT PRIMARY KEY,
            password_hash TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

// 确保至少有一个管理员账号
async fn ensure_admin(pool: &SqlitePool) {
    let count_res = sqlx::query("SELECT COUNT(*) FROM admins")
        .fetch_one(pool)
        .await;

    let count: i64 = count_res.map(|r| r.get(0)).unwrap_or(0);

    if count == 0 {
        tracing::info!("No admins found. Creating default admin.");
        let username = std::env::var("ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
        let password = std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());

        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = match argon2.hash_password(password.as_bytes(), &salt) {
            Ok(h) => h.to_string(),
            Err(e) => {
                tracing::error!("Failed to hash default password: {}", e);
                return;
            }
        };

        let _ = sqlx::query("INSERT INTO admins (username, password_hash) VALUES ($1, $2)")
            .bind(&username)
            .bind(&password_hash)
            .execute(pool)
            .await;

        tracing::info!("Default admin created: {} / {}", username, password);
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // 1. 加载配置
    println!("Loading configuration...");
    let config = match config::AppConfig::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load config: {}", e);
            std::process::exit(1);
        }
    };

    // 2. 初始化日志
    log::init_logging(&config);

    // 3. 创建数据目录
    if let Some(parent) = std::path::Path::new(DB_PATH).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // 4. 连接 SQLite 数据库
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| format!("sqlite://{}", DB_PATH));

    let pool = SqlitePoolOptions::new()
        .max_connections(config.database.max_connections as u32)
        .connect(&db_url)
        .await
        .expect("Failed to connect to SQLite");

    init_db(&pool)
        .await
        .expect("Failed to initialize DB schema");

    ensure_admin(&pool).await;

    // 5. 初始化全局共享的 HTTP 客户端
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.subscription.fetch_timeout_secs))
        .pool_max_idle_per_host(5)
        .build()
        .expect("Failed to create reqwest client");

    // 6. 创建应用状态
    let state = Arc::new(AppState::from_config(
        pool,
        client,
        config.subscription.fetch_timeout_secs,
        config.subscription.concurrent_limit,
        config.subscription.max_links_per_user,
        config.subscription.max_users,
    ));

    let bind_addr = format!("{}:{}", config.server.host, config.server.port);

    tracing::info!(
        server = %bind_addr,
        db_max_connections = config.database.max_connections,
        fetch_timeout = config.subscription.fetch_timeout_secs,
        concurrent_limit = config.subscription.concurrent_limit,
        "Starting VSS SubStore"
    );

    // 7. 配置 Session 中间件
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(config.server.cookie_secure);

    // 8. 构建路由
    let app = Router::new()
        // 静态文件
        .nest_service("/static", ServeDir::new("web"))
        // 首页
        .route("/", get(index))
        // 健康检查
        .route("/healthz", get(healthz))
        // Auth Handlers
        .route("/api/auth/login", post(handlers::auth::login))
        .route("/api/auth/logout", post(handlers::auth::logout))
        .route("/api/auth/me", get(handlers::auth::get_me))
        .route("/api/auth/account", put(handlers::auth::update_account))
        // User Handlers
        .route("/api/users", get(handlers::user::list_users).post(handlers::user::create_user))
        .route("/api/users/order", put(handlers::user::set_user_order))
        .route("/api/users/:username", delete(handlers::user::delete_user))
        .route("/api/users/:username/links", get(handlers::user::get_links).put(handlers::user::set_links))
        // Subscription Handlers
        .route("/:username", get(handlers::subscription::merged_user))
        // 中间件
        .layer(session_layer)
        .layer(axum::middleware::from_fn(log::trace_requests))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // 9. 启动服务器
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await
}
