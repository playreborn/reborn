//! `rebornd`: the Reborn background daemon.
//!
//! It will own the Steam connection, the download queue and the local IPC
//! server for frontends (and later for games through the shim). For now it
//! only starts, logs and waits for Ctrl+C.
#![forbid(unsafe_code)]

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let from_env = EnvFilter::try_from_default_env();
    let filter = from_env.unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "rebornd starting");
    tracing::warn!("skeleton build: no Steam connection and no IPC yet (milestones M0-M2)");

    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    Ok(())
}
