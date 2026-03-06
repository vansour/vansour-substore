use sqlx::SqlitePool;
use std::time::Duration;

/// HTTP 请求获取配置
///
/// 包含超时和并发限制等配置，用于控制订阅获取行为。
#[derive(Debug, Clone)]
pub struct FetchConfig {
    /// HTTP 请求超时时间
    #[allow(dead_code)]
    pub timeout: Duration,
    /// 并发请求的最大数量
    pub concurrent_limit: usize,
}

/// 用户限制配置
///
/// 包含用户级别的资源限制，如最大链接数和最大用户数。
#[derive(Debug, Clone)]
pub struct UserLimits {
    /// 单个用户允许的最大链接数
    pub max_links_per_user: usize,
    /// 系统允许的最大用户数
    pub max_users: usize,
}

/// 应用全局状态
///
/// 在请求处理器之间共享的状态，包含数据库连接池、HTTP 客户端和配置。
#[derive(Debug, Clone)]
pub struct AppState {
    /// SQLite 数据库连接池
    pub db: SqlitePool,
    /// HTTP 客户端
    pub client: reqwest::Client,
    /// 订阅获取配置
    pub fetch_config: FetchConfig,
    /// 用户限制配置
    pub user_limits: UserLimits,
}

impl AppState {
    /// 从配置创建应用状态
    ///
    /// # 参数
    ///
    /// * `db` - 数据库连接池
    /// * `client` - HTTP 客户端
    /// * `timeout_secs` - HTTP 请求超时时间（秒）
    /// * `concurrent_limit` - 并发请求限制
    /// * `max_links_per_user` - 每用户最大链接数
    /// * `max_users` - 最大用户数
    pub fn from_config(
        db: SqlitePool,
        client: reqwest::Client,
        timeout_secs: u64,
        concurrent_limit: usize,
        max_links_per_user: usize,
        max_users: usize,
    ) -> Self {
        Self {
            db,
            client,
            fetch_config: FetchConfig {
                timeout: Duration::from_secs(timeout_secs),
                concurrent_limit,
            },
            user_limits: UserLimits {
                max_links_per_user,
                max_users,
            },
        }
    }
}
