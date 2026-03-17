use submora_shared::{
    auth::{CurrentUserResponse, LoginRequest, UpdateAccountRequest},
    users::{
        CreateUserRequest, LinksPayload, UserCacheStatusResponse, UserDiagnosticsResponse,
        UserLinksResponse, UserOrderPayload, UserSummary,
    },
};

use crate::api;

pub async fn load_current_user() -> Result<Option<CurrentUserResponse>, String> {
    api::get_me().await
}

pub async fn load_users() -> Result<Vec<UserSummary>, String> {
    api::list_users().await
}

pub async fn load_links(username: Option<String>) -> Result<Option<UserLinksResponse>, String> {
    match username {
        Some(username) => api::get_links(&username).await.map(Some),
        None => Ok(None),
    }
}

pub async fn load_diagnostics(
    username: Option<String>,
) -> Result<Option<UserDiagnosticsResponse>, String> {
    match username {
        Some(username) => api::get_diagnostics(&username).await.map(Some),
        None => Ok(None),
    }
}

pub async fn load_cache_status(
    username: Option<String>,
) -> Result<Option<UserCacheStatusResponse>, String> {
    match username {
        Some(username) => api::get_cache_status(&username).await.map(Some),
        None => Ok(None),
    }
}

pub async fn login(username: String, password: String) -> Result<String, String> {
    api::login(&LoginRequest { username, password })
        .await
        .map(|message| message.message)
}

pub async fn logout() -> Result<String, String> {
    api::logout().await.map(|message| message.message)
}

pub async fn create_user(username: String) -> Result<UserSummary, String> {
    api::create_user(&CreateUserRequest { username }).await
}

pub async fn delete_user(username: String) -> Result<String, String> {
    api::delete_user(&username)
        .await
        .map(|message| message.message)
}

pub async fn save_links(username: String, links_text: String) -> Result<UserLinksResponse, String> {
    let payload = LinksPayload {
        links: parse_links(&links_text),
    };
    api::set_links(&username, &payload).await
}

pub async fn refresh_cache(username: String) -> Result<UserCacheStatusResponse, String> {
    api::refresh_cache(&username).await
}

pub async fn clear_cache(username: String) -> Result<String, String> {
    api::clear_cache(&username)
        .await
        .map(|message| message.message)
}

pub async fn update_account(
    current_username: String,
    account_username: String,
    current_password: String,
    new_password: String,
) -> Result<String, String> {
    let new_username = if account_username.trim().is_empty() {
        current_username
    } else {
        account_username
    };

    api::update_account(&UpdateAccountRequest {
        current_password: Some(current_password),
        new_username,
        new_password,
    })
    .await
    .map(|message| message.message)
}

pub async fn set_order(order: Vec<String>) -> Result<Vec<String>, String> {
    api::set_order(&UserOrderPayload { order }).await
}

pub fn parse_links(links_text: &str) -> Vec<String> {
    links_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub fn count_links(links_text: &str) -> usize {
    parse_links(links_text).len()
}
