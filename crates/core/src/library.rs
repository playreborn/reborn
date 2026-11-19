//! Apps installed in a library folder (the folder that contains `steamapps`).

use std::path::{Path, PathBuf};

use reborn_formats::{AppManifest, vdf};
use tracing::warn;

use crate::CoreError;
use crate::error::io_error;

/// Marker files of Steam library folders: secondary libraries contain
/// `libraryfolder.vdf`, the main one `steamapps/libraryfolders.vdf`.
pub fn is_steam_library(library: &Path) -> bool {
    [
        library.join("libraryfolder.vdf"),
        library.join("steamapps").join("libraryfolder.vdf"),
        library.join("steamapps").join("libraryfolders.vdf"),
    ]
    .iter()
    .any(|marker| marker.exists())
}

/// Path of an app's appmanifest in a library.
pub fn app_manifest_path(library: &Path, app_id: u32) -> PathBuf {
    library
        .join("steamapps")
        .join(format!("appmanifest_{app_id}.acf"))
}

/// Reads an appmanifest: the raw document (to update it in place) and its
/// typed view. `None` if the file does not exist.
pub fn read_app_manifest(path: &Path) -> Result<Option<(vdf::Object, AppManifest)>, CoreError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(path)(error)),
    };
    let app_manifest_error = |source| CoreError::AppManifest {
        path: path.to_path_buf(),
        source,
    };
    let document = vdf::parse(&text)
        .map_err(reborn_formats::Error::from)
        .map_err(app_manifest_error)?;
    let manifest = AppManifest::from_vdf(&document).map_err(app_manifest_error)?;
    Ok(Some((document, manifest)))
}

/// All apps with an appmanifest in the library, sorted by app ID.
/// Unreadable manifests are skipped with a warning.
pub fn installed_apps(library: &Path) -> Result<Vec<AppManifest>, CoreError> {
    let steamapps = library.join("steamapps");
    let entries = match std::fs::read_dir(&steamapps) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(&steamapps)(error)),
    };
    let mut apps = Vec::new();
    for entry in entries {
        let path = entry.map_err(io_error(&steamapps))?.path();
        let is_manifest = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("appmanifest_") && name.ends_with(".acf"));
        if !is_manifest {
            continue;
        }
        match read_app_manifest(&path) {
            Ok(Some((_, manifest))) => apps.push(manifest),
            Ok(None) => {}
            Err(error) => warn!(%error, "skipping unreadable appmanifest"),
        }
    }
    apps.sort_by_key(|app| app.app_id);
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reborn_formats::{InstalledDepot, NewAppManifest};

    fn temp_library(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "reborn-core-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("steamapps")).unwrap();
        dir
    }

    fn write_manifest(library: &Path, app_id: u32) {
        let manifest = NewAppManifest {
            app_id,
            name: format!("App {app_id}"),
            install_dir: format!("App{app_id}"),
            build_id: 1,
            last_owner: 0,
            size_on_disk: 10,
            download_size: 5,
            last_updated: 0,
            installed_depots: vec![InstalledDepot {
                depot_id: app_id + 1,
                manifest_id: 7,
                size: 10,
            }],
        };
        std::fs::write(app_manifest_path(library, app_id), manifest.to_text()).unwrap();
    }

    #[test]
    fn lists_installed_apps_and_skips_broken_files() {
        let library = temp_library("list");
        write_manifest(&library, 480);
        write_manifest(&library, 10);
        std::fs::write(
            library.join("steamapps").join("appmanifest_1.acf"),
            "\"broken",
        )
        .unwrap();
        std::fs::write(library.join("steamapps").join("libraryfolders.vdf"), "").unwrap();

        let apps = installed_apps(&library).unwrap();
        let ids: Vec<_> = apps.iter().map(|app| app.app_id).collect();
        assert_eq!(ids, [10, 480]);
        assert_eq!(apps[1].installed_depots[0].manifest_id, 7);
        assert!(is_steam_library(&library));

        std::fs::remove_dir_all(&library).unwrap();
    }

    #[test]
    fn missing_library_is_empty_and_missing_manifest_is_none() {
        let library = std::env::temp_dir().join("reborn-core-does-not-exist-library");
        assert!(installed_apps(&library).unwrap().is_empty());
        assert!(
            read_app_manifest(&app_manifest_path(&library, 1))
                .unwrap()
                .is_none()
        );
        assert!(!is_steam_library(&library));
    }
}
