use dioxus::prelude::*;

use super::state::{CacheDisplay, FeedbackSignals, RefreshState};
use crate::components::console::services;

#[component]
pub fn CachePanel(
    username: String,
    cache: CacheDisplay,
    cache_error: Option<String>,
    feedback: FeedbackSignals,
    refresh: RefreshState,
) -> Element {
    let username_for_refresh = username.clone();
    let username_for_clear = username.clone();

    rsx! {
        article { class: "panel",
            div { class: "section-head",
                div {
                    h2 { "Merged Cache" }
                    p { class: "muted", "The server reuses a persisted merged snapshot until its TTL expires or the source list changes." }
                }
                span { class: "tag {cache.state_class()}", "{cache.state}" }
            }
            div { class: "metric-grid",
                article { class: "metric-card",
                    span { class: "stat-kicker", "Lines" }
                    strong { class: "metric-value", "{cache.line_count}" }
                    p { class: "stat-note", "Non-empty merged lines persisted in SQLite." }
                }
                article { class: "metric-card",
                    span { class: "stat-kicker", "Bytes" }
                    strong { class: "metric-value", "{cache.body_bytes}" }
                    p { class: "stat-note", "Snapshot payload size in UTF-8 bytes." }
                }
                article { class: "metric-card",
                    span { class: "stat-kicker", "Generated" }
                    strong { class: "metric-value metric-value--small", "{cache.generated_at}" }
                    p { class: "stat-note", "Formatted UTC time plus raw Unix seconds for the last completed rebuild." }
                }
                article { class: "metric-card",
                    span { class: "stat-kicker", "Expires" }
                    strong { class: "metric-value metric-value--small", "{cache.expires_at}" }
                    p { class: "stat-note", "Expired snapshots can now be served stale while a background refresh rebuilds them." }
                }
            }
            div { class: "button-row",
                button {
                    class: "button button--primary",
                    onclick: move |_| {
                        feedback.clear();
                        let username = username_for_refresh.clone();
                        spawn(async move {
                            match services::refresh_cache(username).await {
                                Ok(status) => {
                                    feedback.set_status(format!(
                                        "Refreshed cache for {} with {} lines",
                                        status.username,
                                        status.line_count
                                    ));
                                    refresh.bump_cache();
                                    refresh.bump_diagnostics();
                                }
                                Err(error) => feedback.set_error(error),
                            }
                        });
                    },
                    "Refresh cache"
                }
                button {
                    class: "button button--ghost",
                    onclick: move |_| refresh.bump_cache(),
                    "Reload cache status"
                }
                button {
                    class: "button button--danger",
                    onclick: move |_| {
                        feedback.clear();
                        let username = username_for_clear.clone();
                        spawn(async move {
                            match services::clear_cache(username).await {
                                Ok(message) => {
                                    feedback.set_status(message);
                                    refresh.bump_cache();
                                }
                                Err(error) => feedback.set_error(error),
                            }
                        });
                    },
                    "Clear cache"
                }
            }
            if let Some(message) = cache_error {
                article { class: "notice notice--error diagnostics-notice",
                    div {
                        strong { "Cache Error" }
                        p { "{message}" }
                    }
                }
            }
        }
    }
}
