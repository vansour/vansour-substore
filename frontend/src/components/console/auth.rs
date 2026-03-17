use dioxus::prelude::*;

use crate::{app::Route, components::console::services};

use super::state::{FeedbackSignals, RefreshState};

#[component]
pub fn ControlPlanePanel(
    username: String,
    mut selected_username: Signal<Option<String>>,
    mut links_text: Signal<String>,
    feedback: FeedbackSignals,
    refresh: RefreshState,
) -> Element {
    let selected = selected_username();

    rsx! {
        article { class: "panel panel--hero",
            div { class: "section-head",
                div {
                    h2 { "Control Plane" }
                    p { class: "muted", "Configuration changes land in SQLite immediately and the public routes reflect them on the next request." }
                }
                span { class: "tag tag--accent", "Signed in as {username}" }
            }
            p { class: "panel-copy",
                "The rewrite now protects every mutating endpoint with CSRF verification, persists schema changes through SQL migrations, records the last fetch outcome for each saved source, and reuses merged cache snapshots from SQLite."
            }
            div { class: "button-row",
                Link { class: "button button--ghost", to: Route::Dashboard {}, "Dashboard" }
                Link { class: "button button--ghost", to: Route::Account {}, "Account" }
                if let Some(selected_username_value) = selected {
                    Link {
                        class: "button button--ghost",
                        to: Route::UserDetail { username: selected_username_value },
                        "Selected user"
                    }
                }
                button {
                    class: "button button--danger",
                    onclick: move |_| {
                        feedback.clear();
                        spawn(async move {
                            match services::logout().await {
                                Ok(message) => {
                                    feedback.set_status(message);
                                    selected_username.set(None);
                                    links_text.set(String::new());
                                    refresh.bump_auth();
                                    refresh.bump_users();
                                    refresh.bump_selected_data();
                                }
                                Err(error) => feedback.set_error(error),
                            }
                        });
                    },
                    "Logout"
                }
            }
        }
    }
}

#[component]
pub fn LoginPanel(
    mut login_username: Signal<String>,
    mut login_password: Signal<String>,
    feedback: FeedbackSignals,
    refresh: RefreshState,
) -> Element {
    rsx! {
        article { class: "panel panel--hero auth-panel",
            div { class: "section-head",
                div {
                    h2 { "Login" }
                    p { class: "muted", "Use the bootstrap administrator credentials or the rotated account from your previous migration pass." }
                }
                span { class: "tag", "Cookie Session" }
            }
            p { class: "panel-copy",
                "The client fetches a CSRF token before login, stores the session cookie with browser credentials enabled, and reuses that token for every protected mutation."
            }
            form {
                class: "form-stack",
                onsubmit: move |event| {
                    event.prevent_default();
                    feedback.clear();
                    let username = login_username();
                    let password = login_password();
                    spawn(async move {
                        match services::login(username, password).await {
                            Ok(message) => {
                                feedback.set_status(message);
                                login_password.set(String::new());
                                refresh.bump_auth();
                                refresh.bump_users();
                                refresh.bump_cache();
                            }
                            Err(error) => feedback.set_error(error),
                        }
                    });
                },
                div { class: "field-grid",
                    label { class: "field",
                        span { "Username" }
                        input {
                            value: "{login_username()}",
                            oninput: move |event| login_username.set(event.value()),
                            placeholder: "admin"
                        }
                    }
                    label { class: "field",
                        span { "Password" }
                        input {
                            r#type: "password",
                            value: "{login_password()}",
                            oninput: move |event| login_password.set(event.value()),
                            placeholder: "••••••••"
                        }
                    }
                }
                button { class: "button button--primary button--wide", r#type: "submit", "Login" }
            }
        }
    }
}

#[component]
pub fn AccountPanel(
    mut account_username: Signal<String>,
    mut account_current_password: Signal<String>,
    mut account_new_password: Signal<String>,
    current_username: String,
    feedback: FeedbackSignals,
    refresh: RefreshState,
) -> Element {
    let current_username_for_submit = current_username.clone();
    let current_username_placeholder = current_username.clone();

    rsx! {
        article { class: "panel panel--accent",
            div { class: "section-head",
                div {
                    h2 { "Administrator Account" }
                    p { class: "muted", "Changing credentials flushes the current session and forces a fresh login." }
                }
                span { class: "tag", "{current_username}" }
            }
            p { class: "panel-copy",
                "The account endpoint now keeps the earlier password verification and also shares the same CSRF guard as the rest of the management surface."
            }
            form {
                class: "form-stack",
                onsubmit: move |event| {
                    event.prevent_default();
                    feedback.clear();
                    let username = current_username_for_submit.clone();
                    let next_username = account_username();
                    let current_password = account_current_password();
                    let new_password = account_new_password();
                    spawn(async move {
                        match services::update_account(
                            username,
                            next_username,
                            current_password,
                            new_password,
                        )
                        .await
                        {
                            Ok(message) => {
                                feedback.set_status(message);
                                account_username.set(String::new());
                                account_current_password.set(String::new());
                                account_new_password.set(String::new());
                                refresh.bump_auth();
                                refresh.bump_users();
                                refresh.bump_selected_data();
                            }
                            Err(error) => feedback.set_error(error),
                        }
                    });
                },
                div { class: "field-grid",
                    label { class: "field",
                        span { "New username" }
                        input {
                            value: "{account_username()}",
                            oninput: move |event| account_username.set(event.value()),
                            placeholder: current_username_placeholder.clone()
                        }
                    }
                    label { class: "field",
                        span { "Current password" }
                        input {
                            r#type: "password",
                            value: "{account_current_password()}",
                            oninput: move |event| account_current_password.set(event.value()),
                            placeholder: "required"
                        }
                    }
                }
                label { class: "field",
                    span { "New password" }
                    input {
                        r#type: "password",
                        value: "{account_new_password()}",
                        oninput: move |event| account_new_password.set(event.value()),
                        placeholder: "letters + numbers + symbols"
                    }
                }
                button { class: "button button--primary", r#type: "submit", "Update account" }
            }
        }
    }
}
