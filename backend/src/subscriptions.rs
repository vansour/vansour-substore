use futures::stream::StreamExt;
use reqwest::{Url, header, redirect::Policy};
use scraper::{ElementRef, Html, Node, Selector};
use sqlx::SqlitePool;
use std::{
    collections::BTreeMap,
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tracing::warn;

use crate::{
    diagnostics::{self, DiagnosticUpsert},
    error::{ApiError, ApiResult},
};

const MAX_FETCH_BYTES: usize = 10 * 1024 * 1024;
const MAX_BUFFER: usize = 1024 * 1024;
const MAX_REDIRECTS: usize = 5;

#[derive(Clone, Debug)]
pub struct DnsResolver {
    ttl: Duration,
    cache: Arc<RwLock<HashMap<String, CachedResolution>>>,
    overrides: HashMap<String, Vec<SocketAddr>>,
}

#[derive(Clone, Debug)]
struct CachedResolution {
    addrs: Vec<SocketAddr>,
    expires_at: Instant,
}

#[derive(Clone, Debug)]
struct ResolvedAddrs {
    addrs: Vec<SocketAddr>,
    from_override: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ClientCacheKey {
    host: String,
    resolved_addrs: Vec<SocketAddr>,
}

#[derive(Clone, Debug)]
pub struct PinnedClientPool {
    timeout_secs: u64,
    clients: Arc<Mutex<HashMap<ClientCacheKey, reqwest::Client>>>,
}

impl PinnedClientPool {
    pub fn new(timeout_secs: u64) -> Self {
        Self {
            timeout_secs,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn client_for_target(
        &self,
        target: &ValidatedFetchTarget,
    ) -> Result<reqwest::Client, reqwest::Error> {
        let key = ClientCacheKey {
            host: target.host.clone(),
            resolved_addrs: target.resolved_addrs.clone(),
        };

        let mut clients = self
            .clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }

        let client = fetch_client_builder(self.timeout_secs)
            .resolve_to_addrs(&target.host, &target.resolved_addrs)
            .build()?;
        clients.insert(key, client.clone());
        Ok(client)
    }

    #[cfg(test)]
    fn cached_client_count(&self) -> usize {
        self.clients
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

impl DnsResolver {
    pub fn new(ttl_secs: u64) -> Self {
        Self::with_overrides(ttl_secs, HashMap::new())
    }

    pub fn with_overrides(ttl_secs: u64, overrides: HashMap<String, Vec<SocketAddr>>) -> Self {
        Self {
            ttl: Duration::from_secs(ttl_secs.max(1)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            overrides,
        }
    }

    async fn resolve_host(
        &self,
        host: &str,
        port: u16,
        url_str: &str,
    ) -> Result<ResolvedAddrs, String> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(ResolvedAddrs {
                addrs: vec![SocketAddr::new(ip, port)],
                from_override: false,
            });
        }

        let cache_key = format!("{host}:{port}");
        if let Some(addrs) = self.overrides.get(&cache_key) {
            return Ok(ResolvedAddrs {
                addrs: addrs.clone(),
                from_override: true,
            });
        }

        if let Some(addrs) = self.cached_addrs(&cache_key).await {
            return Ok(ResolvedAddrs {
                addrs,
                from_override: false,
            });
        }

        let resolved_addrs = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| format!("failed to resolve host: {url_str}"))?
            .collect::<Vec<_>>();

        if resolved_addrs.is_empty() {
            return Err(format!("failed to resolve host: {url_str}"));
        }

        self.cache.write().await.insert(
            cache_key,
            CachedResolution {
                addrs: resolved_addrs.clone(),
                expires_at: Instant::now() + self.ttl,
            },
        );

        Ok(ResolvedAddrs {
            addrs: resolved_addrs,
            from_override: false,
        })
    }

    async fn cached_addrs(&self, cache_key: &str) -> Option<Vec<SocketAddr>> {
        let cached = self.cache.read().await.get(cache_key).cloned();
        match cached {
            Some(entry) if entry.expires_at > Instant::now() => Some(entry.addrs),
            Some(_) => {
                self.cache.write().await.remove(cache_key);
                None
            }
            None => None,
        }
    }
}

#[derive(Clone, Debug)]
struct ValidatedFetchTarget {
    url: Url,
    host: String,
    resolved_addrs: Vec<SocketAddr>,
    host_is_ip_literal: bool,
}

fn fetch_client_builder(timeout_secs: u64) -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(Policy::none())
        .pool_max_idle_per_host(5)
}

pub fn build_fetch_client(timeout_secs: u64) -> Result<reqwest::Client, reqwest::Error> {
    fetch_client_builder(timeout_secs).build()
}

pub async fn validate_safe_url(resolver: &DnsResolver, url_str: &str) -> Result<(), String> {
    validate_fetch_url(resolver, url_str).await.map(|_| ())
}

pub async fn ensure_safe_url(resolver: &DnsResolver, url_str: &str) -> ApiResult<()> {
    validate_safe_url(resolver, url_str)
        .await
        .map_err(|message| ApiError::validation("url", message))
}

pub struct FetchRuntime<'a> {
    pub pool: &'a SqlitePool,
    pub client: &'a reqwest::Client,
    pub resolver: Arc<DnsResolver>,
    pub pinned_client_pool: Arc<PinnedClientPool>,
    pub semaphore: Arc<tokio::sync::Semaphore>,
    pub concurrent_limit: usize,
}

pub async fn fetch_and_merge_for_user(
    runtime: FetchRuntime<'_>,
    username: &str,
    links: Vec<String>,
) -> String {
    let link_count = links.len();
    let mut fetches = futures::stream::iter(links.into_iter().enumerate().map(|(idx, link)| {
        let client = runtime.client.clone();
        let resolver = runtime.resolver.clone();
        let pinned_client_pool = runtime.pinned_client_pool.clone();
        let semaphore = runtime.semaphore.clone();
        async move {
            let _permit = semaphore
                .acquire()
                .await
                .expect("semaphore should be available");
            fetch_link(&client, &resolver, &pinned_client_pool, idx, link).await
        }
    }))
    .buffer_unordered(runtime.concurrent_limit);

    let mut diagnostics_to_store = Vec::with_capacity(link_count);
    let mut pending_parts = BTreeMap::new();
    let mut next_part_index = 0usize;
    let mut merged = String::new();

    while let Some((idx, result)) = fetches.next().await {
        diagnostics_to_store.push(result.diagnostic);
        pending_parts.insert(idx, result.content);

        while let Some(content) = pending_parts.remove(&next_part_index) {
            if let Some(content) = content {
                append_merged_content(&mut merged, &content);
            }
            next_part_index += 1;
        }
    }

    if let Err(error) =
        diagnostics::store_user_diagnostics(runtime.pool, username, &diagnostics_to_store).await
    {
        warn!(username, error = %error, "failed to persist fetch diagnostics");
    }

    merged
}

async fn fetch_link(
    client: &reqwest::Client,
    resolver: &DnsResolver,
    pinned_client_pool: &PinnedClientPool,
    idx: usize,
    link: String,
) -> (usize, FetchResult) {
    (
        idx,
        fetch_link_body(client, resolver, pinned_client_pool, &link).await,
    )
}

async fn fetch_link_body(
    client: &reqwest::Client,
    resolver: &DnsResolver,
    pinned_client_pool: &PinnedClientPool,
    link: &str,
) -> FetchResult {
    let mut current_target = match validate_fetch_url(resolver, link).await {
        Ok(target) => target,
        Err(error) => {
            return failed_attempt(
                link,
                "blocked",
                error,
                AttemptMetadata {
                    http_status: None,
                    content_type: None,
                    body_bytes: None,
                    redirect_count: 0,
                    is_html: false,
                },
            );
        }
    };

    for redirect_count in 0..=MAX_REDIRECTS {
        let response =
            match send_validated_request(client, pinned_client_pool, &current_target).await {
                Ok(response) => response,
                Err(error) => {
                    return failed_attempt(
                        link,
                        "error",
                        format!("failed to fetch {}: {error}", current_target.url),
                        AttemptMetadata {
                            http_status: None,
                            content_type: None,
                            body_bytes: None,
                            redirect_count: redirect_count as u8,
                            is_html: false,
                        },
                    );
                }
            };

        if response.status().is_redirection() {
            if redirect_count == MAX_REDIRECTS {
                warn!(url = %current_target.url, redirects = redirect_count, "too many redirects");
                return failed_attempt(
                    link,
                    "error",
                    format!("too many redirects while fetching {link}: maximum {MAX_REDIRECTS}"),
                    AttemptMetadata {
                        http_status: None,
                        content_type: None,
                        body_bytes: None,
                        redirect_count: redirect_count as u8,
                        is_html: false,
                    },
                );
            }

            let Some(location) = response.headers().get(header::LOCATION) else {
                return failed_attempt(
                    link,
                    "error",
                    format!("redirect missing location header: {}", current_target.url),
                    AttemptMetadata {
                        http_status: None,
                        content_type: None,
                        body_bytes: None,
                        redirect_count: redirect_count as u8,
                        is_html: false,
                    },
                );
            };
            let location = match location.to_str() {
                Ok(location) => location,
                Err(_) => {
                    return failed_attempt(
                        link,
                        "error",
                        format!(
                            "redirect location is not valid utf-8: {}",
                            current_target.url
                        ),
                        AttemptMetadata {
                            http_status: None,
                            content_type: None,
                            body_bytes: None,
                            redirect_count: redirect_count as u8,
                            is_html: false,
                        },
                    );
                }
            };
            current_target =
                match resolve_redirect_url(resolver, &current_target.url, location).await {
                    Ok(target) => target,
                    Err(error) => {
                        return failed_attempt(
                            link,
                            "blocked",
                            error,
                            AttemptMetadata {
                                http_status: None,
                                content_type: None,
                                body_bytes: None,
                                redirect_count: (redirect_count + 1) as u8,
                                is_html: false,
                            },
                        );
                    }
                };
            continue;
        }

        let status_code = response.status().as_u16();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let is_html = content_type
            .as_deref()
            .map(|value| value.contains("text/html"))
            .unwrap_or(false);

        if !response.status().is_success() {
            return failed_attempt(
                link,
                "error",
                format!(
                    "unexpected response status {} while fetching {}",
                    response.status(),
                    current_target.url
                ),
                AttemptMetadata {
                    http_status: Some(status_code),
                    content_type,
                    body_bytes: None,
                    redirect_count: redirect_count as u8,
                    is_html,
                },
            );
        }

        if let Some(content_length) = response.content_length()
            && content_length > MAX_FETCH_BYTES as u64
        {
            warn!(url = %current_target.url, size = content_length, "content too large");
            return failed_attempt(
                link,
                "error",
                format!(
                    "content too large while fetching {}: {} bytes exceeds {} bytes limit",
                    current_target.url, content_length, MAX_FETCH_BYTES
                ),
                AttemptMetadata {
                    http_status: Some(status_code),
                    content_type,
                    body_bytes: Some(content_length),
                    redirect_count: redirect_count as u8,
                    is_html,
                },
            );
        }

        match read_response_body_limited(response, &current_target.url).await {
            Ok(body) => {
                let body_bytes = body.len() as u64;
                let body = String::from_utf8_lossy(&body).into_owned();
                let content = normalize_fetch_content(body, is_html).await;
                return FetchResult {
                    content,
                    diagnostic: DiagnosticUpsert {
                        source_url: link.to_string(),
                        status: "success".to_string(),
                        detail: Some("Fetch completed successfully".to_string()),
                        http_status: Some(status_code),
                        content_type,
                        body_bytes: Some(body_bytes),
                        redirect_count: redirect_count as u8,
                        is_html,
                    },
                };
            }
            Err(error) => {
                return failed_attempt(
                    link,
                    "error",
                    error,
                    AttemptMetadata {
                        http_status: Some(status_code),
                        content_type,
                        body_bytes: None,
                        redirect_count: redirect_count as u8,
                        is_html,
                    },
                );
            }
        }
    }

    failed_attempt(
        link,
        "error",
        format!("too many redirects while fetching {link}: maximum {MAX_REDIRECTS}"),
        AttemptMetadata {
            http_status: None,
            content_type: None,
            body_bytes: None,
            redirect_count: MAX_REDIRECTS as u8,
            is_html: false,
        },
    )
}

fn failed_attempt(
    link: &str,
    status: &str,
    detail: String,
    metadata: AttemptMetadata,
) -> FetchResult {
    FetchResult {
        content: None,
        diagnostic: DiagnosticUpsert {
            source_url: link.to_string(),
            status: status.to_string(),
            detail: Some(detail),
            http_status: metadata.http_status,
            content_type: metadata.content_type,
            body_bytes: metadata.body_bytes,
            redirect_count: metadata.redirect_count,
            is_html: metadata.is_html,
        },
    }
}

async fn validate_fetch_url(
    resolver: &DnsResolver,
    url_str: &str,
) -> Result<ValidatedFetchTarget, String> {
    let url = Url::parse(url_str).map_err(|_| format!("invalid url: {url_str}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!("unsupported scheme: {url_str}"));
    }

    let host = url
        .host_str()
        .ok_or_else(|| format!("missing host: {url_str}"))?
        .to_string();
    let port = url.port_or_known_default().unwrap_or(80);
    let host_is_ip_literal = host.parse::<IpAddr>().is_ok();
    let resolved = resolver.resolve_host(&host, port, url_str).await?;
    if !resolved.from_override {
        for addr in &resolved.addrs {
            if is_forbidden_ip(addr.ip()) {
                return Err(format!("unsafe target: {url_str}"));
            }
        }
    }

    Ok(ValidatedFetchTarget {
        url,
        host,
        resolved_addrs: resolved.addrs,
        host_is_ip_literal,
    })
}

async fn resolve_redirect_url(
    resolver: &DnsResolver,
    current_url: &Url,
    location: &str,
) -> Result<ValidatedFetchTarget, String> {
    let next_url = current_url
        .join(location)
        .map_err(|_| format!("invalid redirect target from {current_url}: {location}"))?;
    validate_fetch_url(resolver, next_url.as_str()).await
}

async fn send_validated_request(
    client: &reqwest::Client,
    pinned_client_pool: &PinnedClientPool,
    target: &ValidatedFetchTarget,
) -> Result<reqwest::Response, reqwest::Error> {
    if target.host_is_ip_literal {
        return client.get(target.url.clone()).send().await;
    }

    pinned_client_pool
        .client_for_target(target)?
        .get(target.url.clone())
        .send()
        .await
}

async fn read_response_body_limited(
    response: reqwest::Response,
    url: &Url,
) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk =
            chunk.map_err(|error| format!("failed to read response body {url}: {error}"))?;
        append_limited(&mut buffer, chunk.as_ref(), MAX_FETCH_BYTES)?;
    }

    Ok(buffer)
}

fn append_limited(buffer: &mut Vec<u8>, chunk: &[u8], max_bytes: usize) -> Result<(), String> {
    let next_len = buffer.len().saturating_add(chunk.len());
    if next_len > max_bytes {
        return Err(format!(
            "content too large: exceeds {} bytes limit while streaming body",
            max_bytes
        ));
    }

    buffer.extend_from_slice(chunk);
    Ok(())
}

fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => is_forbidden_ipv4(ipv4),
        IpAddr::V6(ipv6) => is_forbidden_ipv6(ipv6),
    }
}

fn is_forbidden_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();

    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.octets() == [255, 255, 255, 255]
        || a == 0
        || (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || a >= 240
}

fn is_forbidden_ipv6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];

    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
}

fn html_to_text(input: &str) -> String {
    if input.len() > MAX_FETCH_BYTES {
        warn!(size = input.len(), "html content too large");
        return format!("<!-- HTML too large: {} bytes, truncated -->", input.len());
    }

    let document = Html::parse_document(input);
    let root_selector = Selector::parse(":root").expect("valid root selector");
    let mut buffer = String::with_capacity(input.len().min(MAX_BUFFER));

    if let Some(root) = document.select(&root_selector).next() {
        walk_element_limited(root, &mut buffer, MAX_BUFFER);
    }

    buffer.trim().to_string()
}

fn walk_element_limited(element: ElementRef, buffer: &mut String, max_len: usize) {
    if buffer.len() >= max_len {
        buffer.push_str("\n<!-- content truncated -->");
        return;
    }

    let name = element.value().name();
    if matches!(name, "script" | "style" | "head" | "noscript") {
        return;
    }

    if is_block_element(name) {
        ensure_newlines(buffer, 2);
    } else if name == "br" {
        buffer.push('\n');
    }

    for child in element.children() {
        if buffer.len() >= max_len {
            buffer.push_str("\n<!-- content truncated -->");
            return;
        }

        match child.value() {
            Node::Text(text) => {
                let text = text.trim();
                if !text.is_empty() {
                    if buffer.ends_with(|c: char| !c.is_whitespace()) {
                        buffer.push(' ');
                    }
                    buffer.push_str(text);
                }
            }
            Node::Element(_) => {
                if let Some(child_element) = ElementRef::wrap(child) {
                    walk_element_limited(child_element, buffer, max_len);
                }
            }
            _ => {}
        }
    }

    if is_block_element(name) {
        ensure_newlines(buffer, 2);
    }
}

fn ensure_newlines(buffer: &mut String, count: usize) {
    if buffer.is_empty() {
        return;
    }

    let existing = buffer.chars().rev().take_while(|ch| *ch == '\n').count();
    for _ in existing..count {
        buffer.push('\n');
    }
}

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

struct FetchResult {
    content: Option<String>,
    diagnostic: DiagnosticUpsert,
}

async fn normalize_fetch_content(body: String, is_html: bool) -> Option<String> {
    let normalized = if is_html {
        tokio::task::spawn_blocking(move || html_to_text(&body))
            .await
            .unwrap_or_else(|_| String::new())
    } else {
        body
    };

    let normalized = normalized.trim().to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn append_merged_content(merged: &mut String, content: &str) {
    if content.is_empty() {
        return;
    }

    if !merged.is_empty() {
        merged.push_str("\n\n");
    }
    merged.push_str(content);
}

struct AttemptMetadata {
    http_status: Option<u16>,
    content_type: Option<String>,
    body_bytes: Option<u64>,
    redirect_count: u8,
    is_html: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        DnsResolver, PinnedClientPool, ValidatedFetchTarget, append_limited, ensure_safe_url,
        html_to_text, resolve_redirect_url, send_validated_request,
    };
    use reqwest::{Client, StatusCode, Url};
    use std::{collections::HashMap, net::SocketAddr};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[tokio::test]
    async fn rejects_private_targets() {
        let resolver = DnsResolver::new(30);
        assert!(
            ensure_safe_url(&resolver, "http://127.0.0.1/feed")
                .await
                .is_err()
        );
        assert!(
            ensure_safe_url(&resolver, "http://10.0.0.10/feed")
                .await
                .is_err()
        );
        assert!(
            ensure_safe_url(&resolver, "http://169.254.10.20/feed")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn allows_public_ip_targets() {
        let resolver = DnsResolver::new(30);
        assert!(
            ensure_safe_url(&resolver, "https://1.1.1.1/feed")
                .await
                .is_ok()
        );
        assert!(
            ensure_safe_url(&resolver, "http://93.184.216.34/feed")
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn blocks_redirects_to_private_hosts() {
        let resolver = DnsResolver::new(30);
        let current = Url::parse("https://1.1.1.1/start").expect("valid url");
        let error = resolve_redirect_url(&resolver, &current, "http://127.0.0.1:3000/private")
            .await
            .unwrap_err();
        assert!(error.contains("unsafe target"));
    }

    #[tokio::test]
    async fn resolves_relative_redirects_against_current_url() {
        let resolver = DnsResolver::new(30);
        let current = Url::parse("https://1.1.1.1/start").expect("valid url");
        let next = resolve_redirect_url(&resolver, &current, "/next")
            .await
            .expect("redirect should resolve");
        assert_eq!(next.url.as_str(), "https://1.1.1.1/next");
    }

    #[test]
    fn enforces_stream_limit_incrementally() {
        let mut buffer = Vec::new();
        append_limited(&mut buffer, b"12", 3).expect("first chunk fits");
        let error = append_limited(&mut buffer, b"34", 3).unwrap_err();
        assert!(error.contains("content too large"));
    }

    #[test]
    fn converts_html_to_text_without_scripts() {
        let html = r#"
        <html>
          <head><title>ignore</title></head>
          <body>
            <main>
              <h1>Title</h1>
              <p>Hello <strong>world</strong>.</p>
              <script>window.alert("ignore")</script>
            </main>
          </body>
        </html>
        "#;

        let text = html_to_text(html);
        assert!(text.contains("Title"));
        assert!(text.contains("Hello world ."));
        assert!(!text.contains("ignore"));
    }

    #[tokio::test]
    async fn failed_fetches_do_not_produce_merged_output() {
        let resolver = DnsResolver::new(30);
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client should build");
        let pinned_client_pool = PinnedClientPool::new(5);

        let attempt = super::fetch_link_body(
            &client,
            &resolver,
            &pinned_client_pool,
            "http://127.0.0.1/feed",
        )
        .await;
        assert!(attempt.content.is_none());
        assert_eq!(attempt.diagnostic.status, "blocked");
    }

    #[tokio::test]
    async fn sends_hostname_requests_to_validated_resolved_addrs() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener should bind");
        let listener_addr = listener.local_addr().expect("listener addr");

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("connection accepted");
            let mut buffer = [0_u8; 1024];
            let _ = socket.read(&mut buffer).await.expect("request should read");
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("response should write");
        });

        let client = super::build_fetch_client(5).expect("client should build");
        let target = ValidatedFetchTarget {
            url: Url::parse(&format!(
                "http://stage10.invalid:{}/feed",
                listener_addr.port()
            ))
            .expect("url should parse"),
            host: "stage10.invalid".to_string(),
            resolved_addrs: vec![listener_addr],
            host_is_ip_literal: false,
        };
        let pinned_client_pool = PinnedClientPool::new(5);

        let response = send_validated_request(&client, &pinned_client_pool, &target)
            .await
            .expect("request should use pinned dns result");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.text().await.expect("body should read"), "ok");

        server.await.expect("server task should finish");
    }

    #[tokio::test]
    async fn explicit_host_overrides_can_target_local_fixture() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "fixture.invalid:4181".to_string(),
            vec!["127.0.0.1:4181".parse::<SocketAddr>().expect("socket addr")],
        );
        let resolver = DnsResolver::with_overrides(30, overrides);

        assert!(
            ensure_safe_url(&resolver, "http://fixture.invalid:4181/feed")
                .await
                .is_ok()
        );
    }

    #[test]
    fn reuses_cached_pinned_client_for_same_target() {
        let pool = PinnedClientPool::new(5);
        let target = ValidatedFetchTarget {
            url: Url::parse("http://fixture.invalid:4181/feed").expect("url should parse"),
            host: "fixture.invalid".to_string(),
            resolved_addrs: vec!["127.0.0.1:4181".parse().expect("socket addr")],
            host_is_ip_literal: false,
        };

        let _ = pool
            .client_for_target(&target)
            .expect("client should build for target");
        let _ = pool
            .client_for_target(&target)
            .expect("client should be reused from cache");

        assert_eq!(pool.cached_client_count(), 1);
    }
}
