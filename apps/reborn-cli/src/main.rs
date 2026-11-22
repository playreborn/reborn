//! `reborn-cli`: command-line frontend for Reborn.
//!
//! All logic lives in `reborn-core`; this binary parses arguments, asks for
//! input and prints results. From M2 on it can also talk to `rebornd`.
#![forbid(unsafe_code)]

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
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
    /// Save an app's info as Steam sends it (text VDF), e.g. to record a
    /// test fixture. App info is public data; still check the file before
    /// sharing it.
    Appinfo {
        /// App ID.
        app_id: u32,
        /// File to write.
        #[arg(long)]
        output: PathBuf,
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
    /// Ask Valve for the ownership ticket and the encrypted app ticket of
    /// an app (ADR 0027). Only the sizes are shown, never the tickets.
    Ticket {
        /// App ID, e.g. 480 for Spacewar.
        app_id: u32,
        /// Instead, record Steam's answers as redacted fixtures into this
        /// folder (ticket bytes replaced by zeros).
        #[arg(long)]
        record: Option<PathBuf>,
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
    /// Show the achievements of an app for the account (ADR 0029). Only
    /// reads; nothing is ever unlocked or changed.
    Achievements {
        /// App ID, e.g. 480 for Spacewar.
        app_id: u32,
        /// Language of the texts, as Steam names it (e.g. german, english).
        #[arg(long, default_value = "english")]
        language: String,
        /// Instead, record Steam's answer as a fixture into this folder.
        #[arg(long)]
        record: Option<PathBuf>,
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
    /// List the files of an app in the account's Steam Cloud (ADR 0031).
    /// Nothing is uploaded or deleted.
    Cloud {
        /// App ID, e.g. 480 for Spacewar.
        app_id: u32,
        /// Download all files into this folder (ADR 0032): verified against
        /// Steam's SHA-1, existing files are never overwritten. Use a folder
        /// of your own, not the game's save folder.
        #[arg(long, conflicts_with_all = ["record", "compare", "restore"])]
        download: Option<PathBuf>,
        /// Compare the files with the ones on this computer (ADR 0033):
        /// where each belongs, and whether it is the same, differs or is
        /// missing. Only reads.
        #[arg(long, conflicts_with_all = ["record", "restore"])]
        compare: bool,
        /// Bring saves that are missing on this computer from the cloud
        /// into the game's save folders (ADR 0034). Writes only new files
        /// and stops without writing anything if a local save differs or
        /// is not in the cloud.
        #[arg(long, conflicts_with = "record")]
        restore: bool,
        /// Instead, record Steam's answer as a redacted fixture into this
        /// folder.
        #[arg(long)]
        record: Option<PathBuf>,
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
    /// Show the depots and launch entries of an app and which ones fit
    /// this computer.
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
    /// Show your friends with status and game, or answer and send friend
    /// requests.
    Friends {
        /// Send, accept or decline a friend request.
        #[command(subcommand)]
        action: Option<FriendAction>,
        /// Instead of listing, record the friends list and persona answers as
        /// redacted test fixtures into this folder (SteamIDs and names
        /// replaced), and the account info Steam sent at logon with the
        /// account's own name replaced (`account_info.bin`).
        #[arg(long)]
        record: Option<PathBuf>,
        /// Account to use (defaults to the last account that signed in).
        #[arg(long)]
        account: Option<String>,
    },
    /// Show the recent messages with a friend, or send one.
    Chat {
        /// The friend: friend code, SteamID64 or profile URL.
        who: String,
        /// Send this message (visible to the friend).
        #[arg(long)]
        send: Option<String>,
        /// Instead, record the recent history, the unread counts and the
        /// messages and typing notices arriving in the next 30 seconds as
        /// redacted fixtures into this folder (texts, SteamIDs replaced).
        #[arg(long)]
        record: Option<PathBuf>,
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
        Command::Appinfo {
            app_id,
            output,
            account,
        } => appinfo(account, app_id, &output).await,
        Command::Ticket {
            app_id,
            record,
            account,
        } => ticket(account, app_id, record.as_deref()).await,
        Command::Achievements {
            app_id,
            language,
            record,
            account,
        } => achievements(account, app_id, &language, record.as_deref()).await,
        Command::Cloud {
            app_id,
            download,
            compare,
            restore,
            record,
            account,
        } => match download {
            Some(folder) => cloud_download(account, app_id, &folder).await,
            None if compare => cloud_compare(account, app_id).await,
            None if restore => cloud_restore(account, app_id).await,
            None => cloud(account, app_id, record.as_deref()).await,
        },
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
        Command::Chat {
            who,
            send,
            record,
            account,
        } => chat(account, &who, send.as_deref(), record.as_deref()).await,
        Command::Friends {
            action: Some(action),
            account,
            ..
        } => friend_action(account, action).await,
        Command::Friends {
            action: None,
            record,
            account,
        } => friends(account, record.as_deref()).await,
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
    if app.launch.is_empty() {
        println!("No launch entries.");
    }
    for option in &app.launch {
        let selected = option.applies_to(&platform, branch);
        let os = if option.os_list.is_empty() {
            "all".to_owned()
        } else {
            option.os_list.join(",")
        };
        println!(
            "{} launch {:>2}  {:<14} {:<8} {} {}{}",
            if selected { '*' } else { ' ' },
            option.index,
            os,
            option.launch_type.as_deref().unwrap_or("-"),
            option.executable,
            option.arguments.as_deref().unwrap_or(""),
            option
                .description
                .as_deref()
                .map(|d| format!("  ({d})"))
                .unwrap_or_default(),
        );
    }
    Ok(())
}

async fn ticket(account: Option<String>, app_id: u32, record: Option<&Path>) -> Result<()> {
    let session = account::sign_in(account).await?;
    if let Some(dir) = record {
        let recording = session.record_tickets(app_id).await?;
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        std::fs::write(dir.join("ownership_ticket.bin"), &recording.ownership)?;
        std::fs::write(dir.join("encrypted_app_ticket.bin"), &recording.encrypted)?;
        println!("Recorded both answers (redacted) into {}.", dir.display());
        return Ok(());
    }
    match session.app_ownership_ticket(app_id).await {
        Ok(ticket) => println!("Ownership ticket for app {app_id}: {} bytes.", ticket.len()),
        Err(error) => println!("Ownership ticket for app {app_id}: {error}"),
    }
    match session.encrypted_app_ticket(app_id, &[]).await {
        Ok(ticket) => println!(
            "Encrypted app ticket for app {app_id}: {} bytes.",
            ticket.ticket.len()
        ),
        Err(error) => println!("Encrypted app ticket for app {app_id}: {error}"),
    }
    Ok(())
}

async fn achievements(
    account: Option<String>,
    app_id: u32,
    language: &str,
    record: Option<&Path>,
) -> Result<()> {
    let session = account::sign_in(account).await?;
    if let Some(dir) = record {
        let recording = session.record_achievements(app_id).await?;
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        std::fs::write(dir.join("user_stats.bin"), &recording)?;
        println!("Recorded Steam's answer into {}.", dir.display());
        return Ok(());
    }
    let achievements = session.achievements(app_id, language).await?;
    if achievements.is_empty() {
        println!("App {app_id} has no achievements.");
        return Ok(());
    }
    let unlocked = achievements
        .iter()
        .filter(|achievement| achievement.unlocked_at.is_some())
        .count();
    println!(
        "{unlocked} of {} achievements unlocked in app {app_id}:",
        achievements.len()
    );
    for achievement in &achievements {
        let hidden = achievement.hidden && achievement.unlocked_at.is_none();
        let mark = if achievement.unlocked_at.is_some() {
            "x"
        } else {
            " "
        };
        let when = achievement
            .unlocked_at
            .map(|time| format!(" ({})", utc_time(time)))
            .unwrap_or_default();
        if hidden {
            println!("[{mark}] {} (hidden){when}", achievement.display_name);
        } else if achievement.description.is_empty() {
            println!("[{mark}] {}{when}", achievement.display_name);
        } else {
            println!(
                "[{mark}] {}: {}{when}",
                achievement.display_name, achievement.description
            );
        }
    }
    Ok(())
}

async fn cloud(account: Option<String>, app_id: u32, record: Option<&Path>) -> Result<()> {
    let session = account::sign_in(account).await?;
    if let Some(dir) = record {
        let recording = session.record_cloud_files(app_id).await?;
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        std::fs::write(dir.join("cloud_files.bin"), &recording)?;
        println!("Recorded Steam's answer (redacted) into {}.", dir.display());
        return Ok(());
    }
    let files = session.cloud_files(app_id).await?;
    if files.is_empty() {
        println!("App {app_id} has no files in Steam Cloud.");
        return Ok(());
    }
    println!("{} files of app {app_id} in Steam Cloud:", files.len());
    for file in files {
        let time = u32::try_from(file.timestamp)
            .map(utc_time)
            .unwrap_or_else(|_| file.timestamp.to_string());
        println!(
            "{} · {} · {time} · {}",
            file.name,
            human_size(u64::from(file.size)),
            file.platforms.join(",")
        );
    }
    Ok(())
}

/// Compares an app's cloud files with this computer (ADR 0033).
async fn cloud_compare(account: Option<String>, app_id: u32) -> Result<()> {
    use reborn_core::cloud::{CloudState, compare_app_cloud};

    let session = account::sign_in(account).await?;
    let report = compare_app_cloud(&session, app_id, None).await?;
    if !report.has_cloud {
        println!("App {app_id} does not use Steam Cloud.");
    } else if !report.auto_cloud {
        println!("App {app_id} saves through the Steam interface (no Auto-Cloud rules).");
    }
    for file in &report.files {
        let local = file
            .local
            .as_ref()
            .map(|path| format!(" ({})", path.display()))
            .unwrap_or_default();
        let state = match &file.state {
            CloudState::Same => "same".to_owned(),
            CloudState::MissingLocally => "missing locally".to_owned(),
            CloudState::Differs {
                local_modified,
                cloud_modified,
            } => {
                let local_time = local_modified
                    .and_then(|time| u32::try_from(time).ok())
                    .map(utc_time)
                    .unwrap_or_else(|| "?".to_owned());
                let cloud_time = u32::try_from(*cloud_modified)
                    .map(utc_time)
                    .unwrap_or_else(|_| cloud_modified.to_string());
                format!("differs (local {local_time}, cloud {cloud_time})")
            }
            CloudState::NotMapped(reason) => format!("not mapped: {reason}"),
        };
        println!("{}: {state}{local}", file.name);
    }
    if report.files.is_empty() {
        println!("No files in Steam Cloud.");
    }
    for path in &report.local_only {
        println!("only local: {}", path.display());
    }
    Ok(())
}

/// Restores saves that are missing locally (ADR 0034).
async fn cloud_restore(account: Option<String>, app_id: u32) -> Result<()> {
    use reborn_core::cloud::{RestoreBlocked, RestoreOutcome, restore_missing_saves};

    let session = account::sign_in(account).await?;
    let report = match restore_missing_saves(&session, app_id, None).await? {
        RestoreOutcome::Blocked(RestoreBlocked::Differs(names)) => {
            for name in &names {
                println!("differs: {name}");
            }
            bail!("nothing written: local saves differ from the cloud; decide which to keep");
        }
        RestoreOutcome::Blocked(RestoreBlocked::LocalOnly(paths)) => {
            for path in &paths {
                println!("only local: {}", path.display());
            }
            bail!(
                "nothing written: these local saves are not in the cloud \
                 (unsynced progress or another account's saves)"
            );
        }
        RestoreOutcome::Blocked(RestoreBlocked::RootMissing(path)) => {
            bail!("nothing written: {} does not exist", path.display());
        }
        RestoreOutcome::Done(report) => report,
    };
    for path in &report.restored {
        println!("restored: {}", path.display());
    }
    for path in &report.unchanged {
        println!("already there: {}", path.display());
    }
    for (name, reason) in &report.not_mapped {
        println!("skipped {name}: {reason}");
    }
    for (name, error) in &report.failed {
        println!("failed {name}: {error}");
    }
    if !report.failed.is_empty() {
        bail!("{} files could not be restored", report.failed.len());
    }
    println!(
        "{} restored, {} already there.",
        report.restored.len(),
        report.unchanged.len()
    );
    Ok(())
}

/// Downloads an app's cloud files into `folder` (ADR 0032), one after the
/// other. Links are never printed.
async fn cloud_download(account: Option<String>, app_id: u32, folder: &Path) -> Result<()> {
    let session = account::sign_in(account).await?;
    let files = session.cloud_files(app_id).await?;
    if files.is_empty() {
        println!("App {app_id} has no files in Steam Cloud.");
        return Ok(());
    }
    let client = CdnClient::new()?;
    let mut failed = 0;
    for file in &files {
        let Some(url) = &file.download_url else {
            println!("{}: Steam sent no download link", file.name);
            failed += 1;
            continue;
        };
        let reference = reborn_content::cloud::CloudFileRef {
            name: &file.name,
            url: url.expose(),
            size: file.size,
            sha1: &file.sha1,
        };
        match reborn_content::cloud::download_cloud_file(&client, folder, reference).await {
            Ok(reborn_content::cloud::CloudOutcome::Written(path)) => {
                println!("{}: written to {}", file.name, path.display());
            }
            Ok(reborn_content::cloud::CloudOutcome::AlreadyThere(path)) => {
                println!(
                    "{}: already there, same content ({})",
                    file.name,
                    path.display()
                );
            }
            Err(error) => {
                println!("{}: {error}", file.name);
                failed += 1;
            }
        }
    }
    if failed > 0 {
        bail!("{failed} of {} files were not downloaded", files.len());
    }
    println!("All {} files are in {}.", files.len(), folder.display());
    Ok(())
}

/// `YYYY-MM-DD HH:MM UTC` for seconds since the Unix epoch. Days to dates
/// with Howard Hinnant's `civil_from_days` (public domain,
/// <https://howardhinnant.github.io/date_algorithms.html>).
fn utc_time(seconds: u32) -> String {
    let seconds = i64::from(seconds);
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        of_day / 3600,
        of_day % 3600 / 60
    )
}

async fn appinfo(account: Option<String>, app_id: u32, output: &Path) -> Result<()> {
    let session = account::sign_in(account).await?;
    let text = session.app_info_text(app_id).await?;
    std::fs::write(output, text).with_context(|| format!("cannot write {}", output.display()))?;
    println!(
        "Saved the app info of app {app_id} to {}.",
        output.display()
    );
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
        println!(
            "  {kind} {:>10}  flags {:#06x}{}  {}",
            human_size(file.size),
            file.flags,
            if file.is_executable() { " x" } else { "  " },
            file.name
        );
    }
    if manifest.files.len() > 15 {
        println!("  ... {} more", manifest.files.len() - 15);
    }
    let executables: Vec<&str> = manifest
        .files
        .iter()
        .filter(|file| file.is_executable())
        .map(|file| file.name.as_str())
        .collect();
    println!(
        "{} files are marked executable{}{}",
        executables.len(),
        if executables.is_empty() { "" } else { ": " },
        executables
            .iter()
            .take(10)
            .copied()
            .collect::<Vec<_>>()
            .join(", ")
    );

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
enum FriendAction {
    /// Send a friend request (visible to the other person).
    Add {
        /// Friend code, SteamID64 or profile URL (`…/profiles/<number>`).
        who: String,
        /// Also record Steam's answer and the following list changes as
        /// redacted fixtures into this folder.
        #[arg(long)]
        record: Option<PathBuf>,
    },
    /// Accept a friend request someone sent you.
    Accept {
        /// Their SteamID64, friend code or profile URL.
        who: String,
    },
    /// Decline a friend request someone sent you.
    Decline {
        /// Their SteamID64, friend code or profile URL.
        who: String,
    },
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
    /// Remove an installed app through the daemon (deletes its files).
    Uninstall {
        /// App ID.
        app_id: u32,
        /// Library folder (defaults to Reborn's own library).
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// Start an installed game through the daemon.
    Launch {
        /// App ID.
        app_id: u32,
        /// Library folder (defaults to Reborn's own library).
        #[arg(long)]
        library: Option<PathBuf>,
        /// Launch entry index (defaults to the entry for this computer).
        #[arg(long)]
        option: Option<u32>,
        /// After a cloud conflict in shim mode (ADR 0055): play with the
        /// `local` files in Reborn's cloud folder or the `cloud` version,
        /// after confirming the conflicting files; the other is kept as a
        /// backup.
        #[arg(long, value_parser = ["local", "cloud"])]
        cloud: Option<String>,
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
        DaemonCommand::Uninstall { app_id, library } => Request::Uninstall {
            app_id: *app_id,
            library: library.clone(),
        },
        // Always sent without an answer first: a cloud conflict is only
        // resolved after the user saw it (below).
        DaemonCommand::Launch {
            app_id,
            library,
            option,
            ..
        } => Request::Launch {
            app_id: *app_id,
            library: library.clone(),
            option: *option,
            cloud_choice: None,
        },
        DaemonCommand::Shutdown => Request::Shutdown,
    };
    let cloud_choice = match &command {
        DaemonCommand::Launch { cloud, .. } => cloud.as_deref().map(|choice| match choice {
            "cloud" => reborn_ipc::protocol::CloudChoice::Cloud,
            _ => reborn_ipc::protocol::CloudChoice::Local,
        }),
        _ => None,
    };

    let mut response = client.request(request.clone()).await?;
    if let (Response::CloudConflict { app_id, files }, Request::Launch { .. }) =
        (&response, &request)
    {
        print_cloud_conflict(*app_id, files);
        let Some(choice) = cloud_choice else {
            println!(
                "Start it again with `--cloud local` to play with Reborn's files or \
                 `--cloud cloud` to play with the Steam Cloud version; you are asked to \
                 confirm the files first, and the other version is kept as a backup."
            );
            return Ok(());
        };
        let version = match choice {
            reborn_ipc::protocol::CloudChoice::Cloud => "the Steam Cloud version",
            _ => "Reborn's files",
        };
        let confirm = prompt_line(&format!(
            "Play with {version} for these {} files? Type `yes` to confirm: ",
            files.len()
        ))?;
        if confirm != "yes" {
            println!("Not started; nothing was changed.");
            return Ok(());
        }
        let Request::Launch {
            app_id,
            library,
            option,
            ..
        } = request
        else {
            unreachable!("matched above");
        };
        let answer = reborn_ipc::protocol::CloudAnswer::for_conflicts(choice, files);
        response = client
            .request(Request::Launch {
                app_id,
                library,
                option,
                cloud_choice: Some(answer),
            })
            .await?;
        if let Response::CloudConflict { app_id, files } = &response {
            print_cloud_conflict(*app_id, files);
            println!("The saves changed again while you chose; start it again to see them.");
            return Ok(());
        }
    }
    match response {
        Response::Error { message } => bail!("rebornd: {message}"),
        Response::Status {
            daemon_version,
            account,
            signed_in,
            active_jobs,
            stored_login,
            running_games,
        } => println!(
            "rebornd {daemon_version}: account {}, {}, {active_jobs} active jobs, {} games running",
            account.as_deref().unwrap_or("(none)"),
            match (signed_in, stored_login) {
                (true, _) => "signed in",
                (false, true) => "not signed in yet (login stored)",
                (false, false) => "no stored login",
            },
            running_games.len()
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
        Response::Launched { app_id } => println!("Started app {app_id}."),
        Response::Ok => println!("ok"),
        other => println!("{other:?}"),
    }
    Ok(())
}

/// Prints a cloud conflict that refused a shim-mode launch (ADR 0055).
fn print_cloud_conflict(app_id: u32, files: &[reborn_ipc::protocol::CloudConflictFile]) {
    println!(
        "App {app_id} was not started: these saves differ between Reborn's cloud folder and \
         Steam Cloud. Reborn's cloud folder was not changed."
    );
    for file in files {
        println!("{}", cloud_conflict_line(file));
    }
}

/// One conflicting file, with readable times.
fn cloud_conflict_line(file: &reborn_ipc::protocol::CloudConflictFile) -> String {
    let time = |seconds: u64| u32::try_from(seconds).map_or_else(|_| seconds.to_string(), utc_time);
    let how = if file.had_base {
        "both changed since the last download"
    } else {
        "no earlier download known, so which is newer is unknown"
    };
    format!(
        "  {}: here {} bytes (changed {}), Steam Cloud {} bytes (changed {}); {how}",
        file.name,
        file.local_size,
        file.local_modified.map_or_else(|| "?".to_owned(), time),
        file.cloud_size,
        time(file.cloud_modified)
    )
}

async fn chat(
    account: Option<String>,
    who: &str,
    send: Option<&str>,
    record: Option<&Path>,
) -> Result<()> {
    let friend = reborn_cm::steam_id_from_input(who)
        .ok_or_else(|| anyhow!("`{who}` is not a friend code, SteamID64 or profile URL"))?;
    let session = account::sign_in(account).await?;
    if let Some(dir) = record {
        println!("Recording; messages arriving in the next 30 seconds are included.");
        let recording = session
            .record_chat(friend, std::time::Duration::from_secs(30))
            .await?;
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        std::fs::write(dir.join("chat_history.bin"), &recording.history)?;
        std::fs::write(dir.join("chat_sessions.bin"), &recording.sessions)?;
        for (index, bytes) in recording.incoming.iter().enumerate() {
            std::fs::write(dir.join(format!("chat_incoming_{index}.bin")), bytes)?;
        }
        println!(
            "Recorded the history, unread counts and {} incoming notices (redacted) into {}.",
            recording.incoming.len(),
            dir.display()
        );
        return Ok(());
    }
    if let Some(text) = send {
        if text.trim().is_empty() {
            bail!("the message is empty");
        }
        session.send_message(friend, text).await?;
        println!("Sent.");
        return Ok(());
    }
    let messages = session.recent_messages(friend, 30).await?;
    if messages.is_empty() {
        println!("No messages yet.");
    }
    for message in &messages {
        let who = if message.from_me { "you" } else { "them" };
        println!("[{}] {who}: {}", message.timestamp, message.text);
    }
    Ok(())
}

async fn friend_action(account: Option<String>, action: FriendAction) -> Result<()> {
    let parse = |who: &str| {
        reborn_cm::steam_id_from_input(who)
            .ok_or_else(|| anyhow!("`{who}` is not a friend code, SteamID64 or profile URL"))
    };
    let session = account::sign_in(account).await?;
    match action {
        FriendAction::Add { who, record } => {
            let steam_id = parse(&who)?;
            if let Some(dir) = record {
                let recording = session
                    .record_add_friend(steam_id, std::time::Duration::from_secs(5))
                    .await?;
                std::fs::create_dir_all(&dir)
                    .with_context(|| format!("cannot create {}", dir.display()))?;
                std::fs::write(dir.join("add_friend_response.bin"), &recording.response)?;
                for (index, bytes) in recording.changes.iter().enumerate() {
                    std::fs::write(dir.join(format!("friends_change_{index}.bin")), bytes)?;
                }
                println!(
                    "Recorded the answer and {} list changes (redacted) into {}.",
                    recording.changes.len(),
                    dir.display()
                );
                let name = recording.result?;
                println!(
                    "Friend request sent to {}.",
                    name.as_deref().unwrap_or("that account")
                );
            } else {
                let name = session.add_friend(steam_id).await?;
                println!(
                    "Friend request sent to {}.",
                    name.as_deref().unwrap_or("that account")
                );
            }
        }
        FriendAction::Accept { who } => {
            let name = session.add_friend(parse(&who)?).await?;
            println!(
                "Accepted the request of {}.",
                name.as_deref().unwrap_or("that account")
            );
        }
        FriendAction::Decline { who } => {
            parse(&who)?;
            // On today's Steam, `ClientRemoveFriend` on a pending request left
            // the sender blocked (live, 2026-11-20).
            bail!(
                "declining is disabled for now: on current Steam it blocked the sender. \
                 Ignore the request in the official client or on the Steam website"
            );
        }
    }
    Ok(())
}

async fn friends(account: Option<String>, record: Option<&Path>) -> Result<()> {
    use futures_util::StreamExt;
    use reborn_cm::{Persona, PersonaState, Relationship};

    let session = account::sign_in(account).await?;
    if let Some(dir) = record {
        let recording = session
            .record_friends(std::time::Duration::from_secs(5))
            .await?;
        std::fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
        std::fs::write(dir.join("friends_list.bin"), &recording.friends_list)?;
        for (index, bytes) in recording.persona_states.iter().enumerate() {
            std::fs::write(dir.join(format!("persona_state_{index}.bin")), bytes)?;
        }
        // Only the name field, replaced by `Player 1` (ADR 0056).
        match session.record_account_info() {
            Some(bytes) => std::fs::write(dir.join("account_info.bin"), bytes)?,
            None => println!("Steam sent no account info in this session."),
        }
        println!(
            "Recorded the friends list and {} persona messages (redacted) into {}.",
            recording.persona_states.len(),
            dir.display()
        );
        return Ok(());
    }

    let list = session.friends().await?;
    // The friend code is the account ID, the low 32 bits (ADR 0022).
    println!(
        "Your friend code: {}",
        session.steam_id() & u64::from(u32::MAX)
    );
    let mut updates = Box::pin(session.persona_updates());
    let ids: Vec<u64> = list.iter().map(|friend| friend.steam_id).collect();
    session.request_personas(&ids).await?;
    let mut personas: std::collections::HashMap<u64, Persona> = std::collections::HashMap::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
    while let Ok(Some(batch)) = tokio::time::timeout_at(deadline, updates.next()).await {
        for persona in batch {
            personas.insert(persona.steam_id, persona);
        }
    }
    if list.is_empty() {
        println!("No friends on this account yet.");
    }
    for friend in &list {
        let persona = personas.get(&friend.steam_id);
        let name = persona
            .and_then(|p| p.name.as_deref())
            .unwrap_or("(name not received)");
        let status = match persona.and_then(|p| p.state) {
            Some(PersonaState::Offline | PersonaState::Invisible) => "offline",
            Some(PersonaState::Online) => "online",
            Some(PersonaState::Busy) => "busy",
            Some(PersonaState::Away | PersonaState::Snooze) => "away",
            Some(PersonaState::LookingToTrade | PersonaState::LookingToPlay) => "online",
            Some(PersonaState::Unknown(_)) | None => "?",
        };
        let game = match persona.and_then(|p| p.game_app_id).filter(|app| *app != 0) {
            Some(app) => format!("  playing app {app}"),
            None => String::new(),
        };
        let relation = match friend.relationship {
            Relationship::Friend => "",
            Relationship::RequestRecipient => "  (sent you a friend request)",
            Relationship::RequestInitiator => "  (your friend request)",
            Relationship::Blocked => "  (Blocked, value 1)",
            Relationship::Ignored => "  (Ignored, value 5)",
            Relationship::IgnoredFriend => "  (IgnoredFriend, value 6)",
            _ => "  (other relationship)",
        };
        println!("{name:<32} {status:<8}{game}{relation}");
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
    fn cloud_conflicts_print_readable_times() {
        let mut file = reborn_ipc::protocol::CloudConflictFile {
            name: "MainPlayerData.json".to_owned(),
            local_sha1: "aa".repeat(20),
            local_size: 10,
            local_modified: Some(0),
            cloud_sha1: "cc".repeat(20),
            cloud_size: 11,
            cloud_modified: 86_400,
            had_base: true,
        };
        let line = cloud_conflict_line(&file);
        assert!(line.contains("changed 1970-01-01 00:00 UTC"), "{line}");
        assert!(line.contains("changed 1970-01-02 00:00 UTC"), "{line}");
        assert!(line.contains("both changed"), "{line}");
        file.had_base = false;
        file.local_modified = None;
        let line = cloud_conflict_line(&file);
        assert!(line.contains("which is newer is unknown"), "{line}");
        assert!(line.contains("changed ?"), "{line}");
    }

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn formats_utc_times() {
        assert_eq!(utc_time(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_time(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc_time(1_700_000_000), "2023-11-14 22:13 UTC");
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }
}
