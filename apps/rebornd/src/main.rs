//! `rebornd`: the Reborn background daemon.
//!
//! Owns the Steam session and the download queue and serves frontends over
//! a per-user local socket (ADR 0002, ADR 0008). Jobs run one after another;
//! progress is pushed to every connected frontend. Stopping the daemon
//! interrupts a running job; the next install of that app resumes it.
#![forbid(unsafe_code)]

mod daemon;
mod server;

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use interprocess_free::is_addr_in_use;
use tracing_subscriber::EnvFilter;

use crate::daemon::Daemon;

#[tokio::main]
async fn main() -> Result<()> {
    let from_env = EnvFilter::try_from_default_env();
    let filter =
        from_env.unwrap_or_else(|_| EnvFilter::new("info,steam_vent::connection::filter=off"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let listener = match reborn_ipc::socket::listen() {
        Ok(listener) => listener,
        Err(error) if is_addr_in_use(&error) => {
            bail!("rebornd is already running for this user");
        }
        Err(error) => return Err(error).context("cannot open the local socket"),
    };
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "rebornd listening");

    let daemon = Arc::new(Daemon::new());
    tokio::spawn(daemon::run_jobs(Arc::clone(&daemon)));

    tokio::select! {
        () = server::accept_loop(listener, Arc::clone(&daemon)) => {}
        result = tokio::signal::ctrl_c() => {
            result.context("waiting for Ctrl+C")?;
            tracing::info!("Ctrl+C received");
        }
        () = daemon.shutdown_requested() => {
            tracing::info!("shutdown requested by a frontend");
        }
    }
    daemon.announce_shutdown();
    // Give writers a moment to deliver the shutdown event.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    tracing::info!("rebornd stopped");
    Ok(())
}

/// Small helper module so `main` stays readable.
mod interprocess_free {
    /// Whether opening the socket failed because another daemon owns it.
    /// Windows reports an existing named pipe as "access denied" (the first
    /// pipe instance is exclusive), Unix as "address in use".
    pub fn is_addr_in_use(error: &std::io::Error) -> bool {
        error.kind() == std::io::ErrorKind::AddrInUse
            || (cfg!(windows) && error.kind() == std::io::ErrorKind::PermissionDenied)
    }
}
