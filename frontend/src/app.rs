use dioxus::prelude::*;

use crate::components::{console::AdminConsole, shell::AppShell};

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Routable, Clone, Debug, PartialEq)]
pub enum Route {
    #[route("/")]
    Dashboard {},
    #[route("/login")]
    Login {},
    #[route("/users/:username")]
    UserDetail { username: String },
    #[route("/account")]
    Account {},
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

#[component]
pub fn App() -> Element {
    rsx! {
        Router::<Route> {}
    }
}

#[component]
fn Dashboard() -> Element {
    rsx! { AdminConsole { mode: "dashboard", initial_user: None } }
}

#[component]
fn Login() -> Element {
    rsx! { AdminConsole { mode: "login", initial_user: None } }
}

#[component]
fn UserDetail(username: String) -> Element {
    rsx! { AdminConsole { mode: "user", initial_user: Some(username) } }
}

#[component]
fn Account() -> Element {
    rsx! { AdminConsole { mode: "account", initial_user: None } }
}

#[component]
fn NotFound(segments: Vec<String>) -> Element {
    let route = format!("/{}", segments.join("/"));

    rsx! {
        AppShell {
            title: "Not Found".to_string(),
            summary: format!("No Dioxus route is registered for {route}."),
            article { class: "panel",
                div { class: "section-head",
                    div {
                        h2 { "Route handled elsewhere" }
                        p { class: "muted", "The admin console owns only the explicit Dioxus pages." }
                    }
                    span { class: "tag", "{route}" }
                }
                p { class: "panel-copy",
                    "The public subscription route "
                    code { {"/{username}".to_string()} }
                    " remains owned by Axum so merged text responses bypass the Dioxus router."
                }
            }
        }
    }
}
