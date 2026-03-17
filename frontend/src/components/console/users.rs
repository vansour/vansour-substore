use dioxus::prelude::*;
use submora_shared::users::UserSummary;

use crate::app::Route;

use super::{
    services,
    state::{FeedbackSignals, RefreshState},
};

#[component]
pub fn UsersPanel(
    mut create_username: Signal<String>,
    users: Option<Vec<UserSummary>>,
    mut selected_username: Signal<Option<String>>,
    feedback: FeedbackSignals,
    refresh: RefreshState,
) -> Element {
    let user_list = users.clone().unwrap_or_default();
    let user_count = user_list.len();
    let selected = selected_username();

    rsx! {
        article { class: "panel",
            div { class: "section-head",
                div {
                    h2 { "Users" }
                    p { class: "muted", "Create, reorder, and jump into per-user source editing." }
                }
                span { class: "tag", "{user_count} total" }
            }
            form {
                class: "inline-form",
                onsubmit: move |event| {
                    event.prevent_default();
                    feedback.clear();
                    let username = create_username();
                    spawn(async move {
                        match services::create_user(username).await {
                            Ok(user) => {
                                feedback.set_status(format!("Created {}", user.username));
                                create_username.set(String::new());
                                selected_username.set(Some(user.username.clone()));
                                refresh.bump_users();
                                refresh.bump_selected_data();
                            }
                            Err(error) => feedback.set_error(error),
                        }
                    });
                },
                label { class: "field field--inline",
                    span { "Create user" }
                    input {
                        value: "{create_username()}",
                        oninput: move |event| create_username.set(event.value()),
                        placeholder: "new-user"
                    }
                }
                button { class: "button button--primary", r#type: "submit", "Create" }
            }
            if users.is_some() {
                if user_list.is_empty() {
                    div { class: "empty-state",
                        strong { "No managed users yet" }
                        p { "Create the first user to open a public aggregation route and assign source links." }
                    }
                } else {
                    div { class: "user-list",
                        for (index, user) in user_list.clone().into_iter().enumerate() {
                            UserRow {
                                key: "{user.username}",
                                index,
                                user,
                                users: user_list.clone(),
                                selected: selected.clone(),
                                selected_username,
                                feedback,
                                refresh,
                            }
                        }
                    }
                }
            } else {
                p { class: "muted", "Loading users..." }
            }
        }
    }
}

#[component]
fn UserRow(
    index: usize,
    user: UserSummary,
    users: Vec<UserSummary>,
    selected: Option<String>,
    mut selected_username: Signal<Option<String>>,
    feedback: FeedbackSignals,
    refresh: RefreshState,
) -> Element {
    let is_selected = selected.as_deref() == Some(user.username.as_str());
    let username = user.username.clone();
    let username_for_link = username.clone();
    let username_for_select = username.clone();
    let username_for_up = username.clone();
    let username_for_down = username.clone();
    let order_source_for_up = users.clone();
    let order_source_for_down = users.clone();
    let can_move_up = index > 0;
    let can_move_down = index + 1 < users.len();
    let card_class = if is_selected {
        "user-card user-card--selected"
    } else {
        "user-card"
    };

    rsx! {
        article { class: "{card_class}",
            div { class: "user-card-head",
                div {
                    strong { "{user.username}" }
                    p { class: "muted", "Route " code { "/{user.username}" } }
                }
                span { class: "tag", "#{index + 1}" }
            }
            div { class: "button-row",
                Link {
                    class: "button button--ghost button--compact",
                    to: Route::UserDetail { username: username_for_link },
                    "Open"
                }
                button {
                    class: "button button--ghost button--compact",
                    r#type: "button",
                    onclick: move |_| selected_username.set(Some(username_for_select.clone())),
                    "Edit"
                }
                button {
                    class: "button button--ghost button--compact",
                    r#type: "button",
                    disabled: !can_move_up,
                    onclick: move |_| {
                        let mut order = order_source_for_up
                            .iter()
                            .map(|item| item.username.clone())
                            .collect::<Vec<_>>();
                        if let Some(position) = order.iter().position(|item| item == &username_for_up)
                            && position > 0 {
                                order.swap(position, position - 1);
                                feedback.clear();
                                spawn(async move {
                                    match services::set_order(order).await {
                                        Ok(_) => {
                                            feedback.set_status("Updated user order");
                                            refresh.bump_users();
                                        }
                                        Err(error) => feedback.set_error(error),
                                    }
                                });
                            }
                    },
                    "Up"
                }
                button {
                    class: "button button--ghost button--compact",
                    r#type: "button",
                    disabled: !can_move_down,
                    onclick: move |_| {
                        let mut order = order_source_for_down
                            .iter()
                            .map(|item| item.username.clone())
                            .collect::<Vec<_>>();
                        if let Some(position) = order.iter().position(|item| item == &username_for_down)
                            && position + 1 < order.len() {
                                order.swap(position, position + 1);
                                feedback.clear();
                                spawn(async move {
                                    match services::set_order(order).await {
                                        Ok(_) => {
                                            feedback.set_status("Updated user order");
                                            refresh.bump_users();
                                        }
                                        Err(error) => feedback.set_error(error),
                                    }
                                });
                            }
                    },
                    "Down"
                }
            }
        }
    }
}
