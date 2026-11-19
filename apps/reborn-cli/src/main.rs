//! `reborn-cli`: command-line frontend for Reborn.
//!
//! In M0 the CLI talks to Steam directly so the protocol work can be tested
//! without a daemon. From M2 on it becomes a thin client of `rebornd`.
#![forbid(unsafe_code)]

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};
use clap::{ArgAction, Parser, Subcommand};
use reborn_cm::confirmation::{
    AuthConfirmationHandler, ConsoleAuthConfirmationHandler, DeviceConfirmationHandler,
};
use reborn_cm::{PUBLIC_BRANCH, Platform, Secret, Session};
use reborn_content::DepotKey;
use reborn_content::cdn::{CdnClient, CdnHost};
use reborn_content::chunk::process_chunk;
use reborn_content::install::{CdnSource, DepotDownload, InstallOptions, InstallTarget};
use reborn_content::manifest::DepotManifest;
use reborn_content::path::install_path;
use reborn_formats::{InstalledDepot, NewAppManifest};
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
    /// Download and install an app.
    ///
    /// Installs into Reborn's own library unless `--library` is given. The
    /// result has the Steam library layout (`steamapps/common/<folder>` and
    /// `steamapps/appmanifest_<app>.acf`).
    Install {
        /// App ID, e.g. 480 for Spacewar.
        app_id: u32,
        /// Library folder (the folder that contains `steamapps`).
        #[arg(long)]
        library: Option<PathBuf>,
        /// Branch to install.
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
        Command::Install {
            app_id,
            library,
            branch,
            account,
        } => install(account, app_id, library, &branch).await,
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
    let servers = cdn_hosts(&session, app_id).await?;
    let cdn = CdnClient::new()?;
    let FetchedDepot {
        key,
        manifest,
        server,
        was_encrypted,
    } = fetch_depot(
        &session,
        &cdn,
        &servers,
        app_id,
        depot.depot_id,
        manifest_id,
        branch,
    )
    .await?;
    eprintln!(
        "Manifest downloaded from {} ({})",
        server.host,
        if server.https { "HTTPS" } else { "HTTP" },
    );
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
        let encrypted = cdn.chunk(&server, depot.depot_id, &chunk.id).await?;
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

async fn install(
    account: Option<String>,
    app_id: u32,
    library: Option<PathBuf>,
    branch: &str,
) -> Result<()> {
    let session = session(account).await?;
    let app = session.app_details(app_id, branch).await?;
    let name = app.name.clone().unwrap_or_else(|| format!("App {app_id}"));
    let install_dir_name = app
        .install_dir
        .clone()
        .ok_or_else(|| anyhow!("app {app_id} has no install folder in its app info"))?;
    // The folder name comes from Steam: treat it like any other untrusted name.
    let install_dir_relative = install_path(&install_dir_name)?;
    if install_dir_relative.components().count() != 1 {
        bail!("unexpected install folder `{install_dir_name}`");
    }

    let platform = Platform::current();
    let depots: Vec<_> = app.depots_for(&platform).collect();
    if depots.is_empty() {
        bail!(
            "{name} has no depots for {} {}-bit on branch `{branch}`",
            platform.os,
            platform.arch
        );
    }

    let library = match library {
        Some(path) => path,
        None => settings::default_library()?,
    };
    // Secondary Steam libraries contain `libraryfolder.vdf`, the main one
    // `steamapps/libraryfolders.vdf`.
    let is_steam_library = [
        library.join("libraryfolder.vdf"),
        library.join("steamapps").join("libraryfolder.vdf"),
        library.join("steamapps").join("libraryfolders.vdf"),
    ]
    .iter()
    .any(|marker| marker.exists());
    if is_steam_library {
        eprintln!(
            "Note: {} is a Steam library. The Steam client must be closed while Reborn writes to it.",
            library.display()
        );
    }
    let steamapps = library.join("steamapps");
    let target = InstallTarget {
        install_dir: steamapps.join("common").join(&install_dir_relative),
        staging_dir: steamapps.join("downloading").join(app_id.to_string()),
    };

    eprintln!(
        "Installing {name} (app {app_id}, build {}) into {}",
        app.build_id
            .map_or_else(|| "?".to_owned(), |build| build.to_string()),
        target.install_dir.display()
    );
    let servers = cdn_hosts(&session, app_id).await?;
    let cdn = CdnClient::new()?;

    let mut downloads = Vec::new();
    let mut installed_depots = Vec::new();
    for depot in depots {
        let Some(manifest_id) = depot.manifest_id else {
            continue;
        };
        let fetched = fetch_depot(
            &session,
            &cdn,
            &servers,
            app_id,
            depot.depot_id,
            manifest_id,
            branch,
        )
        .await
        .with_context(|| format!("depot {}", depot.depot_id))?;
        eprintln!(
            "  depot {}: {} files, {}",
            depot.depot_id,
            fetched.manifest.files.len(),
            human_size(fetched.manifest.content_size())
        );
        installed_depots.push(InstalledDepot {
            depot_id: depot.depot_id,
            manifest_id,
            size: fetched.manifest.content_size(),
        });
        downloads.push(DepotDownload {
            depot_id: depot.depot_id,
            key: fetched.key,
            manifest: fetched.manifest,
        });
    }

    let source = CdnSource {
        cdn: &cdn,
        servers: &servers,
    };
    let started = std::time::Instant::now();
    let report = reborn_content::install::install(
        &downloads,
        &target,
        &source,
        InstallOptions::default(),
        |progress| {
            let percent = if progress.bytes_total == 0 {
                100.0
            } else {
                progress.bytes_done as f64 * 100.0 / progress.bytes_total as f64
            };
            eprint!(
                "\r  {percent:5.1} %  {} / {}  ({} of {} chunks)   ",
                human_size(progress.bytes_done),
                human_size(progress.bytes_total),
                progress.chunks_done,
                progress.chunks_total
            );
        },
    )
    .await?;
    eprintln!();

    let manifest = NewAppManifest {
        app_id,
        name: name.clone(),
        install_dir: install_dir_name,
        build_id: app.build_id.map(u64::from).unwrap_or(0),
        last_owner: session.steam_id(),
        size_on_disk: installed_depots.iter().map(|depot| depot.size).sum(),
        download_size: report.bytes_downloaded,
        last_updated: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|time| time.as_secs())
            .unwrap_or(0),
        installed_depots,
    };
    let acf_path = steamapps.join(format!("appmanifest_{app_id}.acf"));
    let temporary = steamapps.join(format!("appmanifest_{app_id}.acf.tmp"));
    std::fs::write(&temporary, manifest.to_text())
        .with_context(|| format!("writing {}", temporary.display()))?;
    std::fs::rename(&temporary, &acf_path)
        .with_context(|| format!("writing {}", acf_path.display()))?;

    println!(
        "Installed {name}: {} files, {} written, {} downloaded in {:.1} s",
        report.files,
        human_size(report.bytes_written),
        human_size(report.bytes_downloaded),
        started.elapsed().as_secs_f64()
    );
    if report.symlinks_skipped > 0 {
        println!(
            "  {} symlinks were skipped (not supported here)",
            report.symlinks_skipped
        );
    }
    println!("  Game folder: {}", target.install_dir.display());
    println!("  Steam manifest: {}", acf_path.display());
    Ok(())
}

/// Content servers that may serve the app, best first.
async fn cdn_hosts(session: &Session, app_id: u32) -> Result<Vec<CdnHost>> {
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
    Ok(servers)
}

/// A depot's key and its manifest with decrypted names.
struct FetchedDepot {
    key: DepotKey,
    manifest: DepotManifest,
    /// The server that delivered the manifest.
    server: CdnHost,
    /// Whether the names were encrypted on the CDN.
    was_encrypted: bool,
}

async fn fetch_depot(
    session: &Session,
    cdn: &CdnClient,
    servers: &[CdnHost],
    app_id: u32,
    depot_id: u32,
    manifest_id: u64,
    branch: &str,
) -> Result<FetchedDepot> {
    let key = session.depot_key(app_id, depot_id).await?;
    let key = DepotKey::from_slice(key.as_bytes())
        .ok_or_else(|| anyhow!("Steam sent a depot key of unexpected length"))?;

    let mut downloaded = None;
    for server in servers.iter().take(5) {
        // Request codes are short-lived, so fetch a fresh one per attempt.
        let code = session
            .manifest_request_code(app_id, depot_id, manifest_id, branch)
            .await?;
        match cdn.manifest(server, depot_id, manifest_id, code).await {
            Ok(bytes) => {
                downloaded = Some((server.clone(), bytes));
                break;
            }
            Err(error) => eprintln!("  {error}, trying the next server"),
        }
    }
    let (server, raw) =
        downloaded.ok_or_else(|| anyhow!("no content server delivered the manifest"))?;
    tracing::info!(
        host = %server.host,
        https = server.https,
        bytes = raw.len(),
        "manifest downloaded"
    );

    let mut manifest = DepotManifest::parse(&raw)?;
    let was_encrypted = manifest.filenames_encrypted;
    manifest.decrypt_filenames(&key)?;
    Ok(FetchedDepot {
        key,
        manifest,
        server,
        was_encrypted,
    })
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
