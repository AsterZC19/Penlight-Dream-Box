//! Penlight Dream Box — Garupa event & monthly ranking API aggregator.
//!
//! Consumes the already-encapsulated Penlight-Dream-API over HTTP, persists
//! ranking snapshots to MongoDB, and serves a GarupaSpeedTracker-compatible
//! API.

mod api;
mod collector;
mod config;
mod error;
mod garupa;
mod storage;
mod upstream;

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tracing::{error, info};

use crate::api::AppState;
use crate::collector::Collector;
use crate::garupa::ProfileClient;
use crate::storage::Storage;
use crate::upstream::Upstream;

#[tokio::main]
async fn main() {
    let cfg = config::load();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // MongoDB.
    let storage = match Storage::connect(&cfg.mongodb_uri, &cfg.mongodb_db).await {
        Ok(s) => s,
        Err(e) => {
            error!("failed to connect to MongoDB at {}: {e}", cfg.mongodb_uri);
            std::process::exit(1);
        }
    };
    info!("connected to MongoDB database '{}'", cfg.mongodb_db);

    // Upstream Penlight-Dream-API client.
    let upstream = match Upstream::new(&cfg) {
        Ok(u) => u,
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        }
    };
    let profile_client = match ProfileClient::new(&cfg) {
        Ok(client) => client,
        Err(e) => {
            error!("{e}");
            std::process::exit(1);
        }
    };

    // Collector: runs forever in its own task.
    let collector = Collector::new(upstream.clone(), storage.clone(), cfg.clone());
    tokio::spawn(collector.run());

    // HTTP API.
    let state = AppState {
        storage,
        config: cfg.clone(),
        upstream,
        profile_client,
    };
    let app = api::build_router(state, &cfg);

    let addr: SocketAddr = format!("{}:{}", cfg.host, cfg.port)
        .parse()
        .expect("invalid HOST/PORT");
    let listener = TcpListener::bind(addr)
        .await
        .expect("failed to bind listener");
    info!(
        "Penlight-Dream-Box listening on http://{addr} with API prefix '{}'",
        cfg.api_prefix
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutdown signal received, exiting");
}
