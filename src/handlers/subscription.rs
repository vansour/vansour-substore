//! 订阅处理器模块
//!
//! 处理订阅合并和获取，包含 SSRF 防护和 HTML 解析功能。

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    http::header,
};
use futures::stream::StreamExt;
use reqwest::Url;
use scraper::{Html, Node, ElementRef, Selector};
use sqlx::Row;
use std::net::ToSocketAddrs;
use std::sync::Arc;

use crate::error::AppError;
use crate::error::AppResult;
use crate::state::AppState;

/// 检查 URL 是否安全（SSRF 防护）
///
/// 验证 URL 是否会解析到私有或本地地址。
/// 阻止以下地址类型：
/// - 127.0.0.0/8（本地回环）
/// - 10.0.0.0/8、172.16.0.0/12、192.168.0.0/16（私有 IPv4）
/// - fc00::/7（唯一本地地址 IPv6）
/// - fe80::/10（链路本地 IPv6）
fn is_safe_url(url_str: &str) -> bool {
    let url = match Url::parse(url_str) {
        Ok(u) => u,
        Err(_) => return false,
    };

    let host = match url.host_str() {
        Some(h) => h,
        None => return false,
    };

    let port = url.port_or_known_default().unwrap_or(80);
    let socket_addrs = match (host, port).to_socket_addrs() {
        Ok(iter) => iter,
        Err(_) => return false,
    };

    for addr in socket_addrs {
        let ip = addr.ip();
        if ip.is_loopback() || ip.is_unspecified() {
            return false;
        }
        match ip {
            std::net::IpAddr::V4(ipv4) => {
                if ipv4.is_private() || ipv4.is_link_local() {
                    return false;
                }
            }
            std::net::IpAddr::V6(ipv6) => {
                if (ipv6.segments()[0] & 0xfe00) == 0xfc00 {
                    return false; // ULA
                }
                if (ipv6.segments()[0] & 0xffc0) == 0xfe80 {
                    return false; // Link-local
                }
            }
        }
    }

    true
}

/// 合并指定用户的订阅
///
/// 获取用户的所有订阅链接，并发请求并合并内容。
/// 如果响应是 HTML，会自动转换为纯文本。
pub async fn merged_user(
    Path(username): Path<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Response> {
    let row = sqlx::query("SELECT links FROM users WHERE username = $1")
        .bind(&username)
        .fetch_optional(&state.db)
        .await?;

    let links: Vec<String> = match row {
        Some(r) => {
            let val: serde_json::Value = r.get("links");
            serde_json::from_value(val).unwrap_or_default()
        }
        None => return Err(AppError::NotFound("user not found".into())),
    };

    if links.is_empty() {
        return Ok((
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "",
        ).into_response());
    }

    let concurrent_limit = state.fetch_config.concurrent_limit;

    let fetches = futures::stream::iter(links.into_iter().enumerate().map(|(idx, link)| {
        let client = state.client.clone();
        async move {
            if !is_safe_url(&link) {
                return (idx, format!("<!-- blocked unsafe url: {} -->", link), false);
            }

            let (body, is_html) = match client.get(&link).send().await {
                Ok(r) => {
                    let is_html = r
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .map(|v| v.contains("text/html"))
                        .unwrap_or(false);

                    match r.text().await {
                        Ok(t) => (t, is_html),
                        Err(e) => (
                            format!("<!-- failed to read body {}: {} -->", link, e),
                            false,
                        ),
                    }
                }
                Err(e) => (format!("<!-- failed to fetch {}: {} -->", link, e), false),
            };
            (idx, body, is_html)
        }
    }))
    .buffer_unordered(concurrent_limit);

    let mut parts: Vec<(usize, String)> = fetches
        .then(|(idx, body, is_html)| async move {
            if is_html {
                let text = tokio::task::spawn_blocking(move || html_to_text(&body))
                    .await
                    .unwrap_or_else(|_| String::new());
                (idx, text)
            } else {
                (idx, body.trim().to_string())
            }
        })
        .filter(|(_, text)| {
            let is_empty = text.trim().is_empty();
            async move { !is_empty }
        })
        .collect()
        .await;

    parts.sort_by_key(|(i, _)| *i);
    let ordered: Vec<String> = parts.into_iter().map(|(_, s)| s).collect();
    let full_text = ordered.join("\n\n");

    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        full_text,
    ).into_response())
}

/// 将 HTML 转换为纯文本
///
/// 提取 HTML 文档中的文本内容，过滤掉 script、style 等标签。
fn html_to_text(input: &str) -> String {
    let document = Html::parse_document(input);
    let mut buffer = String::new();
    let root_selector = Selector::parse(":root").unwrap();
    if let Some(root) = document.select(&root_selector).next() {
        walk_element(root, &mut buffer);
    }
    buffer.trim().to_string()
}

/// 递归遍历 HTML 元素并提取文本
fn walk_element(element: ElementRef, buffer: &mut String) {
    let name = element.value().name();
    if name == "script" || name == "style" || name == "head" {
        return;
    }
    if is_block_element(name) {
        ensure_newlines(buffer, 2);
    } else if name == "br" {
        buffer.push('\n');
    }

    for child in element.children() {
        match child.value() {
            Node::Text(text) => {
                let s = text.trim();
                if !s.is_empty() {
                    if buffer.ends_with(|c: char| !c.is_whitespace()) {
                        buffer.push(' ');
                    }
                    buffer.push_str(s);
                }
            }
            Node::Element(_) => {
                if let Some(child_elem) = ElementRef::wrap(child) {
                    walk_element(child_elem, buffer);
                }
            }
            _ => {}
        }
    }

    if is_block_element(name) {
        ensure_newlines(buffer, 2);
    }
}

/// 确保缓冲区有指定数量的换行符
fn ensure_newlines(buffer: &mut String, n: usize) {
    if buffer.is_empty() {
        return;
    }
    let existing_newlines = buffer.chars().rev().take_while(|c| *c == '\n').count();
    for _ in existing_newlines..n {
        buffer.push('\n');
    }
}

/// 判断 HTML 标签是否为块级元素
fn is_block_element(tag: &str) -> bool {
    matches!(
        tag,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "canvas"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "li"
            | "main"
            | "nav"
            | "noscript"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "tfoot"
            | "ul"
            | "video"
            | "tr"
    )
}
