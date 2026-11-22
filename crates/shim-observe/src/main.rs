//! `reborn-observe`: arms and disarms an observation session (ADR 0039).
//! Only ever run by the user.
#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Observation sessions for the steamclient replacement (ADR 0038, ADR 0039).
#[derive(Debug, Parser)]
#[command(name = "reborn-observe", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show SteamClientDll64 and whether a session is armed.
    Status,
    /// Save SteamClientDll64, copy the proxy to Reborn's folder and point
    /// SteamClientDll64 at it. Start the official client first.
    Arm {
        /// The built proxy DLL.
        #[arg(long)]
        proxy: PathBuf,
        /// Hand out no stand-in objects: only read, as in step 3a
        /// (ADR 0040). Use it if the game misbehaves with stand-ins.
        #[arg(long)]
        read_only: bool,
        /// The app ID of the game about to run, for shadow mode (ADR 0046):
        /// the proxy compares the official client's answers with Reborn's.
        /// Also registers a read-only shadow session for that app with a
        /// running, signed-in `rebornd` (ADR 0055), from which the proxy
        /// gets Reborn's ownership answer.
        #[arg(long)]
        app: Option<u32>,
        /// Experiment 6 of ADR 0048 only: write this process ID into
        /// `ActiveProcess\pid` for the session (saved, and restored on
        /// disarm if still unchanged).
        #[arg(long)]
        pid: Option<u32>,
    },
    /// Put the saved SteamClientDll64 back (only if it still points to the
    /// proxy). Safe to run any number of times.
    Disarm,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli.command)
}

#[cfg(windows)]
fn run(command: Command) -> Result<()> {
    use reborn_shim_observe::registry::{PidStore, RegistryStore};
    use reborn_shim_observe::session::{Disarmed, Session};

    let session = Session::new(reborn_platform::settings::data_dir()?.join("shim-observe"));
    let store = RegistryStore::steam_client_dll64();
    match command {
        Command::Status => {
            let status = session.status(&store)?;
            println!(
                "SteamClientDll64: {}",
                status.current.as_deref().unwrap_or("(not set)")
            );
            match status.saved {
                Some(saved) => println!("Armed; value before the session: {saved}"),
                None => println!("Not armed."),
            }
            if status.points_to_proxy {
                println!("The value points to the proxy.");
            }
        }
        Command::Arm {
            proxy,
            read_only,
            app,
            pid,
        } => {
            let target = session.arm(&store, &proxy, read_only)?;
            session.set_app_id(app)?;
            if let Some(pid) = pid {
                session.replace_pid(&PidStore::steam_pid(), pid)?;
                println!("Experiment 6: pid is now {pid}.");
            }
            if let Some(app) = app {
                println!("Shadow mode: app ID {app}.");
                println!("{}", register_shadow(app));
            }
            if read_only {
                println!("Read only: the proxy hands out no stand-in objects.");
            }
            println!(
                "Armed: SteamClientDll64 now points to {}.",
                target.display()
            );
            println!("Run `reborn-observe disarm` when the game has exited.");
        }
        Command::Disarm => {
            if let Some(note) = cancel_shadow() {
                println!("{note}");
            }
            if let Some(original) = session.restore_pid(&PidStore::steam_pid())? {
                println!("Restored pid to {original}.");
            }
            match session.disarm(&store)? {
                Disarmed::NothingArmed => println!("Nothing was armed."),
                Disarmed::Restored(value) => println!("Restored SteamClientDll64 to {value}."),
                Disarmed::LeftAlone { current, saved } => println!(
                    "SteamClientDll64 no longer pointed to the proxy ({}); left it alone. \
                 The value before the session was {saved}.",
                    current.as_deref().unwrap_or("not set")
                ),
            }
        }
    }
    Ok(())
}

/// Registers the shadow session for `app` with `rebornd` (ADR 0055); what
/// to tell the user. A failure leaves the session armed: the proxy then
/// logs that it has no Reborn answer for ownership and cloud reads.
#[cfg(windows)]
fn register_shadow(app: u32) -> String {
    use reborn_shim_observe::daemon::{connect, reason_name};
    use reborn_shim_protocol::UnavailableReason;
    match connect()
        .map_err(Into::into)
        .and_then(|mut client| client.register_shadow(app))
    {
        Ok(Ok(())) => format!("rebornd: shadow session registered for app {app}."),
        Ok(Err(reason)) => {
            let advice = match reason {
                UnavailableReason::NotSignedIn => {
                    " Sign in to rebornd (for example `reborn-cli library`), then disarm and arm again."
                }
                UnavailableReason::Offline => {
                    " rebornd is reconnecting, signing in or could not load the licence list; \
                     wait a moment, then disarm and arm again."
                }
                UnavailableReason::NotAllowed
                | UnavailableReason::TooLarge
                | UnavailableReason::NotReceived => "",
            };
            format!(
                "rebornd refused the shadow session ({}); ownership and cloud reads are not compared.{advice}",
                reason_name(reason)
            )
        }
        Err(error) => {
            format!("No shadow session ({error}); ownership and cloud reads are not compared.")
        }
    }
}

/// Drops a shadow registration nobody claimed; a note if it was dropped.
/// Without a reachable `rebornd` there is nothing to drop.
#[cfg(windows)]
fn cancel_shadow() -> Option<String> {
    let mut client = reborn_shim_observe::daemon::connect().ok()?;
    matches!(client.cancel_shadow(), Ok(Ok(())))
        .then(|| "rebornd: open shadow registration dropped.".to_owned())
}

#[cfg(not(windows))]
fn run(_command: Command) -> Result<()> {
    anyhow::bail!("reborn-observe works on Windows only for now (ADR 0039)")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
