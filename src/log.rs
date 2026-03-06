//! 日志管理模块
//!
//! 初始化日志系统和请求追踪中间件。

use crate::config::AppConfig;
use axum::{
    extract::Request,
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use std::path::Path;
use std::sync::Once;
use std::time::Instant;
use tracing::{error, info, warn};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{Layer, fmt};
use uuid::Uuid;

/// 初始化日志系统
///
/// 设置两级日志输出：
/// 1. 控制台日志 - 使用紧凑格式，适合 Docker 日志查看
/// 2. 文件日志 - 使用 JSON 格式，包含完整结构化信息，适合分析
pub fn init_logging(config: &AppConfig) {
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        println!("Initializing logging...");

        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log.level));

        // 控制台输出层
        let stdout_layer = fmt::layer()
            .compact()
            .with_target(false)
            .with_file(false)
            .with_level(true)
            .with_ansi(true)
            .with_filter(env_filter.clone());

        // 文件输出层
        let path_str = &config.log.log_file_path;
        let path = Path::new(path_str);

        let directory = path.parent().unwrap_or_else(|| Path::new("./logs"));
        let filename = path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("sub.log"));

        let file_appender = tracing_appender::rolling::daily(directory, filename);
        let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

        std::mem::forget(_guard);

        let file_layer = fmt::layer()
            .json()
            .with_writer(non_blocking)
            .with_span_events(FmtSpan::CLOSE)
            .with_filter(env_filter);

        if let Err(e) = tracing_subscriber::registry()
            .with(stdout_layer)
            .with(file_layer)
            .try_init()
        {
            eprintln!("Failed to initialize tracing subscriber: {}", e);
        }
    });
}

/// 请求追踪中间件
///
/// 生成唯一请求 ID，记录请求耗时和状态码。
pub async fn trace_requests(
    req: Request,
    next: Next,
) -> Response {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let http_method = req.method().to_string();
    let http_path = req.uri().path().to_string();
    let headers = req.headers();

    let client_ip = extract_client_ip(headers);

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-")
        .to_string();

    let span = tracing::info_span!(
        "http_req",
        id = %request_id,
        method = %http_method,
        path = %http_path,
        ip = %client_ip
    );

    let _enter = span.enter();
    let start_time = Instant::now();

    let mut res = next.run(req).await;

    let duration = start_time.elapsed();
    let status_code = res.status().as_u16();

    res.headers_mut().insert(
        "x-request-id",
        request_id.parse().unwrap(),
    );

    match status_code {
        500..=599 => {
            error!(
                status = status_code,
                latency_ms = duration.as_millis(),
                ua = %user_agent,
                "Internal Server Error"
            );
        }
        400..=499 => {
            warn!(
                status = status_code,
                latency_ms = duration.as_millis(),
                "Client Error"
            );
        }
        _ => {
            if http_path == "/healthz" {
                tracing::debug!(status = status_code, "health check");
            } else {
                info!(
                    status = status_code,
                    latency_ms = duration.as_millis(),
                    "Finished"
                );
            }
        }
    }

    res
}

/// 从请求头中提取客户端 IP 地址
///
/// 优先顺序：
/// 1. X-Forwarded-For（反向代理设置）
/// 2. X-Real-IP
/// 3. unknown
fn extract_client_ip(headers: &HeaderMap) -> String {
    if let Some(xff) = headers.get("x-forwarded-for")
        && let Ok(val) = xff.to_str() {
        return val.split(',').next().unwrap_or("unknown").trim().to_string();
    }
    if let Some(xri) = headers.get("x-real-ip")
        && let Ok(val) = xri.to_str() {
        return val.to_string();
    }
    "unknown".to_string()
}
