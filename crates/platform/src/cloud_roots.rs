//! Where the Steam Cloud roots of Auto-Cloud games are on this computer
//! (ADR 0033).
//!
//! Root names and their folders follow Valve's Steam Cloud documentation
//! (Steamworks "Steam Cloud", Auto-Cloud root paths); `LinuxXdgConfigHome`
//! is seen in recorded app info and follows the XDG Base Directory
//! defaults. Roots for other systems, and Windows roots on Linux (where
//! Windows games keep their saves inside a Proton prefix), are not
//! supported yet.

use std::path::{Path, PathBuf};

use directories::{BaseDirs, UserDirs};

/// The system Reborn runs on, as far as Cloud roots are concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudOs {
    /// Windows.
    Windows,
    /// Linux.
    Linux,
    /// Anything else (not supported).
    Other,
}

impl CloudOs {
    /// The running system.
    pub fn current() -> Self {
        if cfg!(windows) {
            CloudOs::Windows
        } else if cfg!(target_os = "linux") {
            CloudOs::Linux
        } else {
            CloudOs::Other
        }
    }

    /// The name used in `rootoverrides` → `os` (`Windows`, `Linux`).
    pub fn steam_name(self) -> Option<&'static str> {
        match self {
            CloudOs::Windows => Some("Windows"),
            CloudOs::Linux => Some("Linux"),
            CloudOs::Other => None,
        }
    }
}

/// The user's folders the roots point into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownDirs {
    /// Home folder.
    pub home: PathBuf,
    /// Documents (on Windows the current location, even if moved).
    pub documents: Option<PathBuf>,
    /// Windows `AppData\Local`; on Linux the XDG data folder.
    pub data_local: PathBuf,
    /// Windows `AppData\Roaming`; on Linux the XDG data folder.
    pub data: PathBuf,
    /// XDG config folder on Linux; `AppData\Roaming` on Windows.
    pub config: PathBuf,
}

impl KnownDirs {
    /// The current user's folders, from the `directories` crate. `None`
    /// if the home folder cannot be found.
    pub fn current() -> Option<Self> {
        let base = BaseDirs::new()?;
        Some(Self {
            home: base.home_dir().to_path_buf(),
            documents: UserDirs::new().and_then(|user| user.document_dir().map(Path::to_path_buf)),
            data_local: base.data_local_dir().to_path_buf(),
            data: base.data_dir().to_path_buf(),
            config: base.config_dir().to_path_buf(),
        })
    }
}

/// Whether a root names a Windows folder (`WinMyDocuments`, ...).
pub fn is_windows_root(root: &str) -> bool {
    root.len() > 3 && root[..3].eq_ignore_ascii_case("win")
}

/// The folder of a root on `os`, or `None` if the root is unknown or not
/// supported there. `game_dir` is the game's install folder, if installed.
pub fn cloud_root_dir(
    root: &str,
    os: CloudOs,
    dirs: &KnownDirs,
    game_dir: Option<&Path>,
) -> Option<PathBuf> {
    let is = |name: &str| root.eq_ignore_ascii_case(name);
    if is("gameinstall") {
        return game_dir.map(Path::to_path_buf);
    }
    match os {
        CloudOs::Windows if is("WinMyDocuments") => dirs.documents.clone(),
        CloudOs::Windows if is("WinAppDataLocal") => Some(dirs.data_local.clone()),
        CloudOs::Windows if is("WinAppDataLocalLow") => {
            Some(dirs.home.join("AppData").join("LocalLow"))
        }
        CloudOs::Windows if is("WinAppDataRoaming") => Some(dirs.data.clone()),
        CloudOs::Windows if is("WinSavedGames") => Some(dirs.home.join("Saved Games")),
        CloudOs::Linux if is("LinuxHome") => Some(dirs.home.clone()),
        CloudOs::Linux if is("LinuxXdgDataHome") => Some(dirs.data.clone()),
        CloudOs::Linux if is("LinuxXdgConfigHome") => Some(dirs.config.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows_dirs() -> KnownDirs {
        let home = PathBuf::from(r"C:\Users\player");
        KnownDirs {
            documents: Some(PathBuf::from(r"D:\Docs")),
            data_local: home.join("AppData").join("Local"),
            data: home.join("AppData").join("Roaming"),
            config: home.join("AppData").join("Roaming"),
            home,
        }
    }

    fn linux_dirs() -> KnownDirs {
        let home = PathBuf::from("/home/player");
        KnownDirs {
            documents: Some(home.join("Documents")),
            data_local: home.join(".local").join("share"),
            data: home.join(".local").join("share"),
            config: home.join(".config"),
            home,
        }
    }

    #[test]
    fn windows_roots() {
        let dirs = windows_dirs();
        let root = |name| cloud_root_dir(name, CloudOs::Windows, &dirs, None);
        assert_eq!(root("WinMyDocuments"), Some(PathBuf::from(r"D:\Docs")));
        assert_eq!(
            root("WinAppDataLocal"),
            Some(dirs.home.join("AppData").join("Local"))
        );
        assert_eq!(
            root("winappdatalocallow"),
            Some(dirs.home.join("AppData").join("LocalLow")),
            "names are case-insensitive"
        );
        assert_eq!(
            root("WinAppDataRoaming"),
            Some(dirs.home.join("AppData").join("Roaming"))
        );
        assert_eq!(root("WinSavedGames"), Some(dirs.home.join("Saved Games")));
        assert_eq!(root("LinuxHome"), None);
        assert_eq!(root("MacHome"), None);
        assert_eq!(root("gameinstall"), None, "not installed");
    }

    #[test]
    fn linux_roots() {
        let dirs = linux_dirs();
        let game = PathBuf::from("/games/steamapps/common/Game");
        let root = |name| cloud_root_dir(name, CloudOs::Linux, &dirs, Some(&game));
        assert_eq!(root("LinuxHome"), Some(dirs.home.clone()));
        assert_eq!(
            root("LinuxXdgDataHome"),
            Some(dirs.home.join(".local").join("share"))
        );
        assert_eq!(root("LinuxXdgConfigHome"), Some(dirs.home.join(".config")));
        assert_eq!(root("gameinstall"), Some(game.clone()));
        assert_eq!(
            root("WinAppDataLocalLow"),
            None,
            "Proton prefix not supported yet"
        );
    }

    #[test]
    fn tells_windows_roots_apart() {
        assert!(is_windows_root("WinAppDataLocalLow"));
        assert!(is_windows_root("winmydocuments"));
        assert!(!is_windows_root("LinuxHome"));
        assert!(!is_windows_root("gameinstall"));
        assert!(!is_windows_root("Win"));
    }
}
