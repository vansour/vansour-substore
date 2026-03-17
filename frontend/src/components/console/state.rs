use dioxus::prelude::*;
use submora_shared::{
    auth::CurrentUserResponse,
    users::{UserCacheStatusResponse, UserDiagnosticsResponse, UserLinksResponse, UserSummary},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::services;

#[derive(Clone, Copy, PartialEq)]
pub struct FeedbackSignals {
    pub status_message: Signal<Option<String>>,
    pub error_message: Signal<Option<String>>,
}

impl FeedbackSignals {
    pub fn clear(mut self) {
        self.error_message.set(None);
        self.status_message.set(None);
    }

    pub fn set_status(mut self, message: impl Into<String>) {
        self.status_message.set(Some(message.into()));
    }

    pub fn set_error(mut self, message: String) {
        self.error_message.set(Some(message));
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RefreshState {
    pub auth: Signal<u32>,
    pub users: Signal<u32>,
    pub links: Signal<u32>,
    pub diagnostics: Signal<u32>,
    pub cache: Signal<u32>,
}

impl RefreshState {
    pub fn bump_auth(mut self) {
        self.auth.set((self.auth)() + 1);
    }

    pub fn bump_users(mut self) {
        self.users.set((self.users)() + 1);
    }

    pub fn bump_links(mut self) {
        self.links.set((self.links)() + 1);
    }

    pub fn bump_diagnostics(mut self) {
        self.diagnostics.set((self.diagnostics)() + 1);
    }

    pub fn bump_cache(mut self) {
        self.cache.set((self.cache)() + 1);
    }

    pub fn bump_selected_data(self) {
        self.bump_links();
        self.bump_diagnostics();
        self.bump_cache();
    }
}

#[derive(Clone)]
pub struct ConsoleResources {
    pub auth_resource: Resource<Result<Option<CurrentUserResponse>, String>>,
    pub users_resource: Resource<Result<Vec<UserSummary>, String>>,
    pub links_resource: Resource<Result<Option<UserLinksResponse>, String>>,
    pub diagnostics_resource: Resource<Result<Option<UserDiagnosticsResponse>, String>>,
    pub cache_resource: Resource<Result<Option<UserCacheStatusResponse>, String>>,
}

#[derive(Clone, PartialEq)]
pub struct CacheDisplay {
    pub state: String,
    pub line_count: u32,
    pub body_bytes: u64,
    pub generated_at: String,
    pub expires_at: String,
}

impl CacheDisplay {
    pub fn from_status(status: Option<&UserCacheStatusResponse>) -> Self {
        Self {
            state: status
                .map(|status| status.state.clone())
                .unwrap_or_else(|| "empty".to_string()),
            line_count: status.map(|status| status.line_count).unwrap_or_default(),
            body_bytes: status.map(|status| status.body_bytes).unwrap_or_default(),
            generated_at: format_timestamp(
                status.and_then(|status| status.generated_at),
                "not built",
            ),
            expires_at: format_timestamp(status.and_then(|status| status.expires_at), "n/a"),
        }
    }

    pub fn state_class(&self) -> &'static str {
        match self.state.as_str() {
            "fresh" => "tag--success",
            "expired" | "stale" => "tag--danger",
            _ => "tag--cool",
        }
    }
}

pub fn use_feedback_signals() -> FeedbackSignals {
    FeedbackSignals {
        status_message: use_signal(|| None::<String>),
        error_message: use_signal(|| None::<String>),
    }
}

pub fn use_refresh_state() -> RefreshState {
    RefreshState {
        auth: use_signal(|| 0u32),
        users: use_signal(|| 0u32),
        links: use_signal(|| 0u32),
        diagnostics: use_signal(|| 0u32),
        cache: use_signal(|| 0u32),
    }
}

pub fn use_console_resources(
    selected_username: Signal<Option<String>>,
    refresh: RefreshState,
) -> ConsoleResources {
    let auth_resource = use_resource(move || async move {
        let _ = (refresh.auth)();
        services::load_current_user().await
    });
    let users_resource = use_resource(move || async move {
        let _ = (refresh.users)();
        services::load_users().await
    });
    let links_resource = use_resource(move || async move {
        let _ = (refresh.links)();
        services::load_links(selected_username()).await
    });
    let diagnostics_resource = use_resource(move || async move {
        let _ = (refresh.diagnostics)();
        services::load_diagnostics(selected_username()).await
    });
    let cache_resource = use_resource(move || async move {
        let _ = (refresh.cache)();
        services::load_cache_status(selected_username()).await
    });

    ConsoleResources {
        auth_resource,
        users_resource,
        links_resource,
        diagnostics_resource,
        cache_resource,
    }
}

pub fn sync_links_text(
    mut links_text: Signal<String>,
    links_resource: Resource<Result<Option<UserLinksResponse>, String>>,
) {
    use_effect(move || {
        if let Some(Ok(Some(UserLinksResponse { links, .. }))) = &*links_resource.read_unchecked() {
            links_text.set(links.join("\n"));
        }
    });
}

pub fn current_user_snapshot(
    resource: &Resource<Result<Option<CurrentUserResponse>, String>>,
) -> Option<CurrentUserResponse> {
    match &*resource.read_unchecked() {
        Some(Ok(Some(user))) => Some(user.clone()),
        _ => None,
    }
}

pub fn users_snapshot(
    resource: &Resource<Result<Vec<UserSummary>, String>>,
) -> Option<Vec<UserSummary>> {
    match &*resource.read_unchecked() {
        Some(Ok(users)) => Some(users.clone()),
        _ => None,
    }
}

pub fn format_timestamp(value: Option<i64>, empty: &str) -> String {
    match value {
        Some(value) => match OffsetDateTime::from_unix_timestamp(value) {
            Ok(timestamp) => timestamp
                .format(&Rfc3339)
                .map(|formatted| format!("{formatted} ({value})"))
                .unwrap_or_else(|_| value.to_string()),
            Err(_) => value.to_string(),
        },
        None => empty.to_string(),
    }
}
