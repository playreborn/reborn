//! Small, non-secret settings stored as files in the user's config
//! directory (`%APPDATA%\Reborn\config` on Windows,
//! `$XDG_CONFIG_HOME/reborn` on Linux).
//!
//! For now this is only the default account, so `reborn-cli library` works
//! without `--account`. A proper settings file follows with the daemon (M2).

use std::io;
use std::path::PathBuf;

use directories::ProjectDirs;

const DEFAULT_ACCOUNT_FILE: &str = "default-account";

/// Reading or writing a setting failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SettingsError {
    /// The user's home or config directory could not be determined.
    #[error("could not determine the user's config directory")]
    NoConfigDir,
    /// File system error.
    #[error("settings file: {0}")]
    Io(#[from] io::Error),
}

/// The directory where Reborn keeps its settings.
pub fn config_dir() -> Result<PathBuf, SettingsError> {
    ProjectDirs::from("", "", "Reborn")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .ok_or(SettingsError::NoConfigDir)
}

/// The directory for Reborn's local data (`%LOCALAPPDATA%\Reborn\data` on
/// Windows, `$XDG_DATA_HOME/reborn` on Linux).
pub fn data_dir() -> Result<PathBuf, SettingsError> {
    ProjectDirs::from("", "", "Reborn")
        .map(|dirs| dirs.data_local_dir().to_path_buf())
        .ok_or(SettingsError::NoConfigDir)
}

/// Reborn's own game library, used when no library folder is given. It has
/// the same layout as a Steam library (`steamapps/common`, `appmanifest_*.acf`)
/// but is separate from the official client's libraries.
pub fn default_library() -> Result<PathBuf, SettingsError> {
    Ok(data_dir()?.join("library"))
}

/// The account used when no `--account` is given, if set.
pub fn default_account() -> Result<Option<String>, SettingsError> {
    let path = config_dir()?.join(DEFAULT_ACCOUNT_FILE);
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let account = text.trim();
            Ok((!account.is_empty()).then(|| account.to_owned()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Sets the default account.
pub fn set_default_account(account: &str) -> Result<(), SettingsError> {
    let dir = config_dir()?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(DEFAULT_ACCOUNT_FILE), account)?;
    Ok(())
}

/// Removes the default account setting.
pub fn clear_default_account() -> Result<(), SettingsError> {
    match std::fs::remove_file(config_dir()?.join(DEFAULT_ACCOUNT_FILE)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}
