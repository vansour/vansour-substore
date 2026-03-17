use std::{
    collections::HashMap,
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    str::FromStr,
};

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
    pub web_dist_dir: PathBuf,
    pub database_url: String,
    pub cookie_secure: bool,
    pub session_ttl_minutes: i64,
    pub session_cleanup_interval_secs: u64,
    pub trust_proxy_headers: bool,
    pub login_max_attempts: usize,
    pub login_window_secs: u64,
    pub login_lockout_secs: u64,
    pub public_max_requests: usize,
    pub public_window_secs: u64,
    pub cache_ttl_secs: u64,
    pub db_max_connections: u32,
    pub fetch_timeout_secs: u64,
    pub dns_cache_ttl_secs: u64,
    pub fetch_host_overrides: HashMap<String, Vec<SocketAddr>>,
    pub concurrent_limit: usize,
    pub max_links_per_user: usize,
    pub max_users: usize,
    pub admin_user: String,
    pub admin_password: String,
    pub cors_allow_origin: Vec<String>,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self {
            host: env::var("HOST")
                .ok()
                .and_then(|value| IpAddr::from_str(&value).ok())
                .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            port: env::var("PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8080),
            web_dist_dir: env::var("WEB_DIST_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("dist")),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite://data/substore.db?mode=rwc".to_string()),
            cookie_secure: env::var("COOKIE_SECURE")
                .map(|value| value == "true")
                .unwrap_or(false),
            session_ttl_minutes: env::var("SESSION_TTL_MINUTES")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &i64| *value > 0)
                .unwrap_or(60 * 24 * 7),
            session_cleanup_interval_secs: env::var("SESSION_CLEANUP_INTERVAL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &u64| *value > 0)
                .unwrap_or(300),
            trust_proxy_headers: env::var("TRUST_PROXY_HEADERS")
                .map(|value| value == "true")
                .unwrap_or(false),
            login_max_attempts: env::var("LOGIN_MAX_ATTEMPTS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &usize| *value > 0)
                .unwrap_or(5),
            login_window_secs: env::var("LOGIN_WINDOW_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &u64| *value > 0)
                .unwrap_or(300),
            login_lockout_secs: env::var("LOGIN_LOCKOUT_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &u64| *value > 0)
                .unwrap_or(900),
            public_max_requests: env::var("PUBLIC_MAX_REQUESTS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &usize| *value > 0)
                .unwrap_or(60),
            public_window_secs: env::var("PUBLIC_WINDOW_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &u64| *value > 0)
                .unwrap_or(60),
            cache_ttl_secs: env::var("CACHE_TTL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &u64| *value > 0)
                .unwrap_or(300),
            db_max_connections: env::var("DB_MAX_CONNECTIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5),
            fetch_timeout_secs: env::var("FETCH_TIMEOUT_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10),
            dns_cache_ttl_secs: env::var("DNS_CACHE_TTL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .filter(|value: &u64| *value > 0)
                .unwrap_or(30),
            fetch_host_overrides: env::var("FETCH_HOST_OVERRIDES")
                .map(|value| parse_fetch_host_overrides(&value))
                .unwrap_or_default(),
            concurrent_limit: env::var("CONCURRENT_LIMIT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10),
            max_links_per_user: env::var("MAX_LINKS_PER_USER")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(100),
            max_users: env::var("MAX_USERS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(100),
            admin_user: env::var("ADMIN_USER").unwrap_or_else(|_| "admin".to_string()),
            admin_password: env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string()),
            cors_allow_origin: env::var("CORS_ALLOW_ORIGIN")
                .unwrap_or_else(|_| "http://127.0.0.1:8081,http://localhost:8081".to_string())
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        }
    }

    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

fn parse_fetch_host_overrides(input: &str) -> HashMap<String, Vec<SocketAddr>> {
    input
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let (host, addrs) = entry.split_once('=')?;
            let resolved_addrs = addrs
                .split('|')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .filter_map(|value| SocketAddr::from_str(value).ok())
                .collect::<Vec<_>>();

            if resolved_addrs.is_empty() {
                None
            } else {
                Some((host.trim().to_string(), resolved_addrs))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use super::parse_fetch_host_overrides;

    #[test]
    fn parses_fetch_host_overrides() {
        let overrides = parse_fetch_host_overrides(
            "fixture.invalid:4181=127.0.0.1:4181,dual.invalid:80=127.0.0.1:18080|127.0.0.1:18081",
        );

        assert_eq!(
            overrides.get("fixture.invalid:4181"),
            Some(&vec![
                "127.0.0.1:4181".parse::<SocketAddr>().expect("socket addr")
            ])
        );
        assert_eq!(
            overrides.get("dual.invalid:80"),
            Some(&vec![
                "127.0.0.1:18080"
                    .parse::<SocketAddr>()
                    .expect("socket addr"),
                "127.0.0.1:18081"
                    .parse::<SocketAddr>()
                    .expect("socket addr"),
            ])
        );
    }
}
