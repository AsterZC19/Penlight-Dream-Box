//! HTTP client for the already-encapsulated Penlight-Dream-API service.
//!
//! Box never talks to the official Garupa servers itself: all AES decryption,
//! protobuf decoding, client-version detection and request signing happen
//! inside Penlight-Dream-API. This module only consumes its JSON endpoints:
//!
//! - `GET {base}/{server}/monthly-ranking` → master list
//! - `GET {base}/{server}/monthly-ranking/{id}` → full report with top and border users
//! - `GET {base}/{server}/events` → event master list
//! - `GET {base}/{server}/events/{id}/ranking?type=&mid=` → event ranking report
//! - `GET {base}/{server}/user/*` and `GET {base}/{server}/cards` → profile data

use serde::Deserialize;
use serde_json::Value;
use tracing::{debug, warn};

use crate::config::Config;
use crate::error::{AppError, AppResult};

/// A ranking user, field-for-field aligned with GarupaSpeedTracker's
/// `RankingUserRaw`, same shape as Penlight-Dream-API's `RankingUser`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankingUser {
    pub uid: i64,
    pub name: String,
    pub introduction: String,
    /// Player level `rankLevel`.
    pub rank: i64,
    /// Displayed card ID.
    pub sid: i64,
    /// 1 when the displayed card is the after-training illustration.
    pub strained: i64,
    /// Equipped profile degree IDs.
    pub degrees: Vec<i64>,
    /// Ranking position.
    pub tier: i64,
    /// Ranking points.
    pub point: i64,
}

/// Monthly ranking master entry, JP single value from Dream-API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyRankingInfo {
    pub monthly_ranking_id: i64,
    pub monthly_ranking_name: String,
    pub asset_bundle_name: String,
    pub bgm_file_name: String,
    pub start_at: i64,
    pub end_at: i64,
    pub enable_flg: bool,
    pub public_start_at: i64,
    pub public_end_at: i64,
    pub distribution_start_at: i64,
    pub distribution_end_at: i64,
    pub reception_end_at: i64,
    pub aggregate_end_at: i64,
    #[serde(default)]
    pub rewards: Vec<Value>,
    #[serde(default)]
    pub grades: Vec<Value>,
}

/// Full monthly ranking report: `{monthlyRankingPointTopUsers, monthlyRankingPointBorderUsers}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyFull {
    #[serde(default)]
    pub monthly_ranking_point_top_users: Vec<RankingUser>,
    #[serde(default)]
    pub monthly_ranking_point_border_users: Vec<RankingUser>,
}

/// Event master entry, JP single value from Dream-API.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventInfo {
    pub event_id: i64,
    pub event_type: String,
    pub event_name: String,
    pub asset_bundle_name: String,
    pub start_at: i64,
    pub end_at: i64,
    pub enable_flg: bool,
    pub public_start_at: i64,
    pub public_end_at: i64,
    pub distribution_start_at: i64,
    pub distribution_end_at: i64,
    pub bgm_asset_bundle_name: String,
    pub bgm_file_name: String,
    pub aggregate_end_at: i64,
    pub event_exchanges_end_at: i64,
    pub reception_end_at: i64,
    pub previous_event_id: i64,
}

/// Per-song ranking inside an event ranking report.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicRanking {
    pub music_id: i64,
    #[serde(default)]
    pub score_top_users: Vec<RankingUser>,
    #[serde(default)]
    pub score_border_users: Vec<RankingUser>,
}

/// Event ranking report: `{eventType, eventPointTopUsers, eventPointBorderUsers, musicRankings}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventReport {
    pub event_type: String,
    #[serde(default)]
    pub event_point_top_users: Vec<RankingUser>,
    #[serde(default)]
    pub event_point_border_users: Vec<RankingUser>,
    #[serde(default)]
    pub music_rankings: Vec<MusicRanking>,
}

/// Wrapper for endpoints that respond with `{ "entries": [...] }`.
#[derive(Debug, Deserialize)]
struct Entries<T> {
    entries: Vec<T>,
}

/// Upstream Penlight-Dream-API client. Cheap to clone, shares the reqwest pool.
#[derive(Clone)]
pub struct Upstream {
    base: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl Upstream {
    pub fn new(cfg: &Config) -> AppResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(cfg.dream_api_timeout)
            .build()
            .map_err(|e| AppError::internal(format!("failed to build HTTP client: {e}")))?;
        Ok(Self {
            base: cfg.dream_api_base.clone(),
            api_key: cfg.dream_api_key.clone(),
            http,
        })
    }

    async fn get_json(&self, path: &str) -> AppResult<Value> {
        let url = format!("{}/{}", self.base, path.trim_start_matches('/'));
        let request = self.http.get(&url);
        let request = if let Some(api_key) = &self.api_key {
            request.header("X-API-Key", api_key)
        } else {
            request
        };
        let resp = request
            .send()
            .await
            .map_err(|e| AppError::upstream(format!("request failed: {url}: {e}")))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::upstream(format!("read body failed: {url}: {e}")))?;
        if !status.is_success() {
            let snippet: String = body.chars().take(500).collect();
            return Err(AppError::upstream(format!(
                "HTTP {status}: {url}: {snippet}"
            )));
        }
        serde_json::from_str(&body)
            .map_err(|e| AppError::upstream(format!("invalid JSON from {url}: {e}")))
    }

    /// `GET {base}/jp/monthly-ranking` → monthly ranking master list.
    pub async fn monthly_master(&self) -> AppResult<Vec<MonthlyRankingInfo>> {
        let value = self.get_json("jp/monthly-ranking").await?;
        let parsed: Entries<MonthlyRankingInfo> = serde_json::from_value(value)
            .map_err(|e| AppError::upstream(format!("monthly master parse failed: {e}")))?;
        debug!("monthly master: {} entries", parsed.entries.len());
        Ok(parsed.entries)
    }

    /// `GET {base}/jp/monthly-ranking/{id}` → full top/border report.
    pub async fn monthly_full(&self, monthly_id: i64) -> AppResult<MonthlyFull> {
        let value = self
            .get_json(&format!("jp/monthly-ranking/{monthly_id}"))
            .await?;
        serde_json::from_value(value)
            .map_err(|e| AppError::upstream(format!("monthly {monthly_id} parse failed: {e}")))
    }

    /// `GET {base}/jp/events` → event master list.
    pub async fn event_master(&self) -> AppResult<Vec<EventInfo>> {
        let value = self.get_json("jp/events").await?;
        let parsed: Entries<EventInfo> = serde_json::from_value(value)
            .map_err(|e| AppError::upstream(format!("event master parse failed: {e}")))?;
        debug!("event master: {} entries", parsed.entries.len());
        Ok(parsed.entries)
    }

    /// `GET {base}/jp/events/{id}/ranking?type=&mid=` → event ranking report.
    ///
    /// The event type is auto-resolved by Dream-API when omitted; `mid` is
    /// passed through for per-song sub-rankings.
    pub async fn event_ranking(&self, event_id: i64, mid: Option<i64>) -> AppResult<EventReport> {
        let mut path = format!("jp/events/{event_id}/ranking");
        if let Some(mid) = mid {
            path.push_str(&format!("?mid={mid}"));
        }
        let value = self.get_json(&path).await?;
        serde_json::from_value(value)
            .map_err(|e| AppError::upstream(format!("event {event_id} ranking parse failed: {e}")))
    }

    /// `GET {base}/jp/user/profile` → the configured player's profile.
    pub async fn user_profile(&self) -> AppResult<Value> {
        self.get_json("jp/user/profile").await
    }

    /// `GET {base}/jp/user/situations` → the configured player's owned cards.
    pub async fn user_situations(&self) -> AppResult<Value> {
        self.get_json("jp/user/situations").await
    }

    /// `GET {base}/jp/user/episodes` → unlocked/read episode records.
    pub async fn user_episodes(&self) -> AppResult<Value> {
        self.get_json("jp/user/episodes").await
    }

    /// `GET {base}/jp/cards` → card master data, including card episodes.
    pub async fn cards(&self) -> AppResult<Value> {
        self.get_json("jp/cards").await
    }

    /// `GET {base}/jp/user/areas` → enabled area items with category and level.
    pub async fn user_areas(&self) -> AppResult<Value> {
        self.get_json("jp/user/areas").await
    }

    /// `GET {base}/jp/user/characters` → character rank and released potential.
    pub async fn user_characters(&self) -> AppResult<Value> {
        self.get_json("jp/user/characters").await
    }

    /// Health probe used by the collector before starting. The Dream-API
    /// serves `/health` outside its API prefix, so probe the origin directly.
    pub async fn ping(&self) -> bool {
        let Ok(base) = reqwest::Url::parse(&self.base) else {
            return false;
        };
        let url = format!("{}/health", base.origin().ascii_serialization());
        match self.http.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(e) => {
                warn!("Dream-API health probe failed ({url}): {e}");
                false
            }
        }
    }
}
