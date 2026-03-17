use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use sqlx::sqlite::SqlitePoolOptions;
use submora::{app, config::ServerConfig, db, session, state::AppState, subscriptions};
use tokio::{net::TcpListener, sync::Semaphore};
use tracing::info;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,submora=debug".to_string()),
        )
        .with_target(false)
        .compact()
        .init();

    let config = ServerConfig::from_env();
    let bind_addr = config.socket_addr();

    db::prepare_database_dir(&config.database_url)?;

    let pool = SqlitePoolOptions::new()
        .max_connections(config.db_max_connections)
        .connect(&config.database_url)
        .await
        .expect("failed to connect sqlite database");

    db::run_migrations(&pool)
        .await
        .expect("failed to run database migrations");
    db::ensure_admin(&pool, &config.admin_user, &config.admin_password)
        .await
        .expect("failed to ensure default admin");
    let session_store = session::build_session_store(pool.clone())
        .await
        .expect("failed to initialize sqlite session store");
    let session_cleanup_task = session::spawn_expired_session_cleanup(
        session_store.clone(),
        config.session_cleanup_interval_secs,
    );

    let client = subscriptions::build_fetch_client(config.fetch_timeout_secs)
        .expect("failed to build reqwest client");

    let state = Arc::new(AppState {
        db: pool,
        client,
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
        config: config.clone(),
    });

    let app = app::build_router(state).layer(session::build_session_layer(session_store, &config));

    info!(
        name = submora_core::APP_NAME,
        phase = submora_core::CURRENT_PHASE,
        frontend = "dioxus-0.7.3",
        backend = "axum-0.8.8",
        session_store = "sqlite",
        address = %bind_addr,
        database_url = %config.database_url,
        "starting phase 11 implementation"
    );

    let listener = TcpListener::bind(bind_addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        shutdown_signal().await;
        session_cleanup_task.abort();
    })
    .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl+c");
    };

    #[cfg(unix)]
    {
        let terminate = async {
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install terminate handler")
                .recv()
                .await;
        };

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await;
    }
}
