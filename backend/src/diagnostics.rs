use std::collections::HashMap;

use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use submora_shared::users::{LinkDiagnostic, UserDiagnosticsResponse};

#[derive(Clone, Debug)]
pub struct DiagnosticUpsert {
    pub source_url: String,
    pub status: String,
    pub detail: Option<String>,
    pub http_status: Option<u16>,
    pub content_type: Option<String>,
    pub body_bytes: Option<u64>,
    pub redirect_count: u8,
    pub is_html: bool,
}

pub async fn store_user_diagnostics(
    pool: &SqlitePool,
    username: &str,
    diagnostics: &[DiagnosticUpsert],
) -> Result<(), sqlx::Error> {
    if diagnostics.is_empty() {
        return Ok(());
    }

    let fetched_at = now_epoch();
    let mut query_builder: QueryBuilder<'_, Sqlite> = QueryBuilder::new(
        r#"
        INSERT INTO fetch_diagnostics (
            username,
            source_url,
            status,
            detail,
            http_status,
            content_type,
            body_bytes,
            redirect_count,
            is_html,
            fetched_at
        )
        "#,
    );

    query_builder.push_values(diagnostics, |mut builder, diagnostic| {
        builder
            .push_bind(username)
            .push_bind(&diagnostic.source_url)
            .push_bind(&diagnostic.status)
            .push_bind(&diagnostic.detail)
            .push_bind(diagnostic.http_status.map(i64::from))
            .push_bind(&diagnostic.content_type)
            .push_bind(diagnostic.body_bytes.map(saturating_u64_to_i64))
            .push_bind(i64::from(diagnostic.redirect_count))
            .push_bind(if diagnostic.is_html { 1_i64 } else { 0_i64 })
            .push_bind(fetched_at);
    });

    query_builder.push(
        r#"
        ON CONFLICT(username, source_url) DO UPDATE SET
            status = excluded.status,
            detail = excluded.detail,
            http_status = excluded.http_status,
            content_type = excluded.content_type,
            body_bytes = excluded.body_bytes,
            redirect_count = excluded.redirect_count,
            is_html = excluded.is_html,
            fetched_at = excluded.fetched_at
        "#,
    );

    query_builder.build().execute(pool).await?;
    Ok(())
}

pub async fn clear_user_diagnostics(pool: &SqlitePool, username: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM fetch_diagnostics WHERE username = $1")
        .bind(username)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn load_user_diagnostics(
    pool: &SqlitePool,
    username: &str,
    links: &[String],
) -> Result<UserDiagnosticsResponse, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            source_url,
            status,
            detail,
            http_status,
            content_type,
            body_bytes,
            redirect_count,
            is_html,
            fetched_at
        FROM fetch_diagnostics
        WHERE username = $1
        "#,
    )
    .bind(username)
    .fetch_all(pool)
    .await?;

    let diagnostics_by_url: HashMap<String, LinkDiagnostic> = rows
        .into_iter()
        .map(|row| {
            let url: String = row.get("source_url");
            let diagnostic = LinkDiagnostic {
                url: url.clone(),
                status: row.get("status"),
                detail: row.get("detail"),
                http_status: row
                    .get::<Option<i64>, _>("http_status")
                    .and_then(|value| u16::try_from(value).ok()),
                content_type: row.get("content_type"),
                body_bytes: row
                    .get::<Option<i64>, _>("body_bytes")
                    .and_then(|value| u64::try_from(value).ok()),
                redirect_count: row
                    .get::<i64, _>("redirect_count")
                    .try_into()
                    .unwrap_or_default(),
                is_html: row.get::<i64, _>("is_html") != 0,
                fetched_at: row.get("fetched_at"),
            };
            (url, diagnostic)
        })
        .collect();

    let diagnostics = links
        .iter()
        .map(|link| {
            diagnostics_by_url
                .get(link)
                .cloned()
                .unwrap_or_else(|| pending_diagnostic(link))
        })
        .collect();

    Ok(UserDiagnosticsResponse {
        username: username.to_string(),
        diagnostics,
    })
}

fn pending_diagnostic(url: &str) -> LinkDiagnostic {
    LinkDiagnostic {
        url: url.to_string(),
        status: "pending".to_string(),
        detail: Some("No fetch attempt recorded yet".to_string()),
        http_status: None,
        content_type: None,
        body_bytes: None,
        redirect_count: 0,
        is_html: false,
        fetched_at: None,
    }
}

fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| saturating_u64_to_i64(duration.as_secs()))
        .unwrap_or_default()
}

fn saturating_u64_to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}
