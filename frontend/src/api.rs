#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;

    use gloo_net::http::{Request, RequestBuilder, Response};
    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use submora_shared::{
        api::{ApiErrorBody, ApiMessage},
        auth::{CsrfTokenResponse, CurrentUserResponse, LoginRequest, UpdateAccountRequest},
        users::{
            CreateUserRequest, LinksPayload, UserCacheStatusResponse, UserDiagnosticsResponse,
            UserLinksResponse, UserOrderPayload, UserSummary,
        },
    };
    use web_sys::RequestCredentials;

    const CSRF_HEADER: &str = "x-csrf-token";

    std::thread_local! {
        static CSRF_TOKEN_CACHE: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    fn clear_csrf_cache() {
        CSRF_TOKEN_CACHE.with(|cache| cache.replace(None));
    }

    fn cached_csrf_token() -> Option<String> {
        CSRF_TOKEN_CACHE.with(|cache| cache.borrow().clone())
    }

    async fn parse_error(response: Response) -> String {
        let fallback = format!("request failed with status {}", response.status());
        match response.json::<ApiErrorBody>().await {
            Ok(body) => body.message,
            Err(_) => fallback,
        }
    }

    async fn parse_json<T: DeserializeOwned>(response: Response) -> Result<T, String> {
        if response.ok() {
            response
                .json::<T>()
                .await
                .map_err(|error| error.to_string())
        } else {
            Err(parse_error(response).await)
        }
    }

    #[allow(dead_code)]
    async fn parse_text(response: Response) -> Result<String, String> {
        if response.ok() {
            response.text().await.map_err(|error| error.to_string())
        } else {
            Err(parse_error(response).await)
        }
    }

    fn request_builder(method: &str, url: &str) -> Result<RequestBuilder, String> {
        match method {
            "GET" => Ok(Request::get(url)),
            "POST" => Ok(Request::post(url)),
            "PUT" => Ok(Request::put(url)),
            "DELETE" => Ok(Request::delete(url)),
            _ => Err(format!("unsupported method: {method}")),
        }
    }

    async fn send_request(method: &str, url: &str) -> Result<Response, String> {
        request_builder(method, url)?
            .credentials(RequestCredentials::Include)
            .send()
            .await
            .map_err(|error| error.to_string())
    }

    async fn fetch_csrf_token() -> Result<String, String> {
        let response = send_request("GET", "/api/auth/csrf").await?;
        let payload: CsrfTokenResponse = parse_json(response).await?;
        CSRF_TOKEN_CACHE.with(|cache| cache.replace(Some(payload.token.clone())));
        Ok(payload.token)
    }

    async fn csrf_token() -> Result<String, String> {
        if let Some(token) = cached_csrf_token() {
            return Ok(token);
        }

        fetch_csrf_token().await
    }

    async fn send_with_csrf(method: &str, url: &str) -> Result<Response, String> {
        let token = csrf_token().await?;
        request_builder(method, url)?
            .credentials(RequestCredentials::Include)
            .header(CSRF_HEADER, &token)
            .send()
            .await
            .map_err(|error| error.to_string())
    }

    async fn send_json_with_csrf<B: Serialize>(
        method: &str,
        url: &str,
        body: &B,
    ) -> Result<Response, String> {
        let token = csrf_token().await?;
        request_builder(method, url)?
            .credentials(RequestCredentials::Include)
            .header(CSRF_HEADER, &token)
            .json(body)
            .map_err(|error| error.to_string())?
            .send()
            .await
            .map_err(|error| error.to_string())
    }

    async fn send_json<T: DeserializeOwned, B: Serialize>(
        method: &str,
        url: &str,
        body: &B,
    ) -> Result<T, String> {
        let response = send_json_with_csrf(method, url, body).await?;
        if response.status() == 403 {
            clear_csrf_cache();
            return parse_json(send_json_with_csrf(method, url, body).await?).await;
        }

        parse_json(response).await
    }

    async fn send_without_body<T: DeserializeOwned>(method: &str, url: &str) -> Result<T, String> {
        if matches!(method, "GET" | "HEAD") {
            return parse_json(send_request(method, url).await?).await;
        }

        let response = send_with_csrf(method, url).await?;
        if response.status() == 403 {
            clear_csrf_cache();
            return parse_json(send_with_csrf(method, url).await?).await;
        }

        parse_json(response).await
    }

    pub async fn get_me() -> Result<Option<CurrentUserResponse>, String> {
        let response = Request::get("/api/auth/me")
            .credentials(RequestCredentials::Include)
            .send()
            .await
            .map_err(|error| error.to_string())?;

        if response.status() == 401 {
            clear_csrf_cache();
            return Ok(None);
        }

        parse_json(response).await.map(Some)
    }

    pub async fn login(payload: &LoginRequest) -> Result<ApiMessage, String> {
        send_json("POST", "/api/auth/login", payload).await
    }

    pub async fn logout() -> Result<ApiMessage, String> {
        let response = send_without_body("POST", "/api/auth/logout").await;
        if response.is_ok() {
            clear_csrf_cache();
        }
        response
    }

    pub async fn update_account(payload: &UpdateAccountRequest) -> Result<ApiMessage, String> {
        let response = send_json("PUT", "/api/auth/account", payload).await;
        if response.is_ok() {
            clear_csrf_cache();
        }
        response
    }

    pub async fn list_users() -> Result<Vec<UserSummary>, String> {
        send_without_body("GET", "/api/users").await
    }

    pub async fn create_user(payload: &CreateUserRequest) -> Result<UserSummary, String> {
        send_json("POST", "/api/users", payload).await
    }

    pub async fn delete_user(username: &str) -> Result<ApiMessage, String> {
        send_without_body("DELETE", &format!("/api/users/{username}")).await
    }

    pub async fn get_links(username: &str) -> Result<UserLinksResponse, String> {
        send_without_body("GET", &format!("/api/users/{username}/links")).await
    }

    pub async fn set_links(
        username: &str,
        payload: &LinksPayload,
    ) -> Result<UserLinksResponse, String> {
        send_json("PUT", &format!("/api/users/{username}/links"), payload).await
    }

    pub async fn get_diagnostics(username: &str) -> Result<UserDiagnosticsResponse, String> {
        send_without_body("GET", &format!("/api/users/{username}/diagnostics")).await
    }

    pub async fn get_cache_status(username: &str) -> Result<UserCacheStatusResponse, String> {
        send_without_body("GET", &format!("/api/users/{username}/cache")).await
    }

    pub async fn refresh_cache(username: &str) -> Result<UserCacheStatusResponse, String> {
        send_without_body("POST", &format!("/api/users/{username}/cache/refresh")).await
    }

    pub async fn clear_cache(username: &str) -> Result<ApiMessage, String> {
        send_without_body("DELETE", &format!("/api/users/{username}/cache")).await
    }

    pub async fn set_order(payload: &UserOrderPayload) -> Result<Vec<String>, String> {
        send_json("PUT", "/api/users/order", payload).await
    }

    #[allow(dead_code)]
    pub async fn run_public_route(username: &str) -> Result<String, String> {
        let response = send_request("GET", &format!("/{username}")).await?;
        parse_text(response).await
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use submora_shared::{
        api::ApiMessage,
        auth::{CurrentUserResponse, LoginRequest, UpdateAccountRequest},
        users::{
            CreateUserRequest, LinksPayload, UserCacheStatusResponse, UserDiagnosticsResponse,
            UserLinksResponse, UserOrderPayload, UserSummary,
        },
    };

    fn unavailable() -> String {
        "web api client is only available on wasm32".to_string()
    }

    pub async fn get_me() -> Result<Option<CurrentUserResponse>, String> {
        Err(unavailable())
    }

    pub async fn login(_payload: &LoginRequest) -> Result<ApiMessage, String> {
        Err(unavailable())
    }

    pub async fn logout() -> Result<ApiMessage, String> {
        Err(unavailable())
    }

    pub async fn update_account(_payload: &UpdateAccountRequest) -> Result<ApiMessage, String> {
        Err(unavailable())
    }

    pub async fn list_users() -> Result<Vec<UserSummary>, String> {
        Err(unavailable())
    }

    pub async fn create_user(_payload: &CreateUserRequest) -> Result<UserSummary, String> {
        Err(unavailable())
    }

    pub async fn delete_user(_username: &str) -> Result<ApiMessage, String> {
        Err(unavailable())
    }

    pub async fn get_links(_username: &str) -> Result<UserLinksResponse, String> {
        Err(unavailable())
    }

    pub async fn set_links(
        _username: &str,
        _payload: &LinksPayload,
    ) -> Result<UserLinksResponse, String> {
        Err(unavailable())
    }

    pub async fn get_diagnostics(_username: &str) -> Result<UserDiagnosticsResponse, String> {
        Err(unavailable())
    }

    pub async fn get_cache_status(_username: &str) -> Result<UserCacheStatusResponse, String> {
        Err(unavailable())
    }

    pub async fn refresh_cache(_username: &str) -> Result<UserCacheStatusResponse, String> {
        Err(unavailable())
    }

    pub async fn clear_cache(_username: &str) -> Result<ApiMessage, String> {
        Err(unavailable())
    }

    pub async fn set_order(_payload: &UserOrderPayload) -> Result<Vec<String>, String> {
        Err(unavailable())
    }

    #[allow(dead_code)]
    pub async fn run_public_route(_username: &str) -> Result<String, String> {
        Err(unavailable())
    }
}

pub use imp::*;
