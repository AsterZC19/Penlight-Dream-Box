//! GarupaSpeedTracker-compatible API layer.
//!
//! Route shapes, query semantics, response bodies and error formats follow
//! GarupaSpeedTracker so existing consumers can point their
//! tracker base URL at this service without changes.

pub mod auth;
pub mod event;
pub mod monthly;
pub mod profile;
pub mod web;

use axum::extract::{DefaultBodyLimit, FromRef};
use axum::routing::get;
use axum::Router;

use crate::config::Config;
use crate::storage::Storage;

/// Shared application state handed to handlers.
#[derive(Clone)]
pub struct AppState {
    pub storage: Storage,
    pub config: Config,
    pub upstream: crate::upstream::Upstream,
    pub profile_client: crate::garupa::ProfileClient,
}

impl FromRef<AppState> for Storage {
    fn from_ref(state: &AppState) -> Self {
        state.storage.clone()
    }
}

impl FromRef<AppState> for Config {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

/// Resamples per-minute points to `interval_ms` buckets, keeping the last
/// point of each uid and bucket, matching Bestdori's `interval` semantics.
pub(crate) fn resample_points(
    points: &[crate::storage::TopPoint],
    interval_ms: i64,
) -> Vec<crate::storage::TopPoint> {
    if interval_ms <= 60_000 {
        return points.to_vec();
    }
    let mut map: std::collections::BTreeMap<(i64, i64), crate::storage::TopPoint> =
        std::collections::BTreeMap::new();
    for p in points {
        let bucket = p.time / interval_ms;
        map.insert((p.uid, bucket), p.clone());
    }
    let mut out: Vec<crate::storage::TopPoint> = map.into_values().collect();
    out.sort_by_key(|p| p.time);
    out
}

/// Builds the top-level router; the API prefix is applied by the caller.
pub fn build_router(state: AppState, cfg: &Config) -> Router {
    let api = Router::new()
        // Monthly ranking per GarupaSpeedTracker contract
        .route("/monthlyRanking/info", get(monthly::info))
        .route("/monthlyRanking/info.json", get(monthly::info))
        .route("/monthlyRanking/all", get(monthly::all))
        .route("/monthlyRanking/all.json", get(monthly::all))
        .route("/monthlyRanking/top", get(monthly::top))
        .route("/monthlyRanking/top.json", get(monthly::top))
        .route("/monthlyRanking/border", get(monthly::border))
        .route("/monthlyRanking/border.json", get(monthly::border))
        // Event ranking per GarupaSpeedTracker contract
        .route("/eventtop/data", get(event::eventtop_data))
        .route("/tracker/data", get(event::tracker_data))
        .route("/events", get(event::events))
        // Bestdori Profile Manager import data
        .route(
            "/profile/export",
            get(profile::export).post(profile::export_for_credentials),
        )
        .route(
            "/profile/export.json",
            get(profile::export).post(profile::export_for_credentials),
        )
        .layer(DefaultBodyLimit::max(32 * 1024))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::require_api_key,
        ));

    // Health check outside the API prefix.
    Router::new()
        .route("/", get(web::index))
        .route("/assets/app.css", get(web::styles))
        .route("/assets/app.js", get(web::script))
        .route(
            "/health",
            get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }),
        )
        .nest(&cfg.api_prefix, api)
        .layer(tower_http::compression::CompressionLayer::new())
        .with_state(state)
}
