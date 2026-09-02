//! Direct, request-scoped client for the official Garupa API.
//!
//! The normal ranking collector still uses the configured Dream-API service.
//! Profile export is different: a browser request supplies a player's UID,
//! X-Signature UUID and client platform, so the request must be signed and
//! decrypted with those values without changing process-wide configuration.

mod decoder;
mod schema;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, KeyIvInit};
use aes::Aes128;
use cbc::Decryptor;
use serde_json::{json, Value};

use crate::config::Config;
use crate::error::{AppError, AppResult};

type Aes128CbcDec = Decryptor<Aes128>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Ios,
    Android,
}

impl Platform {
    pub fn parse(raw: &str) -> AppResult<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ios" | "iphone" | "ipad" => Ok(Self::Ios),
            "android" => Ok(Self::Android),
            _ => Err(AppError::validation(
                "platform",
                "platform must be either iOS or Android",
            )),
        }
    }

    fn header_value(self) -> &'static str {
        match self {
            Self::Ios => "iOS",
            Self::Android => "Android",
        }
    }
}

/// Credentials used only for one profile export request.
#[derive(Clone)]
pub struct Credentials {
    pub uid: String,
    pub uuid: String,
    pub platform: Platform,
}

impl Credentials {
    pub fn from_input(uid: &str, uuid: &str, platform: &str) -> AppResult<Self> {
        let uid = uid.trim();
        if uid.is_empty() || uid.len() > 20 || !uid.chars().all(|ch| ch.is_ascii_digit()) {
            return Err(AppError::validation(
                "uid",
                "uid must contain 1 to 20 ASCII digits",
            ));
        }

        let uuid = uuid.trim();
        if uuid.is_empty()
            || uuid.len() > 256
            || !uuid.is_ascii()
            || uuid.chars().any(|ch| ch.is_ascii_control())
        {
            return Err(AppError::validation(
                "uuid",
                "uuid must be a non-empty ASCII header value of at most 256 characters",
            ));
        }

        Ok(Self {
            uid: uid.to_string(),
            uuid: uuid.to_string(),
            platform: Platform::parse(platform)?,
        })
    }
}

/// All decoded data needed by the Bestdori profile mapper.
pub struct ProfileSnapshot {
    pub profile: Value,
    pub situations: Value,
    pub episodes: Value,
    pub cards: Value,
    pub areas: Value,
    pub characters: Value,
}

#[derive(Clone)]
pub struct ProfileClient {
    http: reqwest::Client,
    base: String,
    encryption_key: Option<Vec<u8>>,
    encryption_iv: Option<Vec<u8>>,
    client_version: String,
    android_client_version: String,
    unity_version: String,
    user_agent: String,
    package_url: String,
    version_ttl: Duration,
    version_cache: Arc<Mutex<Option<(String, Instant)>>>,
}

impl ProfileClient {
    pub fn new(cfg: &Config) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(cfg.garupa_timeout)
            .build()
            .map_err(|e| AppError::internal(format!("failed to build profile HTTP client: {e}")))?;

        Ok(Self {
            http,
            base: cfg.garupa_base.clone(),
            encryption_key: cfg.garupa_encryption_key.clone(),
            encryption_iv: cfg.garupa_encryption_iv.clone(),
            client_version: cfg.garupa_client_version.clone(),
            android_client_version: cfg.garupa_android_client_version.clone(),
            unity_version: cfg.garupa_unity_version.clone(),
            user_agent: cfg.garupa_user_agent.clone(),
            package_url: cfg.garupa_package_url.clone(),
            version_ttl: cfg.garupa_version_ttl,
            version_cache: Arc::new(Mutex::new(None)),
        })
    }

    /// Fetches the five official responses required to build a Bestdori file.
    ///
    /// The credential object is borrowed for the duration of the request and
    /// is never placed in a cache or persisted store.
    pub async fn fetch(&self, credentials: &Credentials) -> AppResult<ProfileSnapshot> {
        self.ensure_configured()?;
        let client_version = self.client_version(credentials.platform).await;
        let profile_path = format!("user/{}", credentials.uid);
        let situations_path = format!("user/{}/situation", credentials.uid);
        let episodes_path = format!("user/{}/episode", credentials.uid);
        let suite_path = format!("suite/user/{}", credentials.uid);

        let profile = self.get_decoded(
            &profile_path,
            "user profile",
            credentials,
            &client_version,
            &schema::USER_PROFILE_RESPONSE_SCHEMA,
        );
        let situations = self.get_decoded(
            &situations_path,
            "owned cards",
            credentials,
            &client_version,
            &schema::USER_SITUATION_LIST_SCHEMA,
        );
        let episodes = self.get_decoded(
            &episodes_path,
            "episodes",
            credentials,
            &client_version,
            &schema::USER_EPISODE_LIST_SCHEMA,
        );
        let cards = self.get_decoded(
            "situation",
            "card master",
            credentials,
            &client_version,
            &schema::SITUATION_LIST_SCHEMA,
        );
        let suite = self.get_decoded(
            &suite_path,
            "user suite",
            credentials,
            &client_version,
            &schema::SUITE_USER_RESPONSE_SCHEMA,
        );

        let (profile, situations, episodes, cards, suite) =
            tokio::try_join!(profile, situations, episodes, cards, suite)?;

        Ok(ProfileSnapshot {
            profile,
            situations,
            episodes,
            cards,
            areas: suite_area_entries(&suite),
            characters: suite_character_entries(&suite),
        })
    }

    fn ensure_configured(&self) -> AppResult<()> {
        if self.base.is_empty() {
            return Err(AppError::unavailable(
                "profile export is not configured: GARUPA_SERVER_BASES is empty",
            ));
        }
        if self.encryption_key.as_ref().map(Vec::len) != Some(16)
            || self.encryption_iv.as_ref().map(Vec::len) != Some(16)
        {
            return Err(AppError::unavailable(
                "profile export is not configured: encryption key and IV must each be 16 bytes",
            ));
        }
        Ok(())
    }

    async fn get_decoded(
        &self,
        path: &str,
        label: &str,
        credentials: &Credentials,
        client_version: &str,
        response_schema: &'static schema::Schema,
    ) -> AppResult<Value> {
        let url = format!("{}{}", self.base, path.trim_start_matches('/'));
        let response = self
            .http
            .get(url)
            .header("User-Agent", &self.user_agent)
            .header("X-Unity-Version", &self.unity_version)
            .header("X-ClientPlatform", credentials.platform.header_value())
            .header("X-ClientVersion", client_version)
            .header("X-Signature", &credentials.uuid)
            .header("Accept-Encoding", "deflate, gzip")
            .header("Content-Type", "application/octet-stream")
            .header("Accept", "application/octet-stream")
            .send()
            .await
            .map_err(|_| AppError::upstream(format!("official Garupa {label} request failed")))?;

        let status = response.status();
        if !status.is_success() {
            return Err(AppError::upstream(format!(
                "official Garupa {label} request failed (HTTP {status})"
            )));
        }

        let body = response.bytes().await.map_err(|_| {
            AppError::upstream(format!(
                "official Garupa {label} response could not be read"
            ))
        })?;
        let decrypted = self.decrypt(&body)?;
        decoder::decode(&decrypted, response_schema).map_err(|e| {
            AppError::upstream(format!(
                "official Garupa {label} response could not be decoded: {e}"
            ))
        })
    }

    fn decrypt(&self, payload: &[u8]) -> AppResult<Vec<u8>> {
        let key = self.encryption_key.as_deref().ok_or_else(|| {
            AppError::unavailable("profile export encryption key is not configured")
        })?;
        let iv = self.encryption_iv.as_deref().ok_or_else(|| {
            AppError::unavailable("profile export encryption IV is not configured")
        })?;

        #[allow(clippy::manual_is_multiple_of)]
        if payload.is_empty() || payload.len() % 16 != 0 {
            return Err(AppError::upstream(
                "official Garupa response had an invalid encrypted payload",
            ));
        }

        let mut buffer = payload.to_vec();
        let decryptor = Aes128CbcDec::new_from_slices(key, iv)
            .map_err(|_| AppError::unavailable("profile export encryption settings are invalid"))?;
        let plaintext = decryptor
            .decrypt_padded_mut::<NoPadding>(&mut buffer)
            .map_err(|_| AppError::upstream("official Garupa response could not be decrypted"))?;
        Ok(plaintext.to_vec())
    }

    async fn client_version(&self, platform: Platform) -> String {
        let configured = match platform {
            Platform::Ios => &self.client_version,
            Platform::Android => &self.android_client_version,
        };

        if platform != Platform::Ios || self.package_url.trim().is_empty() {
            return configured.clone();
        }

        if let Ok(cache) = self.version_cache.lock() {
            if let Some((version, at)) = cache.as_ref() {
                if at.elapsed() < self.version_ttl {
                    return version.clone();
                }
            }
        }

        let version = self
            .fetch_store_version()
            .await
            .unwrap_or_else(|| configured.clone());
        if let Ok(mut cache) = self.version_cache.lock() {
            *cache = Some((version.clone(), Instant::now()));
        }
        version
    }

    async fn fetch_store_version(&self) -> Option<String> {
        let response = self
            .http
            .get(&self.package_url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let body: Value = response.json().await.ok()?;
        body.pointer("/results/0/version")
            .and_then(Value::as_str)
            .filter(|version| !version.trim().is_empty())
            .map(ToOwned::to_owned)
    }
}

fn suite_area_entries(suite: &Value) -> Value {
    let entries = suite
        .pointer("/userAreaItemMap/entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("value").cloned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({ "entries": entries })
}

fn suite_character_entries(suite: &Value) -> Value {
    let entries = suite
        .pointer("/userCharacterPotentialLevelMap/entries")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    Some(json!({
                        "characterId": entry.get("key")?.clone(),
                        "potentialLevel": entry.get("value")?.clone(),
                    }))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({ "entries": entries })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    use axum::body::Body;
    use axum::extract::{Request, State};
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use axum::Router;
    use cbc::Encryptor;

    #[test]
    fn validates_request_credentials_without_logging_or_normalizing_secrets() {
        let credentials = Credentials::from_input(" 123 ", "uuid-value", "android").unwrap();
        assert_eq!(credentials.uid, "123");
        assert_eq!(credentials.uuid, "uuid-value");
        assert_eq!(credentials.platform, Platform::Android);
        assert!(Credentials::from_input("12/3", "uuid", "iOS").is_err());
        assert!(Credentials::from_input("123", "uuid", "windows").is_err());
    }

    #[test]
    fn maps_suite_maps_to_profile_mapper_shape() {
        let suite = json!({
            "userAreaItemMap": {
                "entries": [{ "key": 1, "value": {
                    "areaItemCategory": 2, "level": 8
                }}]
            },
            "userCharacterPotentialLevelMap": {
                "entries": [{ "key": 40, "value": {
                    "performanceLevel": 50, "techniqueLevel": 40, "visualLevel": 30
                }}]
            }
        });

        let areas = suite_area_entries(&suite);
        let characters = suite_character_entries(&suite);
        assert_eq!(areas["entries"][0]["areaItemCategory"], 2);
        assert_eq!(characters["entries"][0]["characterId"], 40);
        assert_eq!(
            characters["entries"][0]["potentialLevel"]["visualLevel"],
            30
        );
    }

    fn varint(mut value: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            bytes.push(byte);
            if value == 0 {
                return bytes;
            }
        }
    }

    fn field_int(field: u32, value: u64) -> Vec<u8> {
        let mut bytes = varint(u64::from(field) << 3);
        bytes.extend(varint(value));
        bytes
    }

    fn field_bytes(field: u32, value: &[u8]) -> Vec<u8> {
        let mut bytes = varint((u64::from(field) << 3) | 2);
        bytes.extend(varint(value.len() as u64));
        bytes.extend(value);
        bytes
    }

    fn join(parts: &[Vec<u8>]) -> Vec<u8> {
        parts.iter().flat_map(|part| part.iter().copied()).collect()
    }

    fn encrypted(mut plaintext: Vec<u8>) -> Vec<u8> {
        plaintext.resize(plaintext.len().next_multiple_of(16).max(16), 0);
        let mut buffer = plaintext;
        let encryptor =
            Encryptor::<Aes128>::new_from_slices(b"0123456789abcdef", b"abcdef0123456789").unwrap();
        let length = buffer.len();
        encryptor
            .encrypt_padded_mut::<NoPadding>(&mut buffer, length)
            .unwrap();
        buffer
    }

    fn mock_payload(path: &str) -> Vec<u8> {
        let payload = match path {
            "/user/77" => {
                let profile = join(&[field_int(1, 77), field_bytes(3, b"Tester")]);
                field_bytes(1, &profile)
            }
            "/user/77/situation" => {
                let owned = join(&[
                    field_int(2, 1001),
                    field_int(3, 60),
                    field_bytes(7, b"completed"),
                    field_bytes(9, b"normal"),
                    field_int(11, 5),
                    field_int(13, 2),
                ]);
                field_bytes(1, &owned)
            }
            "/user/77/episode" => Vec::new(),
            "/situation" => {
                let episode = field_int(1, 9001);
                let episode_list = field_bytes(1, &episode);
                let card = join(&[field_int(1, 1001), field_bytes(14, &episode_list)]);
                field_bytes(1, &card)
            }
            "/suite/user/77" => {
                let area = join(&[
                    field_int(1, 77),
                    field_int(2, 1),
                    field_int(3, 1),
                    field_int(4, 8),
                ]);
                let area_entry = join(&[field_int(1, 1), field_bytes(2, &area)]);
                let area_map = field_bytes(1, &area_entry);

                let potential = join(&[field_int(1, 20), field_int(2, 15), field_int(3, 20)]);
                let potential_entry = join(&[field_int(1, 1), field_bytes(2, &potential)]);
                let potential_map = field_bytes(1, &potential_entry);
                join(&[field_bytes(22, &area_map), field_bytes(401, &potential_map)])
            }
            _ => Vec::new(),
        };
        encrypted(payload)
    }

    type RequestRecords = Arc<Mutex<Vec<(String, String, String, String)>>>;

    async fn mock_official(State(records): State<RequestRecords>, request: Request) -> Response {
        let path = request.uri().path().to_string();
        let platform = request
            .headers()
            .get("X-ClientPlatform")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let signature = request
            .headers()
            .get("X-Signature")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let version = request
            .headers()
            .get("X-ClientVersion")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        records
            .lock()
            .unwrap()
            .push((path.clone(), platform, signature, version));
        let status = if matches!(
            path.as_str(),
            "/user/77"
                | "/user/77/situation"
                | "/user/77/episode"
                | "/situation"
                | "/suite/user/77"
        ) {
            StatusCode::OK
        } else {
            StatusCode::NOT_FOUND
        };
        (status, Body::from(mock_payload(&path))).into_response()
    }

    #[tokio::test]
    async fn fetches_and_decodes_a_request_scoped_profile_for_each_platform() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let records: RequestRecords = Arc::new(Mutex::new(Vec::new()));
        let server_records = records.clone();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new()
                    .fallback(mock_official)
                    .with_state(server_records),
            )
            .await
            .unwrap();
        });

        let client = ProfileClient {
            http: reqwest::Client::new(),
            base: format!("http://{address}/"),
            encryption_key: Some(b"0123456789abcdef".to_vec()),
            encryption_iv: Some(b"abcdef0123456789".to_vec()),
            client_version: "test-ios".to_string(),
            android_client_version: "test-android".to_string(),
            unity_version: "test-unity".to_string(),
            user_agent: "test-agent".to_string(),
            package_url: String::new(),
            version_ttl: Duration::from_secs(3600),
            version_cache: Arc::new(Mutex::new(None)),
        };

        let ios = Credentials::from_input("77", "ios-secret", "iOS").unwrap();
        let snapshot = client.fetch(&ios).await.unwrap();
        assert_eq!(snapshot.profile["profile"]["userName"], "Tester");
        assert_eq!(snapshot.situations["entries"][0]["situationId"], 1001);
        assert_eq!(snapshot.areas["entries"][0]["areaItemCategory"], 1);
        assert_eq!(
            snapshot.characters["entries"][0]["potentialLevel"]["visualLevel"],
            20
        );

        let android = Credentials::from_input("77", "android-secret", "Android").unwrap();
        client.fetch(&android).await.unwrap();

        server.abort();

        let records = records.lock().unwrap();
        assert_eq!(records.len(), 10);
        let expected_paths = [
            "/user/77",
            "/user/77/situation",
            "/user/77/episode",
            "/situation",
            "/suite/user/77",
        ];
        for (path, platform, signature, version) in records.iter() {
            assert!(expected_paths.contains(&path.as_str()));
            if signature == "ios-secret" {
                assert_eq!(platform, "iOS");
                assert_eq!(version, "test-ios");
            } else {
                assert_eq!(signature, "android-secret");
                assert_eq!(platform, "Android");
                assert_eq!(version, "test-android");
            }
        }
    }
}
