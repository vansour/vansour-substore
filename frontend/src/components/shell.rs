use dioxus::prelude::*;

use crate::app::Route;

#[component]
pub fn AppShell(title: String, summary: String, children: Element) -> Element {
    rsx! {
        div { class: "shell-page",
            div { class: "ambient ambient--warm" }
            div { class: "ambient ambient--cool" }
            div { class: "shell",
                header { class: "masthead panel panel--hero",
                    div { class: "masthead-copy",
                        p { class: "eyebrow", "{submora_core::APP_NAME}" }
                        h1 { "{title}" }
                        p { class: "summary", "{summary}" }
                        div { class: "badge-row",
                            span { class: "badge badge--accent", "Phase {submora_core::CURRENT_PHASE}" }
                            span { class: "badge", "Dioxus 0.7.3" }
                            span { class: "badge", "Axum 0.8.8" }
                        }
                        nav { class: "route-nav",
                            Link { class: "route-link", to: Route::Dashboard {}, "Dashboard" }
                            Link { class: "route-link", to: Route::Login {}, "Login" }
                            Link { class: "route-link", to: Route::UserDetail { username: "demo".to_string() }, "User Detail" }
                            Link { class: "route-link", to: Route::Account {}, "Account" }
                        }
                    }
                    aside { class: "masthead-meta",
                        p { class: "meta-kicker", "Unified Runtime" }
                        p { class: "meta-copy",
                            "One Axum process now serves the management UI, API routes, assets, and "
                            code { {"/{username}".to_string()} }
                            " aggregation output."
                        }
                        ul { class: "meta-list",
                            li { "SQLite-backed sessions and sqlx migrations initialize the runtime state." }
                            li { "CSRF tokens guard mutating admin routes while login and public feeds use separate rate limiters." }
                            li { "Every public fetch stores per-link diagnostics for later inspection." }
                            li { "Default security headers are emitted for the management UI, APIs, assets, and public text routes." }
                        }
                    }
                }
                main { class: "content", {children} }
            }
        }
    }
}
