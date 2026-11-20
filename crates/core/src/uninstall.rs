//! Removing an installed app from a library (ADR 0018).
//!
//! What goes: the game folder `steamapps/common/<installdir>`, leftovers of
//! an interrupted download (`steamapps/downloading/<app id>`) and the
//! appmanifest, in that order, so a failed removal leaves the appmanifest
//! behind and can simply be retried. Save games outside the game folder and
//! Proton prefixes stay.
//!
//! The folder name comes from the appmanifest and is checked like every
//! other name from Steam: a single plain path component. `remove_dir_all`
//! does not follow symlinks (Rust standard library documentation), and a
//! game folder that is itself a symlink only loses the link.

use std::path::{Path, PathBuf};

use reborn_content::path::install_path;

use crate::CoreError;
use crate::error::io_error;
use crate::library::{app_manifest_path, read_app_manifest};

/// What was removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UninstallReport {
    /// The app's name from the appmanifest, if any.
    pub name: Option<String>,
    /// The game folder, if it existed.
    pub removed_dir: Option<PathBuf>,
    /// Whether download leftovers were removed.
    pub removed_staging: bool,
}

/// Removes an app from a library. Fails with [`CoreError::NotInstalled`] if
/// the library has no appmanifest for it. Permanent: files are deleted, not
/// moved to a trash folder.
pub fn uninstall_app(library: &Path, app_id: u32) -> Result<UninstallReport, CoreError> {
    let manifest_path = app_manifest_path(library, app_id);
    let (_, manifest) =
        read_app_manifest(&manifest_path)?.ok_or(CoreError::NotInstalled(app_id))?;
    let folder = install_path(&manifest.install_dir)
        .ok()
        .filter(|path| path.components().count() == 1)
        .ok_or_else(|| CoreError::UnsafeInstallDir(manifest.install_dir.clone()))?;
    let steamapps = library.join("steamapps");

    let game_dir = steamapps.join("common").join(folder);
    let removed_dir = remove(&game_dir)?.then_some(game_dir);
    let removed_staging = remove(&steamapps.join("downloading").join(app_id.to_string()))?;
    std::fs::remove_file(&manifest_path).map_err(io_error(&manifest_path))?;

    Ok(UninstallReport {
        name: manifest.name,
        removed_dir,
        removed_staging,
    })
}

/// Removes a directory tree, or only the link if `path` is a symlink.
/// Returns `false` if there was nothing.
fn remove(path: &Path) -> Result<bool, CoreError> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io_error(path)(error)),
    };
    let result = if metadata.is_dir() {
        std::fs::remove_dir_all(path)
    } else if metadata.is_symlink()
        && cfg!(windows)
        && std::fs::metadata(path).is_ok_and(|m| m.is_dir())
    {
        // A directory symlink or junction on Windows is removed like a directory.
        std::fs::remove_dir(path)
    } else {
        std::fs::remove_file(path)
    };
    result.map_err(io_error(path))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempLibrary(PathBuf);

    impl TempLibrary {
        fn new(tag: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("reborn-uninstall-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(path.join("steamapps").join("common")).unwrap();
            Self(path)
        }

        fn manifest(&self, app_id: u32, install_dir: &str) {
            let text = format!(
                "\"AppState\"\n{{\n\t\"appid\"\t\t\"{app_id}\"\n\t\"name\"\t\t\"Test Game\"\n\t\"StateFlags\"\t\t\"4\"\n\t\"installdir\"\t\t\"{install_dir}\"\n}}\n"
            );
            std::fs::write(app_manifest_path(&self.0, app_id), text).unwrap();
        }
    }

    impl Drop for TempLibrary {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn removes_game_folder_leftovers_and_appmanifest() {
        let library = TempLibrary::new("ok");
        library.manifest(480, "Spacewar");
        let game = library.0.join("steamapps").join("common").join("Spacewar");
        std::fs::create_dir_all(game.join("bin")).unwrap();
        std::fs::write(game.join("bin").join("game.exe"), b"MZ").unwrap();
        let staging = library.0.join("steamapps").join("downloading").join("480");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("part"), b"x").unwrap();
        // Another game in the same library must stay.
        let other = library.0.join("steamapps").join("common").join("Other");
        std::fs::create_dir_all(&other).unwrap();

        let report = uninstall_app(&library.0, 480).unwrap();
        assert_eq!(report.name.as_deref(), Some("Test Game"));
        assert_eq!(report.removed_dir.as_deref(), Some(game.as_path()));
        assert!(report.removed_staging);
        assert!(!game.exists() && !staging.exists());
        assert!(!app_manifest_path(&library.0, 480).exists());
        assert!(other.is_dir(), "other games are untouched");
    }

    #[test]
    fn a_missing_game_folder_still_removes_the_appmanifest() {
        let library = TempLibrary::new("nofolder");
        library.manifest(480, "Spacewar");
        let report = uninstall_app(&library.0, 480).unwrap();
        assert_eq!(report.removed_dir, None);
        assert!(!app_manifest_path(&library.0, 480).exists());
    }

    #[test]
    fn refuses_unknown_apps_and_unsafe_folders() {
        let library = TempLibrary::new("unsafe");
        assert!(matches!(
            uninstall_app(&library.0, 480),
            Err(CoreError::NotInstalled(480))
        ));
        for bad in ["..", "", "common/../..", "C:\\Windows"] {
            library.manifest(480, bad);
            assert!(
                matches!(
                    uninstall_app(&library.0, 480),
                    Err(CoreError::UnsafeInstallDir(_))
                ),
                "{bad}"
            );
            assert!(app_manifest_path(&library.0, 480).exists());
        }
        assert!(library.0.join("steamapps").join("common").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_game_folder_only_loses_the_link() {
        let library = TempLibrary::new("link");
        let outside = library.0.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("keep.txt"), b"keep").unwrap();
        let link = library.0.join("steamapps").join("common").join("Linked");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        library.manifest(480, "Linked");

        uninstall_app(&library.0, 480).unwrap();
        assert!(!link.exists());
        assert!(outside.join("keep.txt").is_file(), "the target stays");
    }
}
