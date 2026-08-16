//! Periodic collector: pulls monthly-ranking and event snapshots from
//! Penlight-Dream-API over HTTP and persists them to MongoDB.
//!
//! - Master lists refresh every `master_interval_secs`.
//! - Active rankings are polled every `poll_interval_secs`.
//! - After a ranking's `endAt`, we keep polling for `post_end_window_secs`
//!   with the snapshot timestamp clamped to `endAt`.
//! - Bootstrap collects all currently active rankings once at startup.

use std::time::{Duration, Instant};

use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use crate::config::Config;
use crate::error::AppResult;
use crate::storage::Storage;
use crate::upstream::Upstream;

/// How long to wait for the Dream-API to become healthy at startup.
const STARTUP_HEALTH_WAIT: Duration = Duration::from_secs(60);
const HEALTH_RETRY: Duration = Duration::from_secs(5);

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Clamps a snapshot timestamp to `end_at` when the ranking already ended.
fn snapshot_ts(now: i64, end_at: i64) -> i64 {
    if end_at > 0 && now > end_at {
        end_at
    } else {
        now
    }
}

#[derive(Clone)]
pub struct Collector {
    pub upstream: Upstream,
    pub storage: Storage,
    pub cfg: Config,
    last_master_refresh: Option<Instant>,
}

impl Collector {
    pub fn new(upstream: Upstream, storage: Storage, cfg: Config) -> Self {
        Self {
            upstream,
            storage,
            cfg,
            last_master_refresh: None,
        }
    }

    /// Runs the collector forever: wait for upstream health, refresh masters,
    /// bootstrap active rankings, then poll on a fixed interval.
    pub async fn run(mut self) {
        // Wait for Penlight-Dream-API to become reachable.
        let mut healthy = false;
        let deadline = Instant::now() + STARTUP_HEALTH_WAIT;
        while Instant::now() < deadline {
            if self.upstream.ping().await {
                healthy = true;
                break;
            }
            tokio::time::sleep(HEALTH_RETRY).await;
        }
        if healthy {
            info!("Dream-API is healthy, starting collector");
        } else {
            warn!("Dream-API not reachable at startup; collector will keep retrying each cycle");
        }

        // Initial master refresh + bootstrap of currently active rankings.
        if let Err(e) = self.refresh_masters_if_stale(true).await {
            warn!("initial master refresh failed: {e}");
        }
        if let Err(e) = self.tick().await {
            warn!("bootstrap collection failed: {e}");
        }

        let mut interval =
            tokio::time::interval(Duration::from_secs(self.cfg.poll_interval_secs.max(30)));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(e) = self.tick().await {
                warn!("collection cycle failed: {e}");
            }
        }
    }

    /// Refreshes monthly-info and event master lists from Dream-API when the
    /// configured interval has elapsed.
    async fn refresh_masters_if_stale(&mut self, force: bool) -> AppResult<()> {
        let due = force
            || self
                .last_master_refresh
                .map(|t| t.elapsed() >= Duration::from_secs(self.cfg.master_interval_secs))
                .unwrap_or(true);
        if !due {
            return Ok(());
        }
        let infos = self.upstream.monthly_master().await?;
        let n_info = self.storage.upsert_monthly_infos(&infos).await?;
        let events = self.upstream.event_master().await?;
        let n_events = self.storage.upsert_events(&events).await?;
        self.last_master_refresh = Some(Instant::now());
        info!("masters refreshed: {n_info} monthly infos, {n_events} events");
        Ok(())
    }

    /// One collection cycle: refresh masters if stale, then collect every
    /// active monthly ranking and event.
    async fn tick(&mut self) -> AppResult<()> {
        if let Err(e) = self.refresh_masters_if_stale(false).await {
            warn!("master refresh failed, continuing collection: {e}");
        }
        let now = now_ms();

        let actives = self
            .storage
            .active_monthlies(now, self.cfg.post_end_window_secs as i64)
            .await?;
        for (monthly_id, end_at) in actives {
            if let Err(e) = self.collect_monthly(monthly_id, end_at).await {
                warn!("monthly {monthly_id} collection failed: {e}");
            }
        }

        let events = self
            .storage
            .active_events(now, self.cfg.post_end_window_secs as i64)
            .await?;
        for (event_id, end_at) in events {
            if let Err(e) = self.collect_event(event_id, end_at).await {
                warn!("event {event_id} collection failed: {e}");
            }
        }

        Ok(())
    }

    /// Collects one monthly ranking: full report → top points + border cutoffs.
    async fn collect_monthly(&self, monthly_id: i64, end_at: i64) -> AppResult<()> {
        let ts = snapshot_ts(now_ms(), end_at);
        let full = self.upstream.monthly_full(monthly_id).await?;
        let n_top = self
            .storage
            .append_monthly_top(0, monthly_id, ts, &full.monthly_ranking_point_top_users)
            .await?;
        let n_border = self
            .storage
            .append_monthly_borders(0, monthly_id, ts, &full.monthly_ranking_point_border_users)
            .await?;
        info!("monthly {monthly_id}: +{n_top} top points, +{n_border} cutoff points @ {ts}");
        Ok(())
    }

    /// Collects one event: event-wide ranking mid=0 plus per-song rankings.
    async fn collect_event(&self, event_id: i64, end_at: i64) -> AppResult<()> {
        let ts = snapshot_ts(now_ms(), end_at);
        let report = self.upstream.event_ranking(event_id, None).await?;
        let n_top = self
            .storage
            .append_event_top(0, event_id, 0, ts, &report.event_point_top_users)
            .await?;
        let n_border = self
            .storage
            .append_event_borders(0, event_id, 0, ts, &report.event_point_border_users, false)
            .await?;
        info!("event {event_id} ({event_type}): +{n_top} top points, +{n_border} cutoff points @ {ts}", event_type = report.event_type);

        for music in &report.music_rankings {
            let _ = self
                .storage
                .append_event_top(0, event_id, music.music_id, ts, &music.score_top_users)
                .await?;
            let _ = self
                .storage
                .append_event_borders(
                    0,
                    event_id,
                    music.music_id,
                    ts,
                    &music.score_border_users,
                    true,
                )
                .await?;
        }
        Ok(())
    }
}
