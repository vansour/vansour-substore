use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use reqwest::Client;
use sqlx::SqlitePool;
use tokio::sync::Semaphore;

use crate::{
    config::ServerConfig,
    security::{LoginRateLimiter, PublicRateLimiter},
    subscriptions::{DnsResolver, PinnedClientPool},
};

#[derive(Clone, Debug)]
pub struct AppState {
    pub db: SqlitePool,
    pub client: Client,
    pub dns_resolver: Arc<DnsResolver>,
    pub pinned_client_pool: Arc<PinnedClientPool>,
    pub fetch_semaphore: Arc<Semaphore>,
    pub refreshing_snapshots: Arc<Mutex<HashSet<String>>>,
    pub login_rate_limiter: LoginRateLimiter,
    pub public_rate_limiter: PublicRateLimiter,
    pub config: ServerConfig,
}
