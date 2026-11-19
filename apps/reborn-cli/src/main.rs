//! `reborn-cli`: command-line frontend for Reborn.
//!
//! In M0 the CLI talks to Steam directly so the protocol work can be tested
//! without a daemon. From M2 on it becomes a thin client of `rebornd`.
#![forbid(unsafe_code)]

use anyhow::{Result, bail};
use clap::{ArgAction, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

/// Reborn: a native, open-source client for your Steam library.
#[derive(Debug, Parser)]
#[command(name = "reborn-cli", version, about)]
struct Cli {
    /// Increase log verbosity (-v info, -vv debug, -vvv trace).
    #[arg(short, long, action = ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Sign in to a Steam account.
    Login {
        /// Sign in by scanning a QR code with the Steam mobile app.
        #[arg(long)]
        qr: bool,
    },
    /// List the games owned by the signed-in account.
    Library,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::Login { qr } => {
            tracing::debug!(qr, "login requested");
            bail!("`login` is not implemented yet (milestone M0)")
        }
        Command::Library => bail!("`library` is not implemented yet (milestone M0)"),
    }
}

/// Logs go to stderr so stdout stays clean for machine-readable output.
fn init_tracing(verbose: u8) {
    let default_level = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let from_env = EnvFilter::try_from_default_env();
    let filter = from_env.unwrap_or_else(|_| EnvFilter::new(default_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
