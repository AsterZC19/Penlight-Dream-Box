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

fn get_optional_bytes(name: &str) -> Option<Vec<u8>> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().as_bytes().to_vec())
        .filter(|value| !value.is_empty())
}

fn normalize_garupa_base(raw: &str) -> String {
    let raw = raw.trim().trim_end_matches('/');
    if raw.is_empty() {
        return String::new();
    }
    if raw.starts_with("http://") || raw.starts_with("https://") {
        if raw.ends_with("/api") {
            format!("{raw}/")
        } else {
            format!("{raw}/api/")
        }
    } else {
        format!("https://{raw}/api/")
    }
}

/// Box runtime configuration.
#[derive(Clone)]
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
    /// Optional API key used when Box calls Dream-API. Falls back to
    /// `API_KEY` so the bundled Docker setup can use one shared secret.
    pub dream_api_key: Option<String>,
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

    /// Direct profile-export client settings. The encryption material is read
    /// by Box as server-side configuration and is never sent to the browser.
    pub garupa_base: String,
    pub garupa_encryption_key: Option<Vec<u8>>,
    pub garupa_encryption_iv: Option<Vec<u8>>,
    pub garupa_client_version: String,
    pub garupa_android_client_version: String,
    pub garupa_unity_version: String,
    pub garupa_user_agent: String,
    pub garupa_package_url: String,
    pub garupa_timeout: Duration,
    pub garupa_version_ttl: Duration,
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
        dream_api_key: {
            let key = get_first(&["DREAM_API_KEY", "API_KEY"], "");
            if key.is_empty() {
                None
            } else {
                Some(key)
            }
        },
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

        garupa_base: normalize_garupa_base(&get("GARUPA_SERVER_BASES", "api.garupa.jp")),
        garupa_encryption_key: get_optional_bytes("GARUPA_ENCRYPTION_KEYS"),
        garupa_encryption_iv: get_optional_bytes("GARUPA_ENCRYPTION_IVS"),
        garupa_client_version: get_first(&["GARUPA_CLIENT_VERSIONS"], "10.1.3"),
        garupa_android_client_version: get_first(
            &["GARUPA_ANDROID_CLIENT_VERSIONS", "GARUPA_CLIENT_VERSIONS"],
            "10.1.3",
        ),
        garupa_unity_version: get_first(&["GARUPA_UNITY_VERSIONS"], "2021.3.45f2"),
        garupa_user_agent: {
            let unity = get_first(&["GARUPA_UNITY_VERSIONS"], "2021.3.45f2");
            get_first(
                &["GARUPA_USER_AGENTS"],
                &format!("UnityPlayer/{unity} (UnityWebRequest/1.0, libcurl/8.5.0-DEV)"),
            )
        },
        garupa_package_url: get_first(
            &["GARUPA_PACKAGE_URLS"],
            "https://itunes.apple.com/jp/lookup?bundleId=jp.co.craftegg.band",
        ),
        garupa_timeout: Duration::from_secs(get_u64(
            "GARUPA_PROFILE_TIMEOUT_SECS",
            get_u64("DREAM_API_TIMEOUT_SECS", 30),
        )),
        garupa_version_ttl: Duration::from_secs(get_u64("GARUPA_VERSION_TTL_SECONDS", 3600)),
    }
}
