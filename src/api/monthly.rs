//! Monthly ranking endpoints, matching GarupaSpeedTracker:
//!
//! - `GET /monthlyRanking/info` | `/info.json` → `{ [id]: MonthlyRankingInfo }`
//! - `GET /monthlyRanking/all` | `/all.json` → full detail
//! - `GET /monthlyRanking/top` | `/top.json` → `{ points, users }`
//! - `GET /monthlyRanking/border` | `/border.json` → `{ result, cutoffs }`

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::api::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::{is_monthly_border_tier, TopPoint, SERVER_COUNT};

/// Serializes the points vector directly, avoiding per-point intermediate
/// allocations on large responses.
#[derive(serde::Serialize)]
struct TopResponse {
    points: Vec<TopPoint>,
    users: Vec<Value>,
}

/// Public view of a monthly ranking, lightweight projection like
/// GarupaSpeedTracker's `toMonthlyRankingInfo`.
fn to_info_view(value: &Value) -> Value {
    json!({
        "monthlyRankingName": value.get("monthlyRankingName").cloned().unwrap_or(Value::Array(vec![])),
        "assetBundleName": value.get("assetBundleName").cloned().unwrap_or(Value::Null),
        "bgmFileName": value.get("bgmFileName").cloned().unwrap_or(Value::Null),
        "startAt": value.get("startAt").cloned().unwrap_or(Value::Array(vec![])),
        "endAt": value.get("endAt").cloned().unwrap_or(Value::Array(vec![])),
    })
}

/// Projects player documents to the public `users` entries.
fn to_users(docs: &[crate::storage::PlayerDoc]) -> Vec<Value> {
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

// ---------------------------------------------------------------------------
// GET /monthlyRanking/info
// ---------------------------------------------------------------------------

/// All monthly ranking periods, keyed by id lightweight view.
pub async fn info(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let all = state.storage.all_monthly_infos().await?;
    let mut out: Map<String, Value> = Map::new();
    for (id, doc) in all {
        out.insert(id, to_info_view(&doc));
    }
    Ok(Json(Value::Object(out)))
}

// ---------------------------------------------------------------------------
// GET /monthlyRanking/all
// ---------------------------------------------------------------------------

/// All monthly ranking periods, keyed by id full detail including rewards and grades.
pub async fn all(State(state): State<AppState>) -> AppResult<Json<Value>> {
    let all = state.storage.all_monthly_infos().await?;
    Ok(Json(Value::Object(all.into_iter().collect())))
}

// ---------------------------------------------------------------------------
// GET /monthlyRanking/top?server=0&monthlyId=N
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct TopQuery {
    pub server: Option<String>,
    pub monthlyId: Option<String>,
    pub interval: Option<String>,
    pub since: Option<String>,
}

pub async fn top(
    State(state): State<AppState>,
    Query(query): Query<TopQuery>,
) -> AppResult<impl axum::response::IntoResponse> {
    let server = parse_server(query.server.as_deref())?;
    let monthly_id = match query.monthlyId.as_deref() {
        Some(raw) => Some(parse_int(raw, "monthlyId", 1)?),
        None => state.storage.active_monthly_id(now_ms()).await?,
    };

    let Some(monthly_id) = monthly_id else {
        return Ok(Json(TopResponse {
            points: vec![],
            users: vec![],
        }));
    };

    let interval_ms = match query.interval.as_deref() {
        Some(raw) => parse_int(raw, "interval", 1)?,
        None => 60_000,
    };
    let since = match query.since.as_deref() {
        Some(raw) => parse_int(raw, "since", 0)?,
        None => 0,
    };

    let (points, players) = state.storage.monthly_top(server, monthly_id, since).await?;
    let points = crate::api::resample_points(&points, interval_ms);
    Ok(Json(TopResponse {
        points,
        users: to_users(&players),
    }))
}

// ---------------------------------------------------------------------------
// GET /monthlyRanking/border?server=0&monthlyId=N&tier=N
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
pub struct BorderQuery {
    pub server: Option<String>,
    pub monthlyId: Option<String>,
    pub tier: Option<String>,
}

pub async fn border(
    State(state): State<AppState>,
    Query(query): Query<BorderQuery>,
) -> AppResult<Json<Value>> {
    let server = parse_server(query.server.as_deref())?;
    let tier = match query.tier.as_deref() {
        Some(raw) => parse_int(raw, "tier", 1)?,
        None => return Err(AppError::validation("tier", "tier is required")),
    };
    if !is_monthly_border_tier(tier) {
        return Err(AppError::validation(
            "tier",
            "tier must be one of: 20,30,40,50,100,200,300,500,1000,2000,3000,4000,5000",
        ));
    }

    let monthly_id = match query.monthlyId.as_deref() {
        Some(raw) => Some(parse_int(raw, "monthlyId", 1)?),
        None => state.storage.active_monthly_id(now_ms()).await?,
    };

    let Some(monthly_id) = monthly_id else {
        return Ok(Json(json!({ "result": true, "cutoffs": [] })));
    };

    let cutoffs = state
        .storage
        .monthly_border(server, monthly_id, tier)
        .await?;
    Ok(Json(json!({
        "result": true,
        "cutoffs": cutoffs.iter().map(|c| json!({ "time": c.time, "ep": c.ep })).collect::<Vec<_>>(),
    })))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_server(raw: Option<&str>) -> AppResult<i64> {
    let server = match raw {
        Some(raw) => parse_int(raw, "server", 0)?,
        None => return Err(AppError::validation("server", "server is required")),
    };
    if !(0..SERVER_COUNT).contains(&server) {
        return Err(AppError::validation(
            "server",
            format!("server must be between 0 and {}", SERVER_COUNT - 1),
        ));
    }
    Ok(server)
}

pub(crate) fn parse_int(raw: &str, field: &str, min: i64) -> AppResult<i64> {
    let value = raw
        .parse::<i64>()
        .map_err(|_| AppError::validation(field, format!("{field} must be an integer")))?;
    if value < min {
        return Err(AppError::validation(
            field,
            format!("{field} must be >= {min}"),
        ));
    }
    Ok(value)
}
