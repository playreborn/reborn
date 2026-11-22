//! Small, non-secret settings stored as files in the user's config
//! directory (`%APPDATA%\Reborn\config` on Windows,
//! `$XDG_CONFIG_HOME/reborn` on Linux).
//!
//! One small text file per setting:
//! - `default-account`: the account used when none is given,
//! - `library`: the folder for Reborn's game library, if not the default,
//! - `proton`: the Proton build to use on Linux, if not the automatic choice,
//! - `own-status`: the online status Reborn sets after sign-in, if any,
//! - `shim-notice-seen`: the app IDs whose shim-mode notice the user
//!   confirmed (ADR 0057), one per line. App IDs only, no account data.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use directories::ProjectDirs;

const DEFAULT_ACCOUNT_FILE: &str = "default-account";
const LIBRARY_FILE: &str = "library";
const PROTON_FILE: &str = "proton";
const OWN_STATUS_FILE: &str = "own-status";
const SHIM_NOTICE_FILE: &str = "shim-notice-seen";

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
    /// The library folder must be an absolute path.
    #[error("the library folder must be an absolute path: {}", .0.display())]
    RelativeLibrary(PathBuf),
    /// The library folder exists but is not a folder.
    #[error("{} is not a folder", .0.display())]
    NotAFolder(PathBuf),
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

/// Reborn's game library, used when no library folder is given: the folder
/// set with [`set_library`], else [`builtin_library`]. It has the same layout
/// as a Steam library (`steamapps/common`, `appmanifest_*.acf`).
pub fn default_library() -> Result<PathBuf, SettingsError> {
    match library_setting()? {
        Some(library) => Ok(library),
        None => builtin_library(),
    }
}

/// The library folder Reborn uses when none is set: `library` in the data
/// directory, separate from the official client's libraries.
pub fn builtin_library() -> Result<PathBuf, SettingsError> {
    Ok(data_dir()?.join("library"))
}

/// The library folder set by the user, if any.
pub fn library_setting() -> Result<Option<PathBuf>, SettingsError> {
    Ok(read_value(&config_dir()?, LIBRARY_FILE)?.map(PathBuf::from))
}

/// Sets the library folder (`None`: back to [`builtin_library`]). The folder
/// must be absolute; it is created with its `steamapps` folder if needed.
/// Games already installed elsewhere stay where they are.
pub fn set_library(library: Option<&Path>) -> Result<(), SettingsError> {
    let dir = config_dir()?;
    match library {
        Some(library) => {
            prepare_library(library)?;
            write_value(&dir, LIBRARY_FILE, Some(&library.to_string_lossy()))
        }
        None => write_value(&dir, LIBRARY_FILE, None),
    }
}

/// Checks a library folder and creates it (with `steamapps`) if missing.
fn prepare_library(library: &Path) -> Result<(), SettingsError> {
    if !library.is_absolute() {
        return Err(SettingsError::RelativeLibrary(library.to_path_buf()));
    }
    if library.exists() && !library.is_dir() {
        return Err(SettingsError::NotAFolder(library.to_path_buf()));
    }
    std::fs::create_dir_all(library.join("steamapps"))?;
    Ok(())
}

/// The Proton build chosen by the user (its display name), if any.
pub fn proton_setting() -> Result<Option<String>, SettingsError> {
    read_value(&config_dir()?, PROTON_FILE)
}

/// Sets the Proton build by display name (`None`: automatic choice).
pub fn set_proton(name: Option<&str>) -> Result<(), SettingsError> {
    write_value(&config_dir()?, PROTON_FILE, name)
}

/// The online status Reborn sets after sign-in (`online`, `away`, ...), if
/// the user chose one (ADR 0021).
pub fn own_status_setting() -> Result<Option<String>, SettingsError> {
    read_value(&config_dir()?, OWN_STATUS_FILE)
}

/// Sets or clears the online status Reborn sets after sign-in.
pub fn set_own_status(status: Option<&str>) -> Result<(), SettingsError> {
    write_value(&config_dir()?, OWN_STATUS_FILE, status)
}

/// The apps whose shim-mode notice the user confirmed (ADR 0057), kept in
/// `shim-notice-seen` in a settings folder.
#[derive(Clone, Debug)]
pub struct ShimNotices {
    dir: PathBuf,
}

impl ShimNotices {
    /// The confirmations in the user's config directory.
    pub fn in_config_dir() -> Result<Self, SettingsError> {
        Ok(Self::new(config_dir()?))
    }

    /// The confirmations kept in `dir` (tests use a temporary folder).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Whether the user already confirmed the notice for this app.
    pub fn seen(&self, app_id: u32) -> Result<bool, SettingsError> {
        Ok(shim_notice_apps(&self.dir)?.contains(&app_id))
    }

    /// Remembers that the user confirmed the notice for this app.
    pub fn remember(&self, app_id: u32) -> Result<(), SettingsError> {
        remember_shim_notice_in(&self.dir, app_id)
    }
}

/// The app IDs in `shim-notice-seen`. Lines that are not an app ID are
/// skipped (and dropped on the next write).
fn shim_notice_apps(dir: &Path) -> Result<BTreeSet<u32>, SettingsError> {
    Ok(read_value(dir, SHIM_NOTICE_FILE)?
        .map(|text| {
            text.lines()
                .filter_map(|line| line.trim().parse().ok())
                .collect()
        })
        .unwrap_or_default())
}

/// Guards the read-modify-write of `shim-notice-seen`, so two first
/// launches at the same time cannot lose an app ID. Only `rebornd` writes
/// the file, and only one `rebornd` runs per user.
static SHIM_NOTICE_LOCK: Mutex<()> = Mutex::new(());

/// Adds an app ID to `shim-notice-seen`. The new list is written to a
/// temporary file first and then renamed over the old one, so a crash
/// leaves either the old or the new list, never half of one.
fn remember_shim_notice_in(dir: &Path, app_id: u32) -> Result<(), SettingsError> {
    // A panic while holding the lock leaves nothing half-written (the
    // rename is atomic), so a poisoned lock is still safe to use.
    let _guard = SHIM_NOTICE_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut apps = shim_notice_apps(dir)?;
    if !apps.insert(app_id) {
        return Ok(());
    }
    let text = apps
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::create_dir_all(dir)?;
    let temporary = dir.join(format!("{SHIM_NOTICE_FILE}.part"));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, dir.join(SHIM_NOTICE_FILE))?;
    Ok(())
}

/// Reads a one-line setting; missing or empty files mean "not set".
fn read_value(dir: &Path, name: &str) -> Result<Option<String>, SettingsError> {
    match std::fs::read_to_string(dir.join(name)) {
        Ok(text) => {
            let value = text.trim();
            Ok((!value.is_empty()).then(|| value.to_owned()))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Writes a one-line setting, or removes it for `None` or an empty value.
fn write_value(dir: &Path, name: &str, value: Option<&str>) -> Result<(), SettingsError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => {
            std::fs::create_dir_all(dir)?;
            std::fs::write(dir.join(name), value)?;
        }
        None => match std::fs::remove_file(dir.join(name)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        },
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("reborn-settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn values_round_trip_and_clear() {
        let dir = temp_dir("values");
        assert_eq!(read_value(&dir, "x").unwrap(), None);
        write_value(&dir, "x", Some("  Proton 9.0 \n")).unwrap();
        assert_eq!(
            read_value(&dir, "x").unwrap().as_deref(),
            Some("Proton 9.0")
        );
        write_value(&dir, "x", Some("   ")).unwrap();
        assert_eq!(read_value(&dir, "x").unwrap(), None, "blank means not set");
        write_value(&dir, "x", Some("again")).unwrap();
        write_value(&dir, "x", None).unwrap();
        assert_eq!(read_value(&dir, "x").unwrap(), None);
        write_value(&dir, "x", None).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn shim_notices_are_remembered_per_app() {
        let dir = temp_dir("shim-notice");
        assert!(shim_notice_apps(&dir).unwrap().is_empty());
        remember_shim_notice_in(&dir, 5_191_630).unwrap();
        remember_shim_notice_in(&dir, 480).unwrap();
        remember_shim_notice_in(&dir, 480).unwrap();
        let apps = shim_notice_apps(&dir).unwrap();
        assert_eq!(apps.into_iter().collect::<Vec<_>>(), [480, 5_191_630]);
        assert_eq!(
            std::fs::read_to_string(dir.join(SHIM_NOTICE_FILE)).unwrap(),
            "480\n5191630"
        );

        // A broken line does not hide the others and is dropped on write.
        std::fs::write(dir.join(SHIM_NOTICE_FILE), "480\nnot an app\n 70 \n").unwrap();
        let apps = shim_notice_apps(&dir).unwrap();
        assert!(apps.contains(&480) && apps.contains(&70) && !apps.contains(&10));
        remember_shim_notice_in(&dir, 10).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(SHIM_NOTICE_FILE)).unwrap(),
            "10\n70\n480"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn launches_at_the_same_time_lose_no_app_id() {
        let dir = temp_dir("shim-notice-threads");
        std::thread::scope(|scope| {
            for app_id in 1..=16 {
                let dir = &dir;
                scope.spawn(move || remember_shim_notice_in(dir, app_id).unwrap());
            }
        });
        let apps = shim_notice_apps(&dir).unwrap();
        assert_eq!(
            apps.into_iter().collect::<Vec<_>>(),
            (1..=16).collect::<Vec<_>>()
        );
        assert!(!dir.join(format!("{SHIM_NOTICE_FILE}.part")).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn library_folders_are_checked_and_created() {
        assert!(matches!(
            prepare_library(Path::new("relative/library")),
            Err(SettingsError::RelativeLibrary(_))
        ));
        let dir = temp_dir("library");
        let library = dir.join("Games");
        prepare_library(&library).unwrap();
        assert!(library.join("steamapps").is_dir());
        prepare_library(&library).unwrap();

        let file = dir.join("file");
        std::fs::write(&file, b"x").unwrap();
        assert!(matches!(
            prepare_library(&file),
            Err(SettingsError::NotAFolder(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
