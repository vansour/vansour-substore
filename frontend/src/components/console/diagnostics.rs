use dioxus::prelude::*;
use submora_shared::users::{LinkDiagnostic, UserDiagnosticsResponse};

use super::state::format_timestamp;

#[component]
pub fn DiagnosticsPanel(
    diagnostics: Option<UserDiagnosticsResponse>,
    diagnostics_error: Option<String>,
    success_count: usize,
    blocked_count: usize,
    pending_count: usize,
) -> Element {
    let diagnostics_list = diagnostics
        .clone()
        .map(|payload| payload.diagnostics)
        .unwrap_or_default();

    rsx! {
        article { class: "panel",
            div { class: "section-head",
                div {
                    h2 { "Fetch Diagnostics" }
                    p { class: "muted", "The server records the last fetch attempt for each saved source when the public route or cache refresh rebuilds the snapshot." }
                }
                div { class: "badge-row",
                    span { class: "tag", "{success_count} success" }
                    span { class: "tag", "{blocked_count} blocked" }
                    span { class: "tag", "{pending_count} pending" }
                }
            }
            if let Some(message) = diagnostics_error {
                article { class: "notice notice--error diagnostics-notice",
                    div {
                        strong { "Diagnostics Error" }
                        p { "{message}" }
                    }
                }
            } else if diagnostics.is_some() {
                if diagnostics_list.is_empty() {
                    div { class: "empty-state empty-state--compact",
                        strong { "No diagnostics yet" }
                        p { "Save one or more source links first. Diagnostics appear after the public aggregation route executes." }
                    }
                } else {
                    div { class: "diagnostics-list",
                        for diagnostic in diagnostics_list {
                            DiagnosticCard { diagnostic }
                        }
                    }
                }
            } else {
                p { class: "muted", "Loading diagnostics..." }
            }
        }
    }
}

#[component]
fn DiagnosticCard(diagnostic: LinkDiagnostic) -> Element {
    let status_class = diagnostic_status_class(&diagnostic.status);
    let detail = diagnostic
        .detail
        .clone()
        .unwrap_or_else(|| "No diagnostic detail recorded".to_string());
    let http_status = diagnostic
        .http_status
        .map(|value| format!("HTTP {value}"))
        .unwrap_or_else(|| "No status".to_string());
    let content_type = diagnostic
        .content_type
        .clone()
        .unwrap_or_else(|| "unknown content type".to_string());
    let body_bytes = diagnostic
        .body_bytes
        .map(|value| format!("{value} bytes"))
        .unwrap_or_else(|| "size unavailable".to_string());
    let fetched_at = format_timestamp(diagnostic.fetched_at, "not fetched yet");
    let body_kind = if diagnostic.is_html {
        "html normalized"
    } else {
        "plain text"
    };

    rsx! {
        article { class: "diagnostic-card",
            div { class: "diagnostic-card__head",
                code { class: "diagnostic-url", "{diagnostic.url}" }
                span { class: "diagnostic-status {status_class}", "{diagnostic.status}" }
            }
            p { class: "muted", "{detail}" }
            div { class: "diagnostic-meta",
                span { "{http_status}" }
                span { "{content_type}" }
                span { "{body_bytes}" }
                span { "{diagnostic.redirect_count} redirects" }
                span { "{body_kind}" }
                span { "Updated {fetched_at}" }
            }
        }
    }
}

fn diagnostic_status_class(status: &str) -> &'static str {
    match status {
        "success" => "diagnostic-status--success",
        "blocked" => "diagnostic-status--blocked",
        "pending" => "diagnostic-status--pending",
        _ => "diagnostic-status--error",
    }
}
