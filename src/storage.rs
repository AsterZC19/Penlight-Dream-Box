//! MongoDB persistence: monthly/event top snapshots, border cutoffs, player
//! profiles, and master info, mirroring GarupaSpeedTracker's storage semantics
//! with per-day buckets, per-server+uid player upserts, per-tier cutoff series.

use std::collections::BTreeMap;

use bson::doc;
use futures::TryStreamExt;
use mongodb::options::IndexOptions;
use mongodb::{Client, Collection, Database, IndexModel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

use crate::error::{AppError, AppResult};
use crate::upstream::{EventInfo, MonthlyRankingInfo, RankingUser};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Valid monthly-ranking border tiers per GarupaSpeedTracker contract.
pub const MONTHLY_BORDER_TIERS: &[i64] = &[
    20, 30, 40, 50, 100, 200, 300, 500, 1000, 2000, 3000, 4000, 5000,
];

/// Valid event-wide border tiers per GarupaSpeedTracker contract.
pub const EVENT_BORDER_TIERS: &[i64] = &[
    20, 30, 40, 50, 100, 200, 300, 500, 1000, 1500, 2000, 3000, 4000, 5000, 10000, 20000, 30000,
    40000, 50000, 100000,
];

/// Valid per-song border tiers per GarupaSpeedTracker contract.
pub const MUSIC_BORDER_TIERS: &[i64] = &[
    20, 30, 40, 50, 100, 200, 300, 500, 1000, 2000, 5000, 10000, 20000, 50000, 100000,
];

/// Number of servers exposed by the API contract with 0 meaning JP. Only JP is served.
pub const SERVER_COUNT: i64 = 1;

pub fn is_monthly_border_tier(tier: i64) -> bool {
    MONTHLY_BORDER_TIERS.contains(&tier)
}

pub fn is_event_border_tier(tier: i64) -> bool {
    EVENT_BORDER_TIERS.contains(&tier)
}

pub fn is_music_border_tier(tier: i64) -> bool {
    MUSIC_BORDER_TIERS.contains(&tier)
}

// ---------------------------------------------------------------------------
// Collection names
// ---------------------------------------------------------------------------

const C_MONTHLY_INFO: &str = "monthly_ranking_info";
const C_MONTHLY_TOP: &str = "monthly_ranking_top_points";
const C_MONTHLY_BORDER: &str = "monthly_ranking_border_points";
const C_PLAYERS: &str = "monthly_ranking_players";
const C_EVENT_TOP: &str = "event_top_points";
const C_EVENT_BORDER: &str = "event_border_points";
const C_EVENTS: &str = "events";

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

/// A single timestamped point in a top snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopPoint {
    pub time: i64,
    pub uid: i64,
    pub value: i64,
}

/// A single timestamped border cutoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutoffPoint {
    pub time: i64,
    pub ep: i64,
}

/// Player profile stored per server and uid.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDoc {
    pub server: i64,
    pub uid: i64,
    pub name: String,
    pub introduction: String,
    pub rank: i64,
    pub sid: i64,
    pub strained: i64,
    pub degrees: Vec<i64>,
    pub updated_at: i64,
}

impl PlayerDoc {
    fn from_user(server: i64, u: &RankingUser, ts: i64) -> Self {
        Self {
            server,
            uid: u.uid,
            name: u.name.clone(),
            introduction: u.introduction.clone(),
            rank: u.rank,
            sid: u.sid,
            strained: u.strained,
            degrees: u.degrees.clone(),
            updated_at: ts,
        }
    }
}

/// Monthly ranking info document. Per-server fields are stored as 5-element
/// arrays with index 0 for JP and null elsewhere to match GarupaSpeedTracker's shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyInfoDoc {
    pub monthly_ranking_id: i64,
    pub monthly_ranking_name: Vec<Option<String>>,
    pub asset_bundle_name: String,
    pub bgm_file_name: String,
    pub start_at: Vec<Option<i64>>,
    pub end_at: Vec<Option<i64>>,
    pub enable_flag: Vec<Option<bool>>,
    pub public_start_at: Vec<Option<i64>>,
    pub public_end_at: Vec<Option<i64>>,
    pub distribution_start_at: Vec<Option<i64>>,
    pub distribution_end_at: Vec<Option<i64>>,
    pub aggregate_end_at: Vec<Option<i64>>,
    pub reception_end_at: Vec<Option<i64>>,
    #[serde(default)]
    pub rewards: Vec<Value>,
    #[serde(default)]
    pub grades: Vec<Value>,
}

/// Event document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDoc {
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

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// MongoDB-backed storage. Cheap to clone.
#[derive(Clone)]
pub struct Storage {
    monthly_info: Collection<MonthlyInfoDoc>,
    monthly_top: Collection<mongodb::bson::Document>,
    monthly_border: Collection<mongodb::bson::Document>,
    players: Collection<PlayerDoc>,
    event_top: Collection<mongodb::bson::Document>,
    event_border: Collection<mongodb::bson::Document>,
    events: Collection<EventDoc>,
}

/// Wraps a single element of a server array into the 5-slot shape.
fn server_array<T: Clone>(value: T) -> Vec<Option<T>> {
    let mut arr: Vec<Option<T>> = vec![None; 5];
    arr[0] = Some(value);
    arr
}

impl Storage {
    pub async fn connect(uri: &str, db_name: &str) -> AppResult<Self> {
        let client = Client::with_uri_str(uri)
            .await
            .map_err(|e| AppError::internal(format!("MongoDB connect failed: {e}")))?;
        let db: Database = client.database(db_name);
        let storage = Self {
            monthly_info: db.collection(C_MONTHLY_INFO),
            monthly_top: db.collection(C_MONTHLY_TOP),
            monthly_border: db.collection(C_MONTHLY_BORDER),
            players: db.collection(C_PLAYERS),
            event_top: db.collection(C_EVENT_TOP),
            event_border: db.collection(C_EVENT_BORDER),
            events: db.collection(C_EVENTS),
        };
        storage.init_indexes().await?;
        Ok(storage)
    }

    async fn init_indexes(&self) -> AppResult<()> {
        macro_rules! create_unique_index {
            ($coll:expr, $keys:expr) => {
                $coll
                    .create_index(
                        IndexModel::builder()
                            .keys($keys)
                            .options(IndexOptions::builder().unique(true).build())
                            .build(),
                    )
                    .await
                    .map_err(|e| AppError::internal(format!("index creation failed: {e}")))?;
            };
        }

        create_unique_index!(self.monthly_info, doc! { "monthlyRankingId": 1 });
        create_unique_index!(
            self.monthly_top,
            doc! { "server": 1, "monthlyId": 1, "bucket": 1 }
        );
        create_unique_index!(
            self.monthly_border,
            doc! { "server": 1, "monthlyId": 1, "tier": 1 }
        );
        create_unique_index!(self.players, doc! { "server": 1, "uid": 1 });
        create_unique_index!(
            self.event_top,
            doc! { "server": 1, "eventId": 1, "mid": 1, "bucket": 1 }
        );
        create_unique_index!(
            self.event_border,
            doc! { "server": 1, "eventId": 1, "mid": 1, "tier": 1 }
        );
        create_unique_index!(self.events, doc! { "eventId": 1 });
        debug!("storage indexes ready");
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Master lists
    // -----------------------------------------------------------------------

    /// Upserts monthly ranking info from the Dream-API master list.
    pub async fn upsert_monthly_infos(&self, infos: &[MonthlyRankingInfo]) -> AppResult<usize> {
        for info in infos {
            let doc = MonthlyInfoDoc {
                monthly_ranking_id: info.monthly_ranking_id,
                monthly_ranking_name: server_array(info.monthly_ranking_name.clone()),
                asset_bundle_name: info.asset_bundle_name.clone(),
                bgm_file_name: info.bgm_file_name.clone(),
                start_at: server_array(info.start_at),
                end_at: server_array(info.end_at),
                enable_flag: server_array(info.enable_flg),
                public_start_at: server_array(info.public_start_at),
                public_end_at: server_array(info.public_end_at),
                distribution_start_at: server_array(info.distribution_start_at),
                distribution_end_at: server_array(info.distribution_end_at),
                aggregate_end_at: server_array(info.aggregate_end_at),
                reception_end_at: server_array(info.reception_end_at),
                rewards: info.rewards.clone(),
                grades: info.grades.clone(),
            };
            self.monthly_info
                .replace_one(doc! { "monthlyRankingId": info.monthly_ranking_id }, doc)
                .upsert(true)
                .await
                .map_err(|e| AppError::internal(format!("monthly info upsert failed: {e}")))?;
        }
        Ok(infos.len())
    }

    /// All monthly infos as raw documents, for the API.
    pub async fn all_monthly_infos(&self) -> AppResult<BTreeMap<String, Value>> {
        let mut out = BTreeMap::new();
        let cursor = self
            .monthly_info
            .find(doc! {})
            .await
            .map_err(|e| AppError::internal(format!("monthly info query failed: {e}")))?;
        let docs = cursor
            .try_collect::<Vec<MonthlyInfoDoc>>()
            .await
            .map_err(|e| AppError::internal(format!("monthly info read failed: {e}")))?;
        for d in docs {
            let v = serde_json::to_value(&d)
                .map_err(|e| AppError::internal(format!("monthly info serialize failed: {e}")))?;
            out.insert(d.monthly_ranking_id.to_string(), v);
        }
        Ok(out)
    }

    /// Id of the currently active monthly ranking, if any.
    pub async fn active_monthly_id(&self, now: i64) -> AppResult<Option<i64>> {
        Ok(self
            .active_monthlies(now, 0)
            .await?
            .into_iter()
            .map(|(id, _)| id)
            .max())
    }

    /// Active monthly rankings: started, and not ended or within
    /// `post_end_window_secs` after end. Returns monthly_id and end_at pairs.
    pub async fn active_monthlies(
        &self,
        now: i64,
        post_end_window_secs: i64,
    ) -> AppResult<Vec<(i64, i64)>> {
        let docs = self
            .monthly_info
            .find(doc! {})
            .await
            .map_err(|e| AppError::internal(format!("monthly info query failed: {e}")))?
            .try_collect::<Vec<MonthlyInfoDoc>>()
            .await
            .map_err(|e| AppError::internal(format!("monthly info read failed: {e}")))?;

        Ok(docs
            .iter()
            .filter(|d| {
                let start = d.start_at.first().and_then(|v| *v).unwrap_or(0);
                let end = d.end_at.first().and_then(|v| *v).unwrap_or(0);
                start > 0 && start <= now && (end == 0 || end + post_end_window_secs * 1000 >= now)
            })
            .map(|d| {
                (
                    d.monthly_ranking_id,
                    d.end_at.first().and_then(|v| *v).unwrap_or(0),
                )
            })
            .collect())
    }

    /// Upserts events from the Dream-API event master list.
    pub async fn upsert_events(&self, events: &[EventInfo]) -> AppResult<usize> {
        for e in events {
            let doc = EventDoc {
                event_id: e.event_id,
                event_type: e.event_type.clone(),
                event_name: e.event_name.clone(),
                asset_bundle_name: e.asset_bundle_name.clone(),
                start_at: e.start_at,
                end_at: e.end_at,
                enable_flg: e.enable_flg,
                public_start_at: e.public_start_at,
                public_end_at: e.public_end_at,
                distribution_start_at: e.distribution_start_at,
                distribution_end_at: e.distribution_end_at,
                bgm_asset_bundle_name: e.bgm_asset_bundle_name.clone(),
                bgm_file_name: e.bgm_file_name.clone(),
                aggregate_end_at: e.aggregate_end_at,
                event_exchanges_end_at: e.event_exchanges_end_at,
                reception_end_at: e.reception_end_at,
                previous_event_id: e.previous_event_id,
            };
            self.events
                .replace_one(doc! { "eventId": e.event_id }, doc)
                .upsert(true)
                .await
                .map_err(|e| AppError::internal(format!("event upsert failed: {e}")))?;
        }
        Ok(events.len())
    }

    /// All events keyed by string id.
    pub async fn all_events(&self) -> AppResult<BTreeMap<String, Value>> {
        let mut out = BTreeMap::new();
        let cursor = self
            .events
            .find(doc! {})
            .await
            .map_err(|e| AppError::internal(format!("events query failed: {e}")))?;
        let docs = cursor
            .try_collect::<Vec<EventDoc>>()
            .await
            .map_err(|e| AppError::internal(format!("events read failed: {e}")))?;
        for d in docs {
            let v = serde_json::to_value(&d)
                .map_err(|e| AppError::internal(format!("event serialize failed: {e}")))?;
            out.insert(d.event_id.to_string(), v);
        }
        Ok(out)
    }

    /// Active events: started and not ended or within `post_end_window_secs`
    /// after end. Returns event_id and end_at pairs.
    pub async fn active_events(
        &self,
        now: i64,
        post_end_window_secs: i64,
    ) -> AppResult<Vec<(i64, i64)>> {
        let docs = self
            .events
            .find(doc! {})
            .await
            .map_err(|e| AppError::internal(format!("events query failed: {e}")))?
            .try_collect::<Vec<EventDoc>>()
            .await
            .map_err(|e| AppError::internal(format!("events read failed: {e}")))?;

        Ok(docs
            .iter()
            .filter(|d| {
                d.start_at > 0
                    && d.start_at <= now
                    && (d.end_at == 0 || d.end_at + post_end_window_secs * 1000 >= now)
            })
            .map(|d| (d.event_id, d.end_at))
            .collect())
    }

    // -----------------------------------------------------------------------
    // Monthly ranking snapshots
    // -----------------------------------------------------------------------

    /// Appends a top
    /// snapshot for `` and upserts player profiles.
    pub async fn append_monthly_top(
        &self,
        server: i64,
        monthly_id: i64,
        ts: i64,
        users: &[RankingUser],
    ) -> AppResult<usize> {
        let bucket = utc_bucket(ts);
        let filter = doc! { "server": server, "monthlyId": monthly_id, "bucket": bucket };
        let existing = self
            .monthly_top
            .find_one(filter.clone())
            .await
            .map_err(|e| AppError::internal(format!("monthly top query failed: {e}")))?;

        let points: Vec<TopPoint> = users
            .iter()
            .map(|u| TopPoint {
                time: ts,
                uid: u.uid,
                value: u.point,
            })
            .collect();
        let points_bson = serde_to_bson(&points)?;
        let n = points.len();

        match existing {
            None => {
                let doc = doc! {
                    "server": server, "monthlyId": monthly_id, "bucket": bucket,
                    "points": points_bson,
                    "updatedAt": ts,
                };
                self.monthly_top
                    .insert_one(doc)
                    .await
                    .map_err(|e| AppError::internal(format!("monthly top insert failed: {e}")))?;
            }
            Some(doc) => {
                let last = doc.get_i64("updatedAt").unwrap_or(0);
                let update = if last < ts {
                    // Append when the new timestamp is newer.
                    doc! {
                        "$set": { "updatedAt": ts },
                        "$push": { "points": { "$each": points_bson } },
                    }
                } else {
                    // Same timestamp e.g. post-end clamp: replace to stay idempotent.
                    doc! { "$set": { "points": points_bson, "updatedAt": ts } }
                };
                self.monthly_top
                    .update_one(filter, update)
                    .await
                    .map_err(|e| AppError::internal(format!("monthly top update failed: {e}")))?;
            }
        }

        // Upsert player profiles.
        let writes = users.iter().map(|u| {
            let p = PlayerDoc::from_user(server, u, ts);
            self.players
                .replace_one(doc! { "server": server, "uid": u.uid }, p)
                .upsert(true)
        });
        for w in writes {
            w.await
                .map_err(|e| AppError::internal(format!("player upsert failed: {e}")))?;
        }

        Ok(n)
    }

    /// Full monthly top history for ``: points sorted by
    /// time plus the latest player profiles of the involved users.
    pub async fn monthly_top(
        &self,
        server: i64,
        monthly_id: i64,
        since: i64,
    ) -> AppResult<(Vec<TopPoint>, Vec<PlayerDoc>)> {
        let filter = if since > 0 {
            doc! { "server": server, "monthlyId": monthly_id, "bucket": { "$gte": since / 86_400_000 } }
        } else {
            doc! { "server": server, "monthlyId": monthly_id }
        };
        let cursor = self
            .monthly_top
            .find(filter)
            .await
            .map_err(|e| AppError::internal(format!("monthly top query failed: {e}")))?;
        let docs = cursor
            .try_collect::<Vec<mongodb::bson::Document>>()
            .await
            .map_err(|e| AppError::internal(format!("monthly top read failed: {e}")))?;

        let mut points: Vec<TopPoint> = Vec::new();
        let mut uids: Vec<i64> = Vec::new();
        for doc in docs {
            if let Ok(arr) = doc.get_array("points") {
                for item in arr {
                    if let Ok(p) = bson::from_bson::<TopPoint>(item.clone()) {
                        if since > 0 && p.time < since {
                            continue;
                        }
                        if !uids.contains(&p.uid) {
                            uids.push(p.uid);
                        }
                        points.push(p);
                    }
                }
            }
        }
        points.sort_by_key(|p| p.time);

        let players = if uids.is_empty() {
            Vec::new()
        } else {
            let cursor = self
                .players
                .find(doc! { "server": server, "uid": { "$in": &uids } })
                .await
                .map_err(|e| AppError::internal(format!("players query failed: {e}")))?;
            cursor
                .try_collect::<Vec<PlayerDoc>>()
                .await
                .map_err(|e| AppError::internal(format!("players read failed: {e}")))?
        };

        Ok((points, players))
    }

    /// Appends border cutoffs for `` from border users.
    pub async fn append_monthly_borders(
        &self,
        server: i64,
        monthly_id: i64,
        ts: i64,
        border_users: &[RankingUser],
    ) -> AppResult<usize> {
        let mut by_tier: BTreeMap<i64, Vec<CutoffPoint>> = BTreeMap::new();
        for u in border_users {
            if is_monthly_border_tier(u.tier) {
                by_tier.entry(u.tier).or_default().push(CutoffPoint {
                    time: ts,
                    ep: u.point,
                });
            }
        }
        let mut written = 0;
        for (tier, cutoffs) in by_tier {
            self.upsert_cutoffs(
                &self.monthly_border,
                doc! { "server": server, "monthlyId": monthly_id, "tier": tier },
                ts,
                &cutoffs,
            )
            .await?;
            written += cutoffs.len();
        }
        Ok(written)
    }

    /// Border cutoff history for ``.
    pub async fn monthly_border(
        &self,
        server: i64,
        monthly_id: i64,
        tier: i64,
    ) -> AppResult<Vec<CutoffPoint>> {
        self.query_cutoffs(
            &self.monthly_border,
            doc! { "server": server, "monthlyId": monthly_id, "tier": tier },
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Event snapshots
    // -----------------------------------------------------------------------

    /// Appends a top snapshot for server, eventId and mid where mid = 0 means
    /// the event-wide ranking, and upserts player profiles.
    pub async fn append_event_top(
        &self,
        server: i64,
        event_id: i64,
        mid: i64,
        ts: i64,
        users: &[RankingUser],
    ) -> AppResult<usize> {
        let bucket = utc_bucket(ts);
        let filter = doc! { "server": server, "eventId": event_id, "mid": mid, "bucket": bucket };
        let existing = self
            .event_top
            .find_one(filter.clone())
            .await
            .map_err(|e| AppError::internal(format!("event top query failed: {e}")))?;

        let points: Vec<TopPoint> = users
            .iter()
            .map(|u| TopPoint {
                time: ts,
                uid: u.uid,
                value: u.point,
            })
            .collect();
        let points_bson = serde_to_bson(&points)?;
        let n = points.len();

        match existing {
            None => {
                let doc = doc! {
                    "server": server, "eventId": event_id, "mid": mid, "bucket": bucket,
                    "points": points_bson,
                    "updatedAt": ts,
                };
                self.event_top
                    .insert_one(doc)
                    .await
                    .map_err(|e| AppError::internal(format!("event top insert failed: {e}")))?;
            }
            Some(doc) => {
                let last = doc.get_i64("updatedAt").unwrap_or(0);
                let update = if last < ts {
                    doc! {
                        "$set": { "updatedAt": ts },
                        "$push": { "points": { "$each": points_bson } },
                    }
                } else {
                    doc! { "$set": { "points": points_bson, "updatedAt": ts } }
                };
                self.event_top
                    .update_one(filter, update)
                    .await
                    .map_err(|e| AppError::internal(format!("event top update failed: {e}")))?;
            }
        }

        let writes = users.iter().map(|u| {
            let p = PlayerDoc::from_user(server, u, ts);
            self.players
                .replace_one(doc! { "server": server, "uid": u.uid }, p)
                .upsert(true)
        });
        for w in writes {
            w.await
                .map_err(|e| AppError::internal(format!("player upsert failed: {e}")))?;
        }

        Ok(n)
    }

    /// Full event top history for ``.
    pub async fn event_top(
        &self,
        server: i64,
        event_id: i64,
        mid: i64,
        since: i64,
    ) -> AppResult<(Vec<TopPoint>, Vec<PlayerDoc>)> {
        let filter = if since > 0 {
            doc! { "server": server, "eventId": event_id, "mid": mid, "bucket": { "$gte": since / 86_400_000 } }
        } else {
            doc! { "server": server, "eventId": event_id, "mid": mid }
        };
        let cursor = self
            .event_top
            .find(filter)
            .await
            .map_err(|e| AppError::internal(format!("event top query failed: {e}")))?;
        let docs = cursor
            .try_collect::<Vec<mongodb::bson::Document>>()
            .await
            .map_err(|e| AppError::internal(format!("event top read failed: {e}")))?;

        let mut points: Vec<TopPoint> = Vec::new();
        let mut uids: Vec<i64> = Vec::new();
        for doc in docs {
            if let Ok(arr) = doc.get_array("points") {
                for item in arr {
                    if let Ok(p) = bson::from_bson::<TopPoint>(item.clone()) {
                        if since > 0 && p.time < since {
                            continue;
                        }
                        if !uids.contains(&p.uid) {
                            uids.push(p.uid);
                        }
                        points.push(p);
                    }
                }
            }
        }
        points.sort_by_key(|p| p.time);

        let players = if uids.is_empty() {
            Vec::new()
        } else {
            let cursor = self
                .players
                .find(doc! { "server": server, "uid": { "$in": &uids } })
                .await
                .map_err(|e| AppError::internal(format!("players query failed: {e}")))?;
            cursor
                .try_collect::<Vec<PlayerDoc>>()
                .await
                .map_err(|e| AppError::internal(format!("players read failed: {e}")))?
        };

        Ok((points, players))
    }

    /// Appends border cutoffs for ``.
    pub async fn append_event_borders(
        &self,
        server: i64,
        event_id: i64,
        mid: i64,
        ts: i64,
        border_users: &[RankingUser],
        music: bool,
    ) -> AppResult<usize> {
        let mut by_tier: BTreeMap<i64, Vec<CutoffPoint>> = BTreeMap::new();
        for u in border_users {
            let valid = if music {
                is_music_border_tier(u.tier)
            } else {
                is_event_border_tier(u.tier)
            };
            if valid {
                by_tier.entry(u.tier).or_default().push(CutoffPoint {
                    time: ts,
                    ep: u.point,
                });
            }
        }
        let mut written = 0;
        for (tier, cutoffs) in by_tier {
            self.upsert_cutoffs(
                &self.event_border,
                doc! { "server": server, "eventId": event_id, "mid": mid, "tier": tier },
                ts,
                &cutoffs,
            )
            .await?;
            written += cutoffs.len();
        }
        Ok(written)
    }

    /// Border cutoff history for ``.
    pub async fn event_border(
        &self,
        server: i64,
        event_id: i64,
        mid: i64,
        tier: i64,
    ) -> AppResult<Vec<CutoffPoint>> {
        self.query_cutoffs(
            &self.event_border,
            doc! { "server": server, "eventId": event_id, "mid": mid, "tier": tier },
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Shared cutoff helpers
    // -----------------------------------------------------------------------

    /// Idempotent cutoff series upsert: appends when `ts` is newer, replaces
    /// when `ts` equals the stored `updatedAt`.
    async fn upsert_cutoffs(
        &self,
        coll: &Collection<mongodb::bson::Document>,
        filter: mongodb::bson::Document,
        ts: i64,
        cutoffs: &[CutoffPoint],
    ) -> AppResult<()> {
        let existing = coll
            .find_one(filter.clone())
            .await
            .map_err(|e| AppError::internal(format!("cutoff query failed: {e}")))?;
        let cutoffs_bson = serde_to_bson(cutoffs)?;

        match existing {
            None => {
                let mut doc = doc! { "cutoffs": cutoffs_bson, "updatedAt": ts };
                for (k, v) in filter.clone().into_iter() {
                    doc.insert(k, v);
                }
                coll.insert_one(doc)
                    .await
                    .map_err(|e| AppError::internal(format!("cutoff insert failed: {e}")))?;
            }
            Some(doc) => {
                let last = doc.get_i64("updatedAt").unwrap_or(0);
                let update = if last < ts {
                    doc! {
                        "$set": { "updatedAt": ts },
                        "$push": { "cutoffs": { "$each": cutoffs_bson } },
                    }
                } else {
                    doc! { "$set": { "cutoffs": cutoffs_bson, "updatedAt": ts } }
                };
                coll.update_one(filter, update)
                    .await
                    .map_err(|e| AppError::internal(format!("cutoff update failed: {e}")))?;
            }
        }
        Ok(())
    }

    async fn query_cutoffs(
        &self,
        coll: &Collection<mongodb::bson::Document>,
        filter: mongodb::bson::Document,
    ) -> AppResult<Vec<CutoffPoint>> {
        let doc = coll
            .find_one(filter)
            .await
            .map_err(|e| AppError::internal(format!("cutoff query failed: {e}")))?;
        let mut cutoffs: Vec<CutoffPoint> = Vec::new();
        if let Some(doc) = doc {
            if let Ok(arr) = doc.get_array("cutoffs") {
                for item in arr {
                    if let Ok(c) = bson::from_bson::<CutoffPoint>(item.clone()) {
                        cutoffs.push(c);
                    }
                }
            }
        }
        cutoffs.sort_by_key(|c| c.time);
        Ok(cutoffs)
    }
}

/// Per-UTC-day bucket index: `ts / 86_400_000`.
///
/// GarupaSpeedTracker buckets by 8 days, which is sized for a 5-minute poll
/// interval. At the default 1-minute interval a top-100 snapshot set grows to
/// ~52 MB per 8-day bucket — beyond MongoDB's 16 MB document limit — so we
/// bucket per day instead.
/// Buckets are internal sharding keys; API output is unaffected.
fn utc_bucket(ts: i64) -> i64 {
    ts.div_euclid(86_400_000)
}

fn serde_to_bson<T: serde::Serialize + ?Sized>(value: &T) -> AppResult<mongodb::bson::Bson> {
    bson::to_bson(value).map_err(|e| AppError::internal(format!("bson serialize failed: {e}")))
}
