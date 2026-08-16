//! Runtime configuration from environment variables.
//!
//! Prefers local overrides `.env.local`, then `.env`, then `.env.example`.

use std::time::Duration;

use dotenvy::dotenv;

fn get(name: &str, fallback: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| fallback.to_string())
}

/// First defined of several names, so a shared .env can give Box its own
/// `BOX_*` values while Dream-API keeps plain `HOST`/`PORT`/`API_PREFIX`.
fn get_first(names: &[&str], fallback: &str) -> String {
    for name in names {
        if let Ok(value) = std::env::var(name) {
            if !value.is_empty() {
                return value;
            }
        }
    }
    fallback.to_string()
}

fn get_first_u16(names: &[&str], fallback: u16) -> u16 {
    for name in names {
        if let Ok(value) = std::env::var(name) {
            if let Ok(parsed) = value.parse::<u16>() {
                return parsed;
            }
        }
    }
    fallback
}

fn get_u64(name: &str, fallback: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(fallback)
}

fn get_u16(name: &str, fallback: u16) -> u16 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(fallback)
}

/// Box runtime configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP listen host.
    pub host: String,
    /// HTTP listen port.
    pub port: u16,
    /// Prefix prepended to all API routes.
    pub api_prefix: String,

    /// Base URL of the already-encapsulated Penlight-Dream-API service
    ///, e.g. `http://dream-api:8080/api`.
    pub dream_api_base: String,
    /// Upstream request timeout.
    pub dream_api_timeout: Duration,

    /// MongoDB connection string.
    pub mongodb_uri: String,
    /// MongoDB database name.
    pub mongodb_db: String,

    /// Poll interval for active monthly rankings / events.
    pub poll_interval_secs: u64,
    /// Refresh interval for master lists.
    pub master_interval_secs: u64,
    /// How long after a ranking's `endAt` we keep collecting the final
    /// snapshot, in seconds.
    pub post_end_window_secs: u64,

    /// Optional API key; when set, every `/api/*` request must send it via
    /// `X-API-Key` or `Authorization: Bearer <key>`.
    pub api_key: Option<String>,
}

/// Loads configuration from the environment.
pub fn load() -> Config {
    // `.env.local` > `.env` > `.env.example`.
    dotenv().ok();

    Config {
        host: get_first(&["BOX_HOST", "HOST"], "127.0.0.1"),
        port: get_first_u16(&["BOX_PORT", "PORT"], 8080),
        api_prefix: get_first(&["BOX_API_PREFIX", "API_PREFIX"], "/api"),

        dream_api_base: get("DREAM_API_BASE", "http://127.0.0.1:8081/api")
            .trim_end_matches('/')
            .to_string(),
        dream_api_timeout: Duration::from_secs(get_u64("DREAM_API_TIMEOUT_SECS", 30)),

        mongodb_uri: get("MONGODB_URI", "mongodb://127.0.0.1:27017"),
        mongodb_db: get("MONGODB_DB", "penlight_box"),

        poll_interval_secs: get_u64("POLL_INTERVAL_SECS", 60),
        master_interval_secs: get_u64("MASTER_INTERVAL_SECS", 3600),
        post_end_window_secs: get_u64("POST_END_WINDOW_SECS", 3600),

        api_key: {
            let key = get("API_KEY", "");
            if key.is_empty() {
                None
            } else {
                Some(key)
            }
        },
    }
}
