//! `reborn-cli`: command-line frontend for Reborn.
//!
//! In M0 the CLI talks to Steam directly so the protocol work can be tested
//! without a daemon. From M2 on it becomes a thin client of `rebornd`.
#![forbid(unsafe_code)]

use std::io::{BufRead, Write};

use anyhow::{Context, Result, anyhow, bail};
use clap::{ArgAction, Parser, Subcommand};
use reborn_cm::Secret;
use reborn_cm::confirmation::{
    AuthConfirmationHandler, ConsoleAuthConfirmationHandler, DeviceConfirmationHandler,
};
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

async fn library(account: Option<String>, all: bool) -> Result<()> {
    let account = resolve_account(account)?;
    let token = secrets::load(SecretKind::RefreshToken, &account)?
        .ok_or_else(|| anyhow!("no stored login for `{account}`, run `reborn-cli login` first"))?;
    let session = reborn_cm::login_with_token(&account, &token)
        .await
        .context("signing in with the stored token failed, run `reborn-cli login` again")?;

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
