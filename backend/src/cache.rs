use sqlx::{Row, SqlitePool};
use submora_shared::users::UserCacheStatusResponse;

use crate::{state::AppState, subscriptions};

#[derive(Clone, Debug)]
pub struct CachedSnapshot {
    pub username: String,
    pub content: String,
    pub line_count: u32,
    pub body_bytes: u64,
    pub generated_at: i64,
    pub expires_at: i64,
    pub source_config_version: i64,
}

impl CachedSnapshot {
    pub fn is_fresh(&self, now: i64) -> bool {
        self.expires_at > now
    }
}

pub async fn load_user_snapshot(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<CachedSnapshot>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT
            username,
            content,
            line_count,
            body_bytes,
            generated_at,
            expires_at,
            source_config_version
        FROM user_cache_snapshots
        WHERE username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(snapshot_from_row))
}

pub async fn store_user_snapshot(
    pool: &SqlitePool,
    username: &str,
    content: &str,
    ttl_secs: u64,
    source_config_version: i64,
) -> Result<Option<CachedSnapshot>, sqlx::Error> {
    let now = now_epoch();
    let expires_at = now.saturating_add(ttl_secs.min(i64::MAX as u64) as i64);
    let line_count = count_non_empty_lines(content);
    let body_bytes = content.len() as u64;

    let result = sqlx::query(
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
        SELECT $1, $2, $3, $4, $5, $6, $7
        WHERE EXISTS (
            SELECT 1
            FROM users
            WHERE username = $1
              AND config_version = $7
        )
        ON CONFLICT(username) DO UPDATE SET
            content = excluded.content,
            line_count = excluded.line_count,
            body_bytes = excluded.body_bytes,
            generated_at = excluded.generated_at,
            expires_at = excluded.expires_at,
            source_config_version = excluded.source_config_version
        WHERE EXISTS (
            SELECT 1
            FROM users
            WHERE username = excluded.username
              AND config_version = excluded.source_config_version
        )
        "#,
    )
    .bind(username)
    .bind(content)
    .bind(i64::from(line_count))
    .bind(body_bytes as i64)
    .bind(now)
    .bind(expires_at)
    .bind(source_config_version)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    Ok(Some(CachedSnapshot {
        username: username.to_string(),
        content: content.to_string(),
        line_count,
        body_bytes,
        generated_at: now,
        expires_at,
        source_config_version,
    }))
}

pub async fn clear_user_snapshot(pool: &SqlitePool, username: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM user_cache_snapshots WHERE username = $1")
        .bind(username)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn rebuild_user_snapshot(
    state: &AppState,
    username: &str,
    links: Vec<String>,
    source_config_version: i64,
) -> Result<Option<CachedSnapshot>, sqlx::Error> {
    let merged = subscriptions::fetch_and_merge_for_user(
        subscriptions::FetchRuntime {
            pool: &state.db,
            client: &state.client,
            resolver: state.dns_resolver.clone(),
            pinned_client_pool: state.pinned_client_pool.clone(),
            semaphore: state.fetch_semaphore.clone(),
            concurrent_limit: state.config.concurrent_limit,
        },
        username,
        links,
    )
    .await;

    store_user_snapshot(
        &state.db,
        username,
        &merged,
        state.config.cache_ttl_secs,
        source_config_version,
    )
    .await
}

pub async fn load_user_cache_status(
    pool: &SqlitePool,
    username: &str,
    expected_config_version: i64,
) -> Result<UserCacheStatusResponse, sqlx::Error> {
    let snapshot = load_user_snapshot(pool, username).await?;

    Ok(match snapshot.as_ref() {
        Some(snapshot) if snapshot.source_config_version == expected_config_version => {
            status_from_snapshot(username, Some(snapshot))
        }
        _ => empty_status(username),
    })
}

pub fn status_from_snapshot(
    username: &str,
    snapshot: Option<&CachedSnapshot>,
) -> UserCacheStatusResponse {
    match snapshot {
        Some(snapshot) => UserCacheStatusResponse {
            username: snapshot.username.clone(),
            state: snapshot_state(snapshot, now_epoch()).to_string(),
            line_count: snapshot.line_count,
            body_bytes: snapshot.body_bytes,
            generated_at: Some(snapshot.generated_at),
            expires_at: Some(snapshot.expires_at),
        },
        None => empty_status(username),
    }
}

pub fn empty_status(username: &str) -> UserCacheStatusResponse {
    UserCacheStatusResponse {
        username: username.to_string(),
        state: "empty".to_string(),
        line_count: 0,
        body_bytes: 0,
        generated_at: None,
        expires_at: None,
    }
}

fn snapshot_from_row(row: sqlx::sqlite::SqliteRow) -> CachedSnapshot {
    CachedSnapshot {
        username: row.get("username"),
        content: row.get("content"),
        line_count: row
            .get::<i64, _>("line_count")
            .try_into()
            .unwrap_or_default(),
        body_bytes: row
            .get::<i64, _>("body_bytes")
            .try_into()
            .unwrap_or_default(),
        generated_at: row.get("generated_at"),
        expires_at: row.get("expires_at"),
        source_config_version: row.get("source_config_version"),
    }
}

fn snapshot_state(snapshot: &CachedSnapshot, now: i64) -> &'static str {
    if snapshot.is_fresh(now) {
        "fresh"
    } else {
        "expired"
    }
}

fn count_non_empty_lines(content: &str) -> u32 {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count()
        .try_into()
        .unwrap_or(u32::MAX)
}

pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::TempDir;

    use crate::db;

    use super::{clear_user_snapshot, load_user_snapshot, store_user_snapshot};

    async fn test_pool() -> (TempDir, sqlx::SqlitePool) {
        let tempdir = TempDir::new().expect("tempdir should be created");
        let database_url = format!(
            "sqlite://{}?mode=rwc",
            tempdir.path().join("cache.db").display()
        );
        db::prepare_database_dir(&database_url).expect("database directory should exist");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&database_url)
            .await
            .expect("sqlite should connect");
        db::run_migrations(&pool)
            .await
            .expect("migrations should initialize");

        (tempdir, pool)
    }

    #[tokio::test]
    async fn stale_snapshot_write_is_dropped_after_config_version_changes() {
        let (_tempdir, pool) = test_pool().await;

        sqlx::query(
            "INSERT INTO users (username, links, rank, config_version) VALUES ($1, '[]', 0, 1)",
        )
        .bind("demo")
        .execute(&pool)
        .await
        .expect("user should insert");

        let snapshot = store_user_snapshot(&pool, "demo", "old", 300, 1)
            .await
            .expect("snapshot write should succeed");
        assert!(snapshot.is_some());

        clear_user_snapshot(&pool, "demo")
            .await
            .expect("snapshot should clear");

        sqlx::query("UPDATE users SET config_version = 2 WHERE username = $1")
            .bind("demo")
            .execute(&pool)
            .await
            .expect("config version should update");

        let stale = store_user_snapshot(&pool, "demo", "stale", 300, 1)
            .await
            .expect("stale write should not fail");
        assert!(stale.is_none());

        let snapshot = load_user_snapshot(&pool, "demo")
            .await
            .expect("snapshot query should succeed");
        assert!(snapshot.is_none());
    }
}
