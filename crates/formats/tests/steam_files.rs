//! Tests against Steam-style sample files in `tests/data/`.
//!
//! The samples follow the exact layout Steam writes, but every value that
//! could identify a user (content IDs, Steam IDs, paths) is synthetic.

use reborn_formats::{
    AppManifest, Error, InstalledDepot, LibraryApp, LibraryFolders, NewAppManifest, vdf,
};

const LIBRARY_FOLDERS: &str = include_str!("data/libraryfolders.vdf");
const LIBRARY_FOLDERS_LEGACY: &str = include_str!("data/libraryfolders_legacy.vdf");
const APP_MANIFEST: &str = include_str!("data/appmanifest_228980.acf");

#[test]
fn writer_reproduces_steam_files_byte_for_byte() {
    for sample in [LIBRARY_FOLDERS, LIBRARY_FOLDERS_LEGACY, APP_MANIFEST] {
        let document = vdf::parse(sample).unwrap();
        assert_eq!(vdf::write(&document), sample);
    }
}

#[test]
fn reads_current_library_folders() {
    let libraries = LibraryFolders::parse(LIBRARY_FOLDERS).unwrap();
    assert_eq!(libraries.folders.len(), 2);

    let main = &libraries.folders[0];
    assert_eq!(main.index, 0);
    assert_eq!(main.path, r"C:\Program Files (x86)\Steam");
    assert_eq!(main.label, None);
    assert_eq!(
        main.apps,
        [
            LibraryApp {
                app_id: 228980,
                size_on_disk: Some(1_234_567),
            },
            LibraryApp {
                app_id: 730,
                size_on_disk: Some(40_000_000_000),
            },
        ]
    );

    let second = &libraries.folders[1];
    assert_eq!(second.index, 1);
    assert_eq!(second.path, r"D:\SteamLibrary");
    assert_eq!(second.label.as_deref(), Some("Games"));
    assert!(second.apps.is_empty());
}

#[test]
fn reads_legacy_library_folders() {
    let libraries = LibraryFolders::parse(LIBRARY_FOLDERS_LEGACY).unwrap();
    let paths: Vec<_> = libraries
        .folders
        .iter()
        .map(|f| (f.index, f.path.as_str()))
        .collect();
    assert_eq!(paths, [(1, r"D:\SteamLibrary"), (2, r"E:\Games\Steam")]);
    assert!(libraries.folders.iter().all(|f| f.apps.is_empty()));
}

#[test]
fn reads_app_manifest() {
    let manifest = AppManifest::parse(APP_MANIFEST).unwrap();
    assert_eq!(
        manifest,
        AppManifest {
            app_id: 228980,
            name: Some("Steamworks Common Redistributables".to_owned()),
            install_dir: "Steamworks Shared".to_owned(),
            state_flags: 4,
            size_on_disk: Some(1_234_567),
            build_id: Some(12_345_678),
            last_updated: Some(1_700_000_000),
            last_played: Some(0),
            installed_depots: vec![InstalledDepot {
                depot_id: 228983,
                manifest_id: 1_000_000_000_000_000_003,
                size: 1_234_567,
            }],
        }
    );
    assert!(manifest.is_fully_installed());
}

#[test]
fn updating_a_manifest_keeps_unknown_fields() {
    let mut document = vdf::parse(APP_MANIFEST).unwrap();
    let update = NewAppManifest {
        app_id: 228980,
        name: "Steamworks Common Redistributables".to_owned(),
        install_dir: "Steamworks Shared".to_owned(),
        build_id: 99,
        last_owner: 1,
        size_on_disk: 5,
        download_size: 3,
        last_updated: 1_800_000_000,
        installed_depots: vec![InstalledDepot {
            depot_id: 228984,
            manifest_id: 42,
            size: 5,
        }],
    };
    update.apply_to(&mut document);

    let state = document.get_object("AppState").unwrap();
    // Untouched fields stay, in their original position.
    assert!(
        state
            .get_str("LauncherPath")
            .unwrap()
            .ends_with("steam.exe")
    );
    let keys: Vec<_> = state.iter().map(|(key, _)| key).collect();
    assert_eq!(keys[..4], ["appid", "universe", "LauncherPath", "name"]);
    assert_eq!(
        state.get_object("UserConfig").unwrap().get_str("language"),
        Some("english")
    );
    // Updated fields.
    assert_eq!(state.get_str("buildid"), Some("99"));
    assert_eq!(state.get_str("TargetBuildID"), Some("99"));
    // Depot entries: the existing one is kept, the installed one added.
    let manifest = AppManifest::from_vdf(&document).unwrap();
    let depots: Vec<_> = manifest
        .installed_depots
        .iter()
        .map(|depot| (depot.depot_id, depot.manifest_id))
        .collect();
    assert_eq!(depots, [(228983, 1_000_000_000_000_000_003), (228984, 42)]);
}

#[test]
fn app_manifest_survives_a_rewrite() {
    let mut document = vdf::parse(APP_MANIFEST).unwrap();
    let mut state = document.get_object("AppState").unwrap().clone();
    state.insert("StateFlags", "6");
    document.insert("AppState", state);

    let rewritten = vdf::write(&document);
    let manifest = AppManifest::parse(&rewritten).unwrap();
    assert_eq!(manifest.state_flags, 6);
    assert_eq!(rewritten.lines().count(), APP_MANIFEST.lines().count());
}

#[test]
fn missing_required_keys_name_the_full_path() {
    let err = AppManifest::parse("\"AppState\" { \"installdir\" \"x\" }").unwrap_err();
    assert_eq!(err, Error::MissingKey("AppState/appid".to_owned()));

    let err = LibraryFolders::parse("\"libraryfolders\" { \"0\" { \"label\" \"\" } }").unwrap_err();
    assert_eq!(err, Error::MissingKey("libraryfolders/0/path".to_owned()));

    let err = AppManifest::parse("\"Other\" {}").unwrap_err();
    assert_eq!(err, Error::MissingKey("AppState".to_owned()));
}

#[test]
fn invalid_numbers_are_reported() {
    let err =
        AppManifest::parse("\"AppState\" { \"appid\" \"abc\" \"installdir\" \"x\" }").unwrap_err();
    assert_eq!(
        err,
        Error::InvalidNumber {
            key: "AppState/appid".to_owned(),
            value: "abc".to_owned(),
        }
    );

    let err = LibraryFolders::parse(
        "\"libraryfolders\" { \"0\" { \"path\" \"p\" \"apps\" { \"x1\" \"5\" } } }",
    )
    .unwrap_err();
    assert!(matches!(err, Error::InvalidNumber { key, .. } if key == "libraryfolders/0/apps/x1"));
}

#[test]
fn wrong_types_are_reported() {
    let err = AppManifest::parse("\"AppState\" \"not an object\"").unwrap_err();
    assert!(matches!(err, Error::WrongType { key, .. } if key == "AppState"));

    let err =
        AppManifest::parse("\"AppState\" { \"appid\" { } \"installdir\" \"x\" }").unwrap_err();
    assert!(matches!(err, Error::WrongType { key, .. } if key == "AppState/appid"));
}

#[test]
fn syntax_errors_are_passed_through() {
    let err = AppManifest::parse("\"AppState\" {").unwrap_err();
    assert!(matches!(err, Error::Vdf(_)));
}
