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
        #[arg(long)]
        app: Option<u32>,
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
    use reborn_shim_observe::registry::RegistryStore;
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
        } => {
            let target = session.arm(&store, &proxy, read_only)?;
            session.set_app_id(app)?;
            if let Some(app) = app {
                println!("Shadow mode: app ID {app}.");
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
        Command::Disarm => match session.disarm(&store)? {
            Disarmed::NothingArmed => println!("Nothing was armed."),
            Disarmed::Restored(value) => println!("Restored SteamClientDll64 to {value}."),
            Disarmed::LeftAlone { current, saved } => println!(
                "SteamClientDll64 no longer pointed to the proxy ({}); left it alone. \
                 The value before the session was {saved}.",
                current.as_deref().unwrap_or("not set")
            ),
        },
    }
    Ok(())
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
