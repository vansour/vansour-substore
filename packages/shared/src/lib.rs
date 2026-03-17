pub mod api {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct ApiMessage {
        pub message: String,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct ApiErrorBody {
        pub error: String,
        pub message: String,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct AppInfoResponse {
        pub name: String,
        pub phase: u8,
        pub frontend: String,
        pub backend: String,
        pub version: String,
        pub web_dist_dir: String,
    }
}

pub mod auth {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct LoginRequest {
        pub username: String,
        pub password: String,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct UpdateAccountRequest {
        pub current_password: Option<String>,
        pub new_username: String,
        pub new_password: String,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct CurrentUserResponse {
        pub username: String,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct CsrfTokenResponse {
        pub token: String,
    }
}

pub mod users {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct UserSummary {
        pub username: String,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct CreateUserRequest {
        pub username: String,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct LinksPayload {
        pub links: Vec<String>,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct UserLinksResponse {
        pub username: String,
        pub links: Vec<String>,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct UserOrderPayload {
        pub order: Vec<String>,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct LinkDiagnostic {
        pub url: String,
        pub status: String,
        pub detail: Option<String>,
        pub http_status: Option<u16>,
        pub content_type: Option<String>,
        pub body_bytes: Option<u64>,
        pub redirect_count: u8,
        pub is_html: bool,
        pub fetched_at: Option<i64>,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct UserDiagnosticsResponse {
        pub username: String,
        pub diagnostics: Vec<LinkDiagnostic>,
    }

    #[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
    pub struct UserCacheStatusResponse {
        pub username: String,
        pub state: String,
        pub line_count: u32,
        pub body_bytes: u64,
        pub generated_at: Option<i64>,
        pub expires_at: Option<i64>,
    }
}
