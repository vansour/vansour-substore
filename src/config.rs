//! 配置管理模块
//!
//! 从环境变量加载应用配置并提供验证。

use serde::Deserialize;
use std::env;

/// 服务器配置
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// 监听地址
    pub host: String,
    /// 监听端口
    pub port: u16,
    /// Cookie 是否使用 HTTPS
    pub cookie_secure: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            cookie_secure: false,
        }
    }
}

/// 日志配置
#[derive(Debug, Deserialize, Clone)]
pub struct LogConfig {
    /// 日志文件路径
    pub log_file_path: String,
    /// 日志级别 (trace, debug, info, warn, error)
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            log_file_path: "app.log".to_string(),
            level: "info".to_string(),
        }
    }
}

/// 数据库配置
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    /// 最大连接数
    pub max_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            max_connections: 5,
        }
    }
}

/// 订阅配置
#[derive(Debug, Deserialize, Clone)]
pub struct SubscriptionConfig {
    /// HTTP 请求超时时间（秒）
    pub fetch_timeout_secs: u64,
    /// 并发请求限制
    pub concurrent_limit: usize,
    /// 每用户最大链接数
    pub max_links_per_user: usize,
    /// 最大用户数
    pub max_users: usize,
}

impl Default for SubscriptionConfig {
    fn default() -> Self {
        Self {
            fetch_timeout_secs: 10,
            concurrent_limit: 10,
            max_links_per_user: 100,
            max_users: 100,
        }
    }
}

/// 应用配置
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub log: LogConfig,
    pub database: DatabaseConfig,
    pub subscription: SubscriptionConfig,
}

impl AppConfig {
    /// 从环境变量加载配置
    ///
    /// # 环境变量
    ///
    /// - `HOST` - 服务器监听地址（默认: 0.0.0.0）
    /// - `PORT` - 服务器端口（默认: 8080）
    /// - `COOKIE_SECURE` - Cookie 是否使用 HTTPS（默认: false）
    /// - `LOG_FILE` - 日志文件路径（默认: app.log）
    /// - `LOG_LEVEL` - 日志级别（默认: info）
    /// - `DB_MAX_CONNECTIONS` - 数据库最大连接数（默认: 5）
    /// - `FETCH_TIMEOUT_SECS` - HTTP 请求超时时间（默认: 10）
    /// - `CONCURRENT_LIMIT` - 并发请求限制（默认: 10）
    /// - `MAX_LINKS_PER_USER` - 每用户最大链接数（默认: 100）
    /// - `MAX_USERS` - 最大用户数（默认: 100）
    pub fn load() -> Result<Self, String> {
        let server = ServerConfig {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
            cookie_secure: env::var("COOKIE_SECURE")
                .map(|v| v == "true")
                .unwrap_or(false),
        };

        let log = LogConfig {
            log_file_path: env::var("LOG_FILE").unwrap_or_else(|_| "app.log".to_string()),
            level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        };

        let database = DatabaseConfig {
            max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
        };

        let subscription = SubscriptionConfig {
            fetch_timeout_secs: env::var("FETCH_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            concurrent_limit: env::var("CONCURRENT_LIMIT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            max_links_per_user: env::var("MAX_LINKS_PER_USER")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            max_users: env::var("MAX_USERS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
        };

        let config = AppConfig { server, log, database, subscription };
        config.validate()?;
        Ok(config)
    }

    /// 验证配置有效性
    ///
    /// 检查所有配置值是否在合理范围内。
    pub fn validate(&self) -> Result<(), String> {
        if self.server.port < 1 {
            return Err("Port must be greater than 0".to_string());
        }

        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.log.level.to_lowercase().as_str()) {
            return Err(format!("Invalid log level '{}'. Valid values: trace, debug, info, warn, error", self.log.level));
        }

        if self.database.max_connections < 1 || self.database.max_connections > 100 {
            return Err("DB max connections must be between 1 and 100".to_string());
        }

        if self.subscription.fetch_timeout_secs == 0 || self.subscription.fetch_timeout_secs > 300 {
            return Err("Fetch timeout must be between 1 and 300 seconds".to_string());
        }

        if self.subscription.concurrent_limit == 0 || self.subscription.concurrent_limit > 100 {
            return Err("Concurrent limit must be between 1 and 100".to_string());
        }

        if self.subscription.max_links_per_user == 0 || self.subscription.max_links_per_user > 1000 {
            return Err("Max links per user must be between 1 and 1000".to_string());
        }

        if self.subscription.max_users == 0 || self.subscription.max_users > 1000 {
            return Err("Max users must be between 1 and 1000".to_string());
        }

        Ok(())
    }
}
