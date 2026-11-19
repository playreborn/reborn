//! `reborn-cli`: command-line frontend for Reborn.
//!
//! In M0 the CLI talks to Steam directly so the protocol work can be tested
//! without a daemon. From M2 on it becomes a thin client of `rebornd`.
#![forbid(unsafe_code)]

use std::io::{BufRead, Write};

use anyhow::{Context, Result, anyhow, bail};
use clap::{ArgAction, Parser, Subcommand};
use reborn_cm::confirmation::{
    AuthConfirmationHandler, ConsoleAuthConfirmationHandler, DeviceConfirmationHandler,
};
use reborn_cm::{PUBLIC_BRANCH, Platform, Secret, Session};
use reborn_content::DepotKey;
use reborn_content::cdn::{CdnClient, CdnHost};
use reborn_content::chunk::process_chunk;
use reborn_content::manifest::DepotManifest;
use reborn_content::path::install_path;
use reborn_platform::secrets::{self, SecretKind};
use reborn_platform::settings;
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
    /// Sign in with account name, password and Steam Guard.
    ///
    /// The password is only sent to Steam and never stored. Steam returns a
    /// refresh token, which is kept in the system keyring so later commands
    /// work without the password.
    Login {
        /// Steam account name (asked for if omitted).
        #[arg(long)]
        account: Option<String>,
    },
    /// List the games owned by the signed-in account.
    Library {
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
        /// Also list DLC, tools and other non-game entries.
        #[arg(long)]
        all: bool,
    },
    /// Show the depots of an app and which ones fit this computer.
    Depots {
        /// App ID, e.g. 480 for Spacewar.
        app_id: u32,
        /// Branch to look at.
        #[arg(long, default_value = PUBLIC_BRANCH)]
        branch: String,
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
    /// Download and check a depot manifest and its first chunk (developer
    /// check for the content engine; installs nothing).
    Manifest {
        /// App ID, e.g. 480 for Spacewar.
        app_id: u32,
        /// Depot ID (defaults to the first depot that fits this computer).
        depot_id: Option<u32>,
        /// Branch to use.
        #[arg(long, default_value = PUBLIC_BRANCH)]
        branch: String,
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
    /// Remove the stored login of an account from this computer.
    Logout {
        /// Account to sign out (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::Login { account } => login(account).await,
        Command::Library { account, all } => library(account, all).await,
        Command::Depots {
            app_id,
            branch,
            account,
        } => depots(account, app_id, &branch).await,
        Command::Manifest {
            app_id,
            depot_id,
            branch,
            account,
        } => manifest(account, app_id, depot_id, &branch).await,
        Command::Logout { account } => logout(account),
    }
}

async fn login(account: Option<String>) -> Result<()> {
    let account = match account {
        Some(account) => account,
        None => prompt_line("Steam account name: ")?,
    };
    if account.is_empty() {
        bail!("no account name given");
    }
    let password = Secret::new(rpassword::prompt_password("Password: ")?);
    let guard_data = secrets::load(SecretKind::GuardData, &account)?;

    eprintln!(
        "Signing in. Confirm in the Steam mobile app, or enter the Steam Guard code when asked."
    );
    let confirmation = ConsoleAuthConfirmationHandler::default().or(DeviceConfirmationHandler);
    let login =
        reborn_cm::login_with_password(&account, &password, guard_data.as_ref(), confirmation)
            .await?;

    secrets::store(SecretKind::RefreshToken, &account, &login.refresh_token)?;
    if let Some(guard_data) = &login.new_guard_data {
        secrets::store(SecretKind::GuardData, &account, guard_data)?;
    }
    settings::set_default_account(&account)?;

    println!(
        "Signed in as {account} (SteamID {}). The login is stored in the system keyring.",
        login.session.steam_id()
    );
    Ok(())
}

/// Signs in with the stored refresh token of the account.
async fn session(account: Option<String>) -> Result<Session> {
    let account = resolve_account(account)?;
    let token = secrets::load(SecretKind::RefreshToken, &account)?
        .ok_or_else(|| anyhow!("no stored login for `{account}`, run `reborn-cli login` first"))?;
    reborn_cm::login_with_token(&account, &token)
        .await
        .context("signing in with the stored token failed, run `reborn-cli login` again")
}

async fn library(account: Option<String>, all: bool) -> Result<()> {
    let session = session(account).await?;

    let mut apps = session.owned_apps().await?;
    let total = apps.len();
    if !all {
        apps.retain(|app| app.is_listed_by_default());
    }
    apps.sort_by_cached_key(|app| app.name.to_lowercase());

    let mut out = std::io::stdout().lock();
    for app in &apps {
        let hours = f64::from(app.playtime_minutes) / 60.0;
        let name = if app.name.is_empty() {
            "(unknown)"
        } else {
            &app.name
        };
        writeln!(
            out,
            "{:>8}  {:<11}  {hours:>8.1} h  {name}",
            app.app_id, app.app_type
        )?;
    }
    writeln!(
        out,
        "{} shown, {total} apps in the library{}",
        apps.len(),
        if all {
            ""
        } else {
            " (use --all for DLC, tools and more)"
        }
    )?;
    Ok(())
}

async fn depots(account: Option<String>, app_id: u32, branch: &str) -> Result<()> {
    let session = session(account).await?;
    let app = session.app_details(app_id, branch).await?;
    let platform = Platform::current();

    println!(
        "{} (app {app_id}), install folder `{}`, build {}",
        app.name.as_deref().unwrap_or("(unknown)"),
        app.install_dir.as_deref().unwrap_or("?"),
        app.build_id
            .map_or_else(|| "?".to_owned(), |build| build.to_string()),
    );
    println!(
        "This computer: {} {}-bit, {}. `*` = would be installed.",
        platform.os, platform.arch, platform.language
    );
    for depot in &app.depots {
        let selected = depot.applies_to(&platform) && depot.manifest_id.is_some();
        let source = match (depot.manifest_id, depot.depot_from_app) {
            (Some(manifest), _) => format!("manifest {manifest}"),
            (None, Some(other)) => format!("shared from app {other}"),
            (None, None) => format!("no manifest on `{branch}`"),
        };
        let os = if depot.os_list.is_empty() {
            "all".to_owned()
        } else {
            depot.os_list.join(",")
        };
        println!(
            "{} {:>8}  {:<14} {:>10}  {:<34}  {}",
            if selected { '*' } else { ' ' },
            depot.depot_id,
            os,
            depot.size.map(human_size).unwrap_or_default(),
            source,
            depot.name.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}

async fn manifest(
    account: Option<String>,
    app_id: u32,
    depot_id: Option<u32>,
    branch: &str,
) -> Result<()> {
    let session = session(account).await?;
    let app = session.app_details(app_id, branch).await?;
    let platform = Platform::current();
    let depot = match depot_id {
        Some(id) => app
            .depots
            .iter()
            .find(|depot| depot.depot_id == id)
            .ok_or_else(|| anyhow!("app {app_id} has no depot {id}"))?,
        None => app
            .depots_for(&platform)
            .next()
            .ok_or_else(|| anyhow!("app {app_id} has no depot for this computer"))?,
    };
    let manifest_id = depot.manifest_id.ok_or_else(|| {
        anyhow!(
            "depot {} has no manifest on branch `{branch}`{}",
            depot.depot_id,
            depot
                .depot_from_app
                .map(|other| format!(" (it comes from app {other})"))
                .unwrap_or_default()
        )
    })?;

    eprintln!(
        "Depot {}, manifest {manifest_id}: asking Steam for the key and content servers",
        depot.depot_id
    );
    let key = session.depot_key(app_id, depot.depot_id).await?;
    let key = DepotKey::from_slice(key.as_bytes())
        .ok_or_else(|| anyhow!("Steam sent a depot key of unexpected length"))?;
    let servers: Vec<CdnHost> = session
        .content_servers()
        .await?
        .into_iter()
        .filter(|server| server.serves_app(app_id))
        .map(|server| CdnHost {
            host: server.host,
            https: server.https,
        })
        .collect();
    if servers.is_empty() {
        bail!("Steam returned no usable content servers");
    }

    let cdn = CdnClient::new()?;
    let mut downloaded = None;
    for server in servers.iter().take(5) {
        // Request codes are short-lived, so fetch a fresh one per attempt.
        let code = session
            .manifest_request_code(app_id, depot.depot_id, manifest_id, branch)
            .await?;
        match cdn
            .manifest(server, depot.depot_id, manifest_id, code)
            .await
        {
            Ok(bytes) => {
                downloaded = Some((server, bytes));
                break;
            }
            Err(error) => eprintln!("  {error}, trying the next server"),
        }
    }
    let (server, raw) =
        downloaded.ok_or_else(|| anyhow!("no content server delivered the manifest"))?;
    eprintln!(
        "Manifest downloaded from {} ({}, {})",
        server.host,
        if server.https { "HTTPS" } else { "HTTP" },
        human_size(raw.len() as u64)
    );

    let mut manifest = DepotManifest::parse(&raw)?;
    let was_encrypted = manifest.filenames_encrypted;
    manifest.decrypt_filenames(&key)?;
    let chunk_count: usize = manifest.files.iter().map(|file| file.chunks.len()).sum();
    let unsafe_names = manifest
        .files
        .iter()
        .filter(|file| install_path(&file.name).is_err())
        .count();
    println!(
        "Depot {} manifest {}: {} entries, {} chunks, {} on disk, file names {}encrypted, {} unsafe names",
        manifest.depot_id,
        manifest.manifest_id,
        manifest.files.len(),
        chunk_count,
        human_size(manifest.content_size()),
        if was_encrypted { "" } else { "not " },
        unsafe_names,
    );
    for file in manifest.files.iter().take(15) {
        let kind = if file.is_directory() {
            "dir "
        } else if file.is_symlink() {
            "link"
        } else {
            "file"
        };
        println!("  {kind} {:>10}  {}", human_size(file.size), file.name);
    }
    if manifest.files.len() > 15 {
        println!("  ... {} more", manifest.files.len() - 15);
    }

    if let Some((file, chunk)) = manifest
        .files
        .iter()
        .find_map(|file| file.chunks.first().map(|chunk| (file, chunk)))
    {
        let encrypted = cdn.chunk(server, depot.depot_id, &chunk.id).await?;
        let data = process_chunk(&encrypted, &key, chunk.expectation())?;
        println!(
            "First chunk of `{}`: {} downloaded, {} after decryption and decompression, checksum ok",
            file.name,
            human_size(encrypted.len() as u64),
            human_size(data.len() as u64),
        );
    }
    Ok(())
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn logout(account: Option<String>) -> Result<()> {
    let account = resolve_account(account)?;
    let removed_token = secrets::delete(SecretKind::RefreshToken, &account)?;
    let removed_guard = secrets::delete(SecretKind::GuardData, &account)?;
    if settings::default_account()?.as_deref() == Some(account.as_str()) {
        settings::clear_default_account()?;
    }
    if removed_token || removed_guard {
        println!("Removed the stored login of {account} from this computer.");
    } else {
        println!("No stored login for {account} found.");
    }
    Ok(())
}

fn resolve_account(account: Option<String>) -> Result<String> {
    match account {
        Some(account) => Ok(account),
        None => settings::default_account()?.ok_or_else(|| {
            anyhow!(
                "no account given and no default account, use `--account` or `reborn-cli login`"
            )
        }),
    }
}

fn prompt_line(prompt: &str) -> Result<String> {
    let mut stderr = std::io::stderr();
    stderr.write_all(prompt.as_bytes())?;
    stderr.flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

/// Logs go to stderr so stdout stays clean for machine-readable output.
fn init_tracing(verbose: u8) {
    let default_level = match verbose {
        // steam-vent logs rejected logins as low-level read errors before the
        // real error (e.g. InvalidPassword) is returned; hide that noise.
        0 => "warn,steam_vent::connection::filter=off",
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
