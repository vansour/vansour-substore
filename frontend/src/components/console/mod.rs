mod auth;
mod cache;
mod diagnostics;
mod editor;
mod services;
mod state;
mod users;

use dioxus::prelude::*;

use crate::components::shell::AppShell;
use auth::{AccountPanel, ControlPlanePanel, LoginPanel};
use cache::CachePanel;
use diagnostics::DiagnosticsPanel;
use editor::EditorPanel;
use state::{
    CacheDisplay, current_user_snapshot, sync_links_text, use_console_resources,
    use_feedback_signals, use_refresh_state, users_snapshot,
};
use submora_shared::{
    auth::CurrentUserResponse,
    users::{UserCacheStatusResponse, UserDiagnosticsResponse},
};
use users::UsersPanel;

#[component]
pub fn AdminConsole(mode: &'static str, initial_user: Option<String>) -> Element {
    let login_username = use_signal(String::new);
    let login_password = use_signal(String::new);
    let create_username = use_signal(String::new);
    let selected_username = use_signal(|| initial_user.clone());
    let links_text = use_signal(String::new);
    let account_username = use_signal(String::new);
    let account_current_password = use_signal(String::new);
    let account_new_password = use_signal(String::new);

    let feedback = use_feedback_signals();
    let refresh = use_refresh_state();
    let resources = use_console_resources(selected_username, refresh);
    sync_links_text(links_text, resources.links_resource);

    let current_user = current_user_snapshot(&resources.auth_resource);
    let users = users_snapshot(&resources.users_resource);
    let cache_status = cache_status_snapshot(&resources.cache_resource);
    let cache_error = cache_error_snapshot(&resources.cache_resource);
    let diagnostics = diagnostics_snapshot(&resources.diagnostics_resource);
    let diagnostics_error = diagnostics_error_snapshot(&resources.diagnostics_resource);

    let user_list = users.clone().unwrap_or_default();
    let selected = selected_username();
    let current_username = current_user
        .clone()
        .map(|user| user.username)
        .unwrap_or_default();
    let user_count = user_list.len();
    let selected_display = selected
        .clone()
        .unwrap_or_else(|| "No active selection".to_string());
    let selected_route = selected
        .clone()
        .map(|username| format!("/{username}"))
        .unwrap_or_else(|| "/{username}".to_string());
    let selected_link_count = services::count_links(&links_text());
    let diagnostics_list = diagnostics
        .clone()
        .map(|payload| payload.diagnostics)
        .unwrap_or_default();
    let diagnostics_total = diagnostics_list.len();
    let diagnostics_success_count = diagnostics_list
        .iter()
        .filter(|diagnostic| diagnostic.status == "success")
        .count();
    let diagnostics_blocked_count = diagnostics_list
        .iter()
        .filter(|diagnostic| diagnostic.status == "blocked")
        .count();
    let diagnostics_pending_count = diagnostics_list
        .iter()
        .filter(|diagnostic| diagnostic.status == "pending")
        .count();
    let cache_display = CacheDisplay::from_status(cache_status.as_ref());
    let cache_state = cache_display.state.clone();
    let cache_line_count = cache_display.line_count;

    let title = match mode {
        "login" => "Session Login",
        "account" => "Administrator Account",
        "user" => "User Detail",
        _ => "Rewrite Dashboard",
    };
    let summary = match mode {
        "login" => "Authenticate with cookie sessions and a per-session CSRF token.",
        "account" => {
            "Rotate the administrator username or password through the guarded account endpoint."
        }
        "user" => {
            "Inspect one user, adjust ordered sources, and review cache plus per-link diagnostics."
        }
        _ => {
            "Phase 11 adds default security headers and a dedicated public-route rate limiter on top of the earlier proxy and SSRF hardening."
        }
    };
    let mode_label = match mode {
        "login" => "Login",
        "account" => "Account",
        "user" => "User Route",
        _ => "Dashboard",
    };

    rsx! {
        AppShell {
            title: title.to_string(),
            summary: summary.to_string(),
            if let Some(message) = (feedback.status_message)() {
                article { class: "notice notice--success",
                    div {
                        strong { "Saved" }
                        p { "{message}" }
                    }
                }
            }
            if let Some(message) = (feedback.error_message)() {
                article { class: "notice notice--error",
                    div {
                        strong { "Request Error" }
                        p { "{message}" }
                    }
                }
            }
            if let Some(CurrentUserResponse { username }) = current_user.clone() {
                ControlPlanePanel {
                    username,
                    selected_username,
                    links_text,
                    feedback,
                    refresh,
                }
                div { class: "stats-grid",
                    article { class: "stat-card",
                        span { class: "stat-kicker", "Mode" }
                        strong { class: "stat-value", "{mode_label}" }
                        p { class: "stat-note", "The shell adapts to login, account, detail, and dashboard routes." }
                    }
                    article { class: "stat-card",
                        span { class: "stat-kicker", "Users" }
                        strong { class: "stat-value", "{user_count}" }
                        p { class: "stat-note", "Sorted server-side and used for public aggregation order." }
                    }
                    article { class: "stat-card",
                        span { class: "stat-kicker", "Selection" }
                        strong { class: "stat-value", "{selected_display}" }
                        p { class: "stat-note", "Public route " code { "{selected_route}" } }
                    }
                    article { class: "stat-card",
                        span { class: "stat-kicker", "Sources" }
                        strong { class: "stat-value", "{selected_link_count}" }
                        p { class: "stat-note", "Duplicates collapse in order before persistence." }
                    }
                    article { class: "stat-card",
                        span { class: "stat-kicker", "Diagnostics" }
                        strong { class: "stat-value", "{diagnostics_success_count}/{diagnostics_total}" }
                        p { class: "stat-note", "Successful fetches tracked against the current ordered source list." }
                    }
                    article { class: "stat-card",
                        span { class: "stat-kicker", "Cache" }
                        strong { class: "stat-value", "{cache_state}" }
                        p { class: "stat-note", "{cache_line_count} merged lines currently stored for the selected user." }
                    }
                }
            } else {
                LoginPanel {
                    login_username,
                    login_password,
                    feedback,
                    refresh,
                }
            }
            if current_user.is_some() {
                div { class: "workspace-grid",
                    UsersPanel {
                        create_username,
                        users,
                        selected_username,
                        feedback,
                        refresh,
                    }
                    if let Some(selected_username_value) = selected.clone() {
                        div { class: "panel-stack",
                            EditorPanel {
                                username: selected_username_value.clone(),
                                selected_route: selected_route.clone(),
                                links_text,
                                selected_link_count,
                                selected_username,
                                feedback,
                                refresh,
                            }
                            CachePanel {
                                username: selected_username_value.clone(),
                                cache: cache_display.clone(),
                                cache_error,
                                feedback,
                                refresh,
                            }
                            DiagnosticsPanel {
                                diagnostics,
                                diagnostics_error,
                                success_count: diagnostics_success_count,
                                blocked_count: diagnostics_blocked_count,
                                pending_count: diagnostics_pending_count,
                            }
                        }
                    } else {
                        article { class: "panel panel--editor",
                            div { class: "empty-state",
                                strong { "Choose a user first" }
                                p { "Select a row from the user list to edit sources, then publish the merged route from the same runtime." }
                            }
                        }
                    }
                }
                AccountPanel {
                    account_username,
                    account_current_password,
                    account_new_password,
                    current_username,
                    feedback,
                    refresh,
                }
            }
        }
    }
}

fn cache_status_snapshot(
    resource: &Resource<Result<Option<UserCacheStatusResponse>, String>>,
) -> Option<UserCacheStatusResponse> {
    match &*resource.read_unchecked() {
        Some(Ok(Some(status))) => Some(status.clone()),
        _ => None,
    }
}

fn cache_error_snapshot(
    resource: &Resource<Result<Option<UserCacheStatusResponse>, String>>,
) -> Option<String> {
    match &*resource.read_unchecked() {
        Some(Err(error)) => Some(error.clone()),
        _ => None,
    }
}

fn diagnostics_snapshot(
    resource: &Resource<Result<Option<UserDiagnosticsResponse>, String>>,
) -> Option<UserDiagnosticsResponse> {
    match &*resource.read_unchecked() {
        Some(Ok(Some(diagnostics))) => Some(diagnostics.clone()),
        _ => None,
    }
}

fn diagnostics_error_snapshot(
    resource: &Resource<Result<Option<UserDiagnosticsResponse>, String>>,
) -> Option<String> {
    match &*resource.read_unchecked() {
        Some(Err(error)) => Some(error.clone()),
        _ => None,
    }
}
