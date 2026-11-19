//! `reborn-cli`: command-line frontend for Reborn.
//!
//! All logic lives in `reborn-core`; this binary parses arguments, asks for
//! input and prints results. From M2 on it can also talk to `rebornd`.
#![forbid(unsafe_code)]

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};
use clap::{ArgAction, Parser, Subcommand};
use reborn_cm::confirmation::{
    AuthConfirmationHandler, ConsoleAuthConfirmationHandler, DeviceConfirmationHandler,
};
use reborn_cm::{PUBLIC_BRANCH, Platform, Secret};
use reborn_content::cdn::CdnClient;
use reborn_content::chunk::process_chunk;
use reborn_content::path::install_path;
use reborn_core::account;
use reborn_core::install::{
    FetchedDepot, InstallAction, InstallEvent, InstallOutcome, InstallRequest, cdn_hosts,
    fetch_depot, install_app,
};
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
    /// Download and install an app, or update it if it is already installed.
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
    /// Check an installed app against its manifest and repair broken or
    /// missing files (like "verify integrity of game files").
    Verify {
        /// App ID, e.g. 480 for Spacewar.
        app_id: u32,
        /// Library folder (the folder that contains `steamapps`).
        #[arg(long)]
        library: Option<PathBuf>,
        /// Branch to check against.
        #[arg(long, default_value = PUBLIC_BRANCH)]
        branch: String,
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
    /// Talk to a running `rebornd` instead of doing the work here.
    #[command(subcommand)]
    Daemon(DaemonCommand),
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
        } => install(account, app_id, library, branch, false).await,
        Command::Verify {
            app_id,
            library,
            branch,
            account,
        } => install(account, app_id, library, branch, true).await,
        Command::Daemon(command) => daemon(command).await,
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
    eprintln!(
        "Signing in. Confirm in the Steam mobile app, or enter the Steam Guard code when asked."
    );
    let confirmation = ConsoleAuthConfirmationHandler::default().or(DeviceConfirmationHandler);
    let session = account::sign_in_with_password(&account, &password, confirmation).await?;
    println!(
        "Signed in as {account} (SteamID {}). The login is stored in the system keyring.",
        session.steam_id()
    );
    Ok(())
}

async fn library(account: Option<String>, all: bool) -> Result<()> {
    let session = account::sign_in(account).await?;
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
    let session = account::sign_in(account).await?;
    let app = session.app_details(app_id, branch).await?;
    let platform = Platform::current();

    println!(
        "{} (app {app_id}), install folder `{}`, build {}",
        app.name.as_deref().unwrap_or("(unknown)"),
        app.install_dir.as_deref().unwrap_or("?"),
        build_text(app.build_id),
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
    let session = account::sign_in(account).await?;
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

/// Installs, updates (`verify == false`) or verifies and repairs
/// (`verify == true`) an app.
async fn install(
    account: Option<String>,
    app_id: u32,
    library: Option<PathBuf>,
    branch: String,
    verify: bool,
) -> Result<()> {
    let session = account::sign_in(account).await?;
    let request = InstallRequest {
        app_id,
        library,
        branch,
        verify,
    };
    let outcome = install_app(&session, &request, print_event).await?;
    eprintln!();

    let summary = match outcome {
        InstallOutcome::UpToDate { name, build_id } => {
            println!(
                "{name} is up to date (build {}). Use `reborn-cli verify {app_id}` to check its files.",
                build_text(build_id)
            );
            return Ok(());
        }
        InstallOutcome::Done(summary) => summary,
    };
    let report = summary.report;
    let done = match summary.action {
        InstallAction::Verify => "Verified",
        InstallAction::Update => "Updated",
        InstallAction::Install => "Installed",
    };
    println!(
        "{done} {} in {:.1} s: {} files written, {} unchanged, {} removed; \
         {} chunks downloaded ({}), {} reused",
        summary.name,
        summary.elapsed.as_secs_f64(),
        report.files,
        report.files_unchanged,
        report.files_removed,
        report.chunks_downloaded,
        human_size(report.bytes_downloaded),
        report.chunks_reused,
    );
    if report.user_config_kept > 0 {
        println!(
            "  {} existing user config files kept",
            report.user_config_kept
        );
    }
    if report.symlinks_skipped > 0 {
        println!(
            "  {} symlinks were skipped (not supported here)",
            report.symlinks_skipped
        );
    }
    println!("  Game folder: {}", summary.install_dir.display());
    println!("  Steam manifest: {}", summary.app_manifest.display());
    Ok(())
}

/// Prints install events to stderr.
fn print_event(event: InstallEvent) {
    match event {
        InstallEvent::SteamLibrary(library) => eprintln!(
            "Note: {} is a Steam library. The Steam client must be closed while Reborn writes to it.",
            library.display()
        ),
        InstallEvent::Started {
            action,
            name,
            build_id,
            install_dir,
        } => {
            let verb = match action {
                InstallAction::Verify => "Verifying",
                InstallAction::Update => "Updating",
                InstallAction::Install => "Installing",
            };
            eprintln!(
                "{verb} {name} (build {}) in {}",
                build_text(build_id),
                install_dir.display()
            );
        }
        InstallEvent::Depot {
            depot_id,
            files,
            size,
            update_from,
        } => eprintln!(
            "  depot {depot_id}: {files} files, {}{}",
            human_size(size),
            update_from
                .map(|previous| format!(" (update from manifest {previous})"))
                .unwrap_or_default()
        ),
        InstallEvent::PreviousManifestUnavailable {
            depot_id,
            manifest_id,
            error,
        } => eprintln!(
            "  depot {depot_id}: installed manifest {manifest_id} unavailable ({error}); \
             files of the old version will not be removed"
        ),
        InstallEvent::Progress(progress) => {
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
        }
        _ => {}
    }
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Version, account and active jobs of the daemon.
    Status,
    /// Apps owned by the daemon's account.
    Library,
    /// Apps installed in a library folder.
    Installed {
        /// Library folder (defaults to Reborn's own library).
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// Queue an install, update or verification and follow its progress.
    Install {
        /// App ID.
        app_id: u32,
        /// Library folder (defaults to Reborn's own library).
        #[arg(long)]
        library: Option<PathBuf>,
        /// Branch (defaults to `public`).
        #[arg(long)]
        branch: Option<String>,
        /// Verify and repair instead of a normal install/update.
        #[arg(long)]
        verify: bool,
        /// Only queue the job, do not follow its progress.
        #[arg(long)]
        detach: bool,
    },
    /// List the daemon's jobs.
    Jobs,
    /// Cancel a queued or running job (a cancelled download resumes later).
    Cancel {
        /// Job ID.
        job_id: u64,
    },
    /// Print events from the daemon until Ctrl+C.
    Watch,
    /// Stop the daemon.
    Shutdown,
}

async fn daemon(command: DaemonCommand) -> Result<()> {
    use reborn_ipc::protocol::{Event, JobResult, JobState};
    use reborn_ipc::{Client, Request, Response};

    let client = Client::connect(concat!("reborn-cli ", env!("CARGO_PKG_VERSION"))).await?;
    let request = match &command {
        DaemonCommand::Status => Request::Status,
        DaemonCommand::Library => Request::Library,
        DaemonCommand::Installed { library } => Request::Installed {
            library: library.clone(),
        },
        DaemonCommand::Install {
            app_id,
            library,
            branch,
            verify,
            ..
        } => Request::Install {
            app_id: *app_id,
            library: library.clone(),
            branch: branch.clone(),
            verify: *verify,
        },
        DaemonCommand::Jobs => Request::Jobs,
        DaemonCommand::Cancel { job_id } => Request::Cancel { job_id: *job_id },
        DaemonCommand::Watch => {
            eprintln!(
                "Connected to rebornd {}. Waiting for events (Ctrl+C to stop).",
                client.daemon_version()
            );
            while let Some(event) = client.next_event().await {
                println!("{event:?}");
            }
            return Ok(());
        }
        DaemonCommand::Shutdown => Request::Shutdown,
    };

    let response = client.request(request).await?;
    match response {
        Response::Error { message } => bail!("rebornd: {message}"),
        Response::Status {
            daemon_version,
            account,
            signed_in,
            active_jobs,
            stored_login,
        } => println!(
            "rebornd {daemon_version}: account {}, {}, {active_jobs} active jobs",
            account.as_deref().unwrap_or("(none)"),
            match (signed_in, stored_login) {
                (true, _) => "signed in",
                (false, true) => "not signed in yet (login stored)",
                (false, false) => "no stored login",
            }
        ),
        Response::Library { mut apps } => {
            apps.sort_by_cached_key(|app| app.name.to_lowercase());
            for app in &apps {
                println!("{:>8}  {:<11}  {}", app.app_id, app.app_type, app.name);
            }
            println!("{} apps", apps.len());
        }
        Response::Installed { library, apps } => {
            println!("Library {}", library.display());
            for app in &apps {
                println!(
                    "{:>8}  build {:<10}  {:>10}  {}{}",
                    app.app_id,
                    app.build_id.map(|b| b.to_string()).unwrap_or_default(),
                    app.size_on_disk.map(human_size).unwrap_or_default(),
                    app.name.as_deref().unwrap_or(&app.install_dir),
                    if app.fully_installed {
                        ""
                    } else {
                        " (incomplete)"
                    }
                );
            }
            println!("{} installed", apps.len());
        }
        Response::Jobs { jobs } => {
            for job in &jobs {
                let state = match &job.state {
                    JobState::Queued => "queued".to_owned(),
                    JobState::Running {
                        bytes_done,
                        bytes_total,
                    } => format!(
                        "running {} / {}",
                        human_size(*bytes_done),
                        human_size(*bytes_total)
                    ),
                    JobState::Finished { result } => format!("finished: {result:?}"),
                };
                println!(
                    "job {:>3}  app {:>8}  {:?}  {}  {state}",
                    job.job_id,
                    job.app_id,
                    job.kind,
                    job.name.as_deref().unwrap_or("")
                );
            }
            if jobs.is_empty() {
                println!("no jobs");
            }
        }
        Response::JobQueued { job_id } => {
            println!("Queued job {job_id}.");
            let follow = matches!(command, DaemonCommand::Install { detach: false, .. });
            if !follow {
                return Ok(());
            }
            while let Some(event) = client.next_event().await {
                match event {
                    Event::JobStarted {
                        job_id: id, name, ..
                    } if id == job_id => {
                        eprintln!("Started: {name}");
                    }
                    Event::JobProgress {
                        job_id: id,
                        bytes_done,
                        bytes_total,
                    } if id == job_id => eprint!(
                        "\r  {} / {}   ",
                        human_size(bytes_done),
                        human_size(bytes_total)
                    ),
                    Event::JobFinished { job_id: id, result } if id == job_id => {
                        eprintln!();
                        match result {
                            JobResult::Failed { message } => {
                                bail!("job {job_id} failed: {message}")
                            }
                            other => println!("Job {job_id} finished: {other:?}"),
                        }
                        return Ok(());
                    }
                    Event::ShuttingDown => bail!("rebornd is shutting down"),
                    _ => {}
                }
            }
            bail!("connection to rebornd closed");
        }
        Response::Ok => println!("ok"),
        other => println!("{other:?}"),
    }
    Ok(())
}

fn logout(account: Option<String>) -> Result<()> {
    let account = account::resolve_account(account)?;
    if account::sign_out(&account)? {
        println!("Removed the stored login of {account} from this computer.");
    } else {
        println!("No stored login for {account} found.");
    }
    Ok(())
}

fn build_text(build_id: Option<u32>) -> String {
    build_id.map_or_else(|| "?".to_owned(), |build| build.to_string())
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
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }
}
