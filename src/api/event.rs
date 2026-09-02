//! Event ranking endpoints, matching GarupaSpeedTracker:
//!
//! - `GET /eventtop/data?server=0&event=N[&mid=N]` → `{ points, users }`
//! - `GET /tracker/data?server=0&event=N&tier=N[&mid=N]` → `{ result, cutoffs }`
//! - `GET /events` → event list keyed by id
//!
//! When no local snapshot exists, `/eventtop/data` and `/tracker/data` redirect
//! to `https://bestdori.com{original_url}`, exactly like GarupaSpeedTracker.

use axum::extract::{Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::monthly::parse_int;
use crate::api::resample_points;
use crate::api::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::TopPoint;
use crate::storage::{is_event_border_tier, is_music_border_tier, CutoffPoint, PlayerDoc};

const BESTDORI_BASE: &str = "https://bestdori.com";

/// Serializes the points vector directly, avoiding per-point intermediate
/// allocations on large responses.
#[derive(serde::Serialize)]
struct TopResponse {
    points: Vec<TopPoint>,
    users: Vec<Value>,
}

fn to_users(docs: &[PlayerDoc]) -> Vec<Value> {
    docs.iter()
        .map(|p| {
            json!({
                "uid": p.uid,
                "name": p.name,
                "introduction": p.introduction,
                "rank": p.rank,
                "sid": p.sid,
                "strained": p.strained,
                "degrees": p.degrees,
            })
        })
        .collect()
}

/// 302 redirect to Bestdori, matching GarupaSpeedTracker's fallback behavior.
fn bestdori_redirect(uri: &Uri) -> Response {
    let suffix = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("");
    let location = format!("{BESTDORI_BASE}{suffix}");
    (
        StatusCode::FOUND,
        [(axum::http::header::LOCATION, location)],
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /eventtop/data?server=0&event=N[&mid=N]
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EventTopQuery {
    pub server: Option<String>,
    pub event: Option<String>,
    pub mid: Option<String>,
    pub interval: Option<String>,
    pub since: Option<String>,
}

pub async fn eventtop_data(
    State(state): State<AppState>,
    Query(query): Query<EventTopQuery>,
    uri: Uri,
) -> AppResult<impl IntoResponse> {
    let _ = &uri;
    let server = parse_server(query.server.as_deref())?;
    let event = match query.event.as_deref() {
        Some(raw) => parse_int(raw, "event", 1)?,
        None => return Err(AppError::validation("event", "event is required")),
    };
    let mid = match query.mid.as_deref() {
        Some(raw) => parse_int(raw, "mid", 0)?,
        None => 0,
    };

    let interval_ms = match query.interval.as_deref() {
        Some(raw) => parse_int(raw, "interval", 1)?,
        None => 60_000,
    };
    let since = match query.since.as_deref() {
        Some(raw) => parse_int(raw, "since", 0)?,
        None => 0,
    };

    let (points, players) = state.storage.event_top(server, event, mid, since).await?;
    if points.is_empty() {
        return Ok(bestdori_redirect(&uri).into_response());
    }

    let points = resample_points(&points, interval_ms);
    Ok(Json(TopResponse {
        points,
        users: to_users(&players),
    })
    .into_response())
}

// ---------------------------------------------------------------------------
// GET /tracker/data?server=0&event=N&tier=N[&mid=N]
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TrackerQuery {
    pub server: Option<String>,
    pub event: Option<String>,
    pub tier: Option<String>,
    pub mid: Option<String>,
    pub interval: Option<String>,
}

pub async fn tracker_data(
    State(state): State<AppState>,
    Query(query): Query<TrackerQuery>,
    uri: Uri,
) -> AppResult<impl IntoResponse> {
    let server = parse_server(query.server.as_deref())?;
    let event = match query.event.as_deref() {
        Some(raw) => parse_int(raw, "event", 1)?,
        None => return Err(AppError::validation("event", "event is required")),
    };
    let mid = match query.mid.as_deref() {
        Some(raw) => parse_int(raw, "mid", 0)?,
        None => 0,
    };
    let tier = match query.tier.as_deref() {
        Some(raw) => parse_int(raw, "tier", 1)?,
        None => return Err(AppError::validation("tier", "tier is required")),
    };

    let valid = if mid > 0 {
        is_music_border_tier(tier)
    } else {
        is_event_border_tier(tier)
    };
    if !valid {
        let message = if mid > 0 {
            "tier must be one of: 20,30,40,50,100,200,300,500,1000,2000,5000,10000,20000,50000,100000"
        } else {
            "tier must be one of: 20,30,40,50,100,200,300,500,1000,1500,2000,3000,4000,5000,10000,20000,30000,40000,50000,100000"
        };
        return Err(AppError::validation("tier", message));
    }

    let interval_ms = match query.interval.as_deref() {
        Some(raw) => parse_int(raw, "interval", 1)?,
        None => 60_000,
    };

    let cutoffs = state.storage.event_border(server, event, mid, tier).await?;
    if cutoffs.is_empty() {
        return Ok(bestdori_redirect(&uri).into_response());
    }

    let cutoffs = resample_cutoffs(&cutoffs, interval_ms);
    Ok(Json(json!({
        "result": true,
        "cutoffs": cutoffs.iter().map(|c| json!({ "time": c.time, "ep": c.ep })).collect::<Vec<_>>(),
    }))
    .into_response())
}

/// Resamples cutoff series to `interval_ms` buckets, keeping the last cutoff
/// of each bucket, matching Bestdori's `interval` semantics.
fn resample_cutoffs(cutoffs: &[CutoffPoint], interval_ms: i64) -> Vec<CutoffPoint> {
    if interval_ms <= 60_000 {
        return cutoffs.to_vec();
    }
    let mut map: std::collections::BTreeMap<i64, CutoffPoint> = std::collections::BTreeMap::new();
    for c in cutoffs {
        map.insert(c.time / interval_ms, c.clone());
    }
    let mut out: Vec<CutoffPoint> = map.into_values().collect();
    out.sort_by_key(|c| c.time);
    out
}

// ---------------------------------------------------------------------------
// GET /events
// ---------------------------------------------------------------------------

/// Event master list keyed by event id normalized projection.
pub async fn events(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let all = state.storage.all_events().await?;
    let mut out = serde_json::Map::new();
    for (id, doc) in all {
        out.insert(
            id,
            json!({
                "eventId": doc.get("eventId").cloned().unwrap_or(Value::Null),
                "eventType": doc.get("eventType").cloned().unwrap_or(Value::Null),
                "eventName": doc.get("eventName").cloned().unwrap_or(Value::Null),
                "assetBundleName": doc.get("assetBundleName").cloned().unwrap_or(Value::Null),
                "startAt": doc.get("startAt").cloned().unwrap_or(Value::Null),
                "endAt": doc.get("endAt").cloned().unwrap_or(Value::Null),
            }),
        );
    }
    Ok(Json(Value::Object(out)))
}

/// Accepts both the GarupaSpeedTracker numeric server index `0` and the
/// Bestdori string server name `jp` so existing Bestdori-style consumers
/// e.g. Garupa-T10's T10 tracker can point at Box unchanged.
fn parse_server(raw: Option<&str>) -> AppResult<i64> {
    match raw {
        Some("jp") | Some("0") => Ok(0),
        Some(other) => Err(AppError::validation(
            "server",
            format!("unsupported server \"{other}\": only jp/0 is served"),
        )),
        None => Err(AppError::validation("server", "server is required")),
    }
}
