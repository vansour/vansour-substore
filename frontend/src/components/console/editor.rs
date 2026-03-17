use dioxus::prelude::*;

use super::{
    services,
    state::{FeedbackSignals, RefreshState},
};

#[component]
pub fn EditorPanel(
    username: String,
    selected_route: String,
    mut links_text: Signal<String>,
    selected_link_count: usize,
    mut selected_username: Signal<Option<String>>,
    feedback: FeedbackSignals,
    refresh: RefreshState,
) -> Element {
    let mut links_error = use_signal(|| None::<String>);
    let username_for_save = username.clone();
    let username_for_delete = username.clone();
    let editor_class = if links_error().is_some() {
        "source-editor source-editor--error"
    } else {
        "source-editor"
    };

    {
        let username = username.clone();
        use_effect(move || {
            let _ = &username;
            links_error.set(None);
        });
    }

    rsx! {
        article { class: "panel panel--editor",
            div { class: "section-head",
                div {
                    h2 { "Sources" }
                    p { class: "muted", "Each non-empty line becomes one source. Order is preserved after deduplication." }
                }
                span { class: "tag", "{selected_link_count} lines" }
            }
            p { class: "panel-copy",
                "Editing "
                strong { "{username}" }
                ". Public route "
                code { "{selected_route}" }
            }
            p { class: "field-hint",
                "Unsafe targets such as loopback and private network addresses are rejected before links are saved."
            }
            textarea {
                class: "{editor_class}",
                value: "{links_text()}",
                oninput: move |event| {
                    links_error.set(None);
                    links_text.set(event.value());
                },
                rows: "16",
                placeholder: "https://example.com/feed\nhttps://news.example.org/article",
                aria_invalid: if links_error().is_some() { "true" } else { "false" }
            }
            if let Some(message) = links_error() {
                p { class: "field-error", "{message}" }
            }
            div { class: "button-row",
                button {
                    class: "button button--primary",
                    onclick: move |_| {
                        feedback.clear();
                        links_error.set(None);
                        let next_links = links_text();
                        let username = username_for_save.clone();
                        spawn(async move {
                            match services::save_links(username, next_links).await {
                                Ok(response) => {
                                    links_error.set(None);
                                    links_text.set(response.links.join("\n"));
                                    feedback.set_status(format!("Saved links for {}", response.username));
                                    refresh.bump_links();
                                    refresh.bump_diagnostics();
                                    refresh.bump_cache();
                                }
                                Err(error) => {
                                    if let Some(message) = extract_links_validation_error(&error) {
                                        links_error.set(Some(message));
                                    } else {
                                        feedback.set_error(error);
                                    }
                                }
                            }
                        });
                    },
                    "Save sources"
                }
                a {
                    class: "button button--ghost",
                    href: "{selected_route}",
                    target: "_blank",
                    rel: "noreferrer",
                    "Open public route"
                }
                button {
                    class: "button button--ghost",
                    onclick: move |_| refresh.bump_diagnostics(),
                    "Refresh diagnostics"
                }
                button {
                    class: "button button--danger",
                    onclick: move |_| {
                        feedback.clear();
                        let username = username_for_delete.clone();
                        spawn(async move {
                            match services::delete_user(username).await {
                                Ok(message) => {
                                    feedback.set_status(message);
                                    selected_username.set(None);
                                    links_text.set(String::new());
                                    refresh.bump_users();
                                    refresh.bump_selected_data();
                                }
                                Err(error) => feedback.set_error(error),
                            }
                        });
                    },
                    "Delete user"
                }
            }
        }
    }
}

fn extract_links_validation_error(message: &str) -> Option<String> {
    message
        .strip_prefix("links: ")
        .map(|detail| detail.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::extract_links_validation_error;

    #[test]
    fn extracts_links_validation_message() {
        assert_eq!(
            extract_links_validation_error("links: unsafe target: http://127.0.0.1/feed"),
            Some("unsafe target: http://127.0.0.1/feed".to_string())
        );
    }

    #[test]
    fn ignores_non_links_errors() {
        assert_eq!(
            extract_links_validation_error("request failed with status 500"),
            None
        );
    }
}
