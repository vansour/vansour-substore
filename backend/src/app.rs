use std::{fs, sync::Arc};

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, header},
    response::{Html, Response},
    routing::{delete, get, post, put},
};
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

use crate::{
    routes::{auth, public, users},
    security,
    state::AppState,
};

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = state
        .config
        .cors_allow_origin
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .fold(
            CorsLayer::new()
                .allow_credentials(true)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([
                    header::ACCEPT,
                    header::CONTENT_TYPE,
                    HeaderName::from_static(security::CSRF_HEADER),
                ]),
            |layer, origin| layer.allow_origin(origin),
        );

    Router::new()
        .nest_service(
            "/assets",
            ServeDir::new(state.config.web_dist_dir.join("assets")),
        )
        .route("/", get(index))
        .route("/login", get(index))
        .route("/account", get(index))
        .route("/users/{username}", get(index))
        .route("/healthz", get(public::healthz))
        .route("/api/meta/app", get(public::app_info))
        .route("/api/auth/csrf", get(auth::csrf_token))
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/account", put(auth::update_account))
        .route(
            "/api/users",
            get(users::list_users).post(users::create_user),
        )
        .route("/api/users/order", put(users::set_order))
        .route("/api/users/{username}", delete(users::delete_user))
        .route(
            "/api/users/{username}/links",
            get(users::get_links).put(users::set_links),
        )
        .route(
            "/api/users/{username}/diagnostics",
            get(users::get_diagnostics),
        )
        .route(
            "/api/users/{username}/cache",
            get(users::get_cache_status).delete(users::clear_cache),
        )
        .route(
            "/api/users/{username}/cache/refresh",
            post(users::refresh_cache),
        )
        .route("/{username}", get(public::merged_user))
        .layer(axum::middleware::map_response(security_headers))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn index(axum::extract::State(state): axum::extract::State<Arc<AppState>>) -> Html<String> {
    let built_index = state.config.web_dist_dir.join("index.html");
    if let Ok(contents) = fs::read_to_string(&built_index) {
        return Html(contents);
    }

    Html(format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{name} phase {phase}</title>
    <style>
      body {{ font-family: ui-sans-serif, system-ui, sans-serif; margin: 0; background: #f5f7fb; color: #162033; }}
      main {{ max-width: 860px; margin: 64px auto; padding: 0 24px; }}
      section {{ background: white; border-radius: 18px; padding: 28px; box-shadow: 0 12px 32px rgba(22,32,51,.08); }}
      code {{ background: #eef3ff; padding: 2px 6px; border-radius: 6px; }}
      a {{ color: #145af2; text-decoration: none; }}
      ul {{ line-height: 1.8; }}
    </style>
  </head>
  <body>
    <main>
      <section>
        <h1>{name} phase {phase} backend is running</h1>
        <p>The server now provides the phase 11 rewrite runtime, serves the built Dioxus console by default, adds default security response headers, rate-limits public feed requests separately from login, and keeps the earlier CSRF, proxy trust, and SSRF protections in place.</p>
        <ul>
          <li><code>GET /healthz</code></li>
          <li><code>GET /api/meta/app</code></li>
          <li><code>GET /api/auth/csrf</code></li>
          <li><code>POST /api/auth/login</code></li>
          <li><code>GET /api/users</code></li>
          <li><code>GET /api/users/{{username}}/diagnostics</code></li>
          <li><code>GET /api/users/{{username}}/cache</code></li>
          <li><code>GET /{{username}}</code></li>
        </ul>
        <p>If the Dioxus frontend has been built into <code>{dist}</code>, this route will serve it automatically.</p>
      </section>
    </main>
  </body>
</html>"#,
        name = submora_core::APP_NAME,
        phase = submora_core::CURRENT_PHASE,
        dist = state.config.web_dist_dir.display(),
    ))
}

async fn security_headers(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    response
}
