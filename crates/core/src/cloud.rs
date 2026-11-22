//! Comparing an app's Steam Cloud files with the files on this computer
//! (ADR 0033), and restoring saves that are missing locally (ADR 0034).
//! Nothing is uploaded, deleted or overwritten.
//!
//! An Auto-Cloud file name is the root token, the rule's path and the
//! file name, e.g.
//! `%WinAppDataLocalLow%Studio312/Coin Maker Simulator Demo/save_slot_demo_1.json`
//! (recorded, `docs/protocol/cloud.md`). Names are untrusted and checked
//! like install paths before they are joined to a folder.

use std::collections::HashSet;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use reborn_cm::{CloudFile, Session};
use reborn_content::cdn::CdnClient;
use reborn_content::cloud::{CloudFileRef, CloudOutcome, download_cloud_file_into};
use reborn_content::path::install_path;
use reborn_formats::{CloudConfig, cloud_config, vdf};
use reborn_platform::cloud_roots::{CloudOs, KnownDirs, cloud_root_dir, is_windows_root};
use reborn_platform::settings;
use sha1::{Digest, Sha1};

use crate::CoreError;
use crate::launch::installed_game_dir;

/// Upper bound on directory entries looked at per rule, so a rule like
/// `gameinstall` + `*` + recursive cannot walk a huge tree for long.
const MAX_ENTRIES: usize = 10_000;

/// Where a cloud file would be on this computer, and how it compares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudFileState {
    /// The name in the cloud.
    pub name: String,
    /// The local path, if the name could be mapped.
    pub local: Option<PathBuf>,
    /// The comparison.
    pub state: CloudState,
}

/// How a cloud file compares with its local counterpart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudState {
    /// The local file has the same content (SHA-1).
    Same,
    /// The local file has other content.
    Differs {
        /// Local change time, seconds since the Unix epoch.
        local_modified: Option<u64>,
        /// Change time in the cloud, seconds since the Unix epoch.
        cloud_modified: u64,
    },
    /// There is no local file.
    MissingLocally,
    /// The name could not be mapped to a local path.
    NotMapped(NotMapped),
}

/// Why a cloud file has no local path (yet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotMapped {
    /// No root token: the game saves through the Steam interface; such
    /// files only get a place with the shim (ADR 0012).
    NoRootToken,
    /// The root is unknown, not supported on this system, or the game is
    /// not installed (`gameinstall`).
    UnknownRoot(String),
    /// The app replaces this root on this system; how `addpath` works is
    /// not known yet (ADR 0033).
    RootOverride(String),
    /// A Windows folder on Linux: inside a Proton prefix, not supported
    /// yet.
    WindowsRootOnLinux(String),
    /// The name could lead outside the root folder or is unsafe.
    UnsafeName,
    /// The local path is a symlink; not followed.
    Symlink,
    /// The local file could not be read.
    Unreadable(String),
}

impl fmt::Display for NotMapped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotMapped::NoRootToken => {
                f.write_str("saved through the Steam interface; needs the steamclient replacement")
            }
            NotMapped::UnknownRoot(root) => {
                write!(
                    f,
                    "folder `{root}` is not supported here (or the game is not installed)"
                )
            }
            NotMapped::RootOverride(root) => {
                write!(
                    f,
                    "the game moves `{root}` on this system; not supported yet"
                )
            }
            NotMapped::WindowsRootOnLinux(root) => {
                write!(
                    f,
                    "Windows folder `{root}` inside a Proton prefix; not supported yet"
                )
            }
            NotMapped::UnsafeName => f.write_str("unsafe file name"),
            NotMapped::Symlink => f.write_str("local path is a symlink; not followed"),
            NotMapped::Unreadable(error) => write!(f, "cannot read the local file: {error}"),
        }
    }
}

/// The comparison for one app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudReport {
    /// Whether the app info has a `ufs` section at all.
    pub has_cloud: bool,
    /// Whether the app has Auto-Cloud rules (`savefiles`).
    pub auto_cloud: bool,
    /// Every file in the cloud and how it compares.
    pub files: Vec<CloudFileState>,
    /// Local files that match a rule but are not in the cloud.
    pub local_only: Vec<PathBuf>,
}

/// Compares an app's cloud files with this computer (ADR 0033): app info
/// for the rules, the file list from Steam, the game's install folder in
/// `library` (default library if `None`) for `gameinstall`. Reads local
/// files; writes nothing.
pub async fn compare_app_cloud(
    session: &Session,
    app_id: u32,
    library: Option<&Path>,
) -> Result<CloudReport, CoreError> {
    let app_info = session.app_info_text(app_id).await?;
    let app = AppCloud::load(session, app_id, library, &app_info).await?;
    let resolve = |root: &str| app.resolve(root);
    let place = app.place(&resolve);
    Ok(CloudReport {
        has_cloud: app.has_cloud,
        auto_cloud: !app.config.save_files.is_empty(),
        files: compare_cloud(&app.files, &place),
        local_only: local_only_files(&place, app.steam_id, &app.files),
    })
}

/// What [`restore_missing_saves`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// Nothing was written because something local conflicts.
    Blocked(RestoreBlocked),
    /// The missing files were handled.
    Done(RestoreReport),
}

/// Why nothing was restored. The user decides how to go on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RestoreBlocked {
    /// These cloud files differ from the local ones.
    Differs(Vec<String>),
    /// These local files match a rule but are not in the cloud: unsynced
    /// progress, or another account's saves.
    LocalOnly(Vec<PathBuf>),
    /// This root folder does not exist; Reborn does not create it.
    RootMissing(PathBuf),
}

/// The result of restoring.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestoreReport {
    /// Files downloaded and written.
    pub restored: Vec<PathBuf>,
    /// Files that were already there with the same content.
    pub unchanged: Vec<PathBuf>,
    /// Cloud files without a local place, with the reason.
    pub not_mapped: Vec<(String, NotMapped)>,
    /// Files that could not be downloaded, with the reason. Nothing was
    /// written for them.
    pub failed: Vec<(String, String)>,
}

/// One file to download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreTask {
    /// The cloud file.
    pub file: CloudFile,
    /// Its root folder (exists).
    pub base: PathBuf,
    /// Its path below the root folder (checked).
    pub relative: PathBuf,
}

/// What restoring would do: download `tasks`, nothing else.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RestorePlan {
    /// Files missing locally.
    pub tasks: Vec<RestoreTask>,
    /// Files already the same locally.
    pub unchanged: Vec<PathBuf>,
    /// Cloud files without a local place.
    pub not_mapped: Vec<(String, NotMapped)>,
}

/// Plans a restore (ADR 0034): blocked if any file differs, if local
/// saves are not in the cloud, or if a root folder is missing; otherwise
/// every file that is missing locally becomes a task. Only reads.
pub fn plan_restore(
    files: &[CloudFile],
    place: &CloudPlace<'_>,
    steam_id: u64,
) -> Result<RestorePlan, RestoreBlocked> {
    let states = compare_cloud(files, place);
    let differing: Vec<String> = states
        .iter()
        .filter(|state| matches!(state.state, CloudState::Differs { .. }))
        .map(|state| state.name.clone())
        .collect();
    if !differing.is_empty() {
        return Err(RestoreBlocked::Differs(differing));
    }
    let local_only = local_only_files(place, steam_id, files);
    if !local_only.is_empty() {
        return Err(RestoreBlocked::LocalOnly(local_only));
    }
    let mut plan = RestorePlan::default();
    for (file, state) in files.iter().zip(states) {
        match state.state {
            CloudState::MissingLocally => {
                let Ok((base, relative)) = local_parts(&file.name, place) else {
                    continue;
                };
                if !base.is_dir() {
                    return Err(RestoreBlocked::RootMissing(base));
                }
                plan.tasks.push(RestoreTask {
                    file: file.clone(),
                    base,
                    relative,
                });
            }
            CloudState::Same => plan.unchanged.extend(state.local),
            CloudState::NotMapped(reason) => plan.not_mapped.push((state.name, reason)),
            CloudState::Differs { .. } => {}
        }
    }
    Ok(plan)
}

/// Brings the app's cloud saves that are missing locally to this computer
/// (ADR 0034). Writes only new files, after checking Steam's SHA-1; stops
/// without writing anything when something local conflicts.
pub async fn restore_missing_saves(
    session: &Session,
    app_id: u32,
    library: Option<&Path>,
) -> Result<RestoreOutcome, CoreError> {
    let app_info = session.app_info_text(app_id).await?;
    restore_missing_saves_with(session, app_id, library, &app_info).await
}

/// [`restore_missing_saves`] with the app info text already fetched
/// ([`Session::app_info_text`]), so a caller that needs the app info for
/// something else too (the shim's cloud quota, ADR 0055) asks Valve for it
/// only once.
pub async fn restore_missing_saves_with(
    session: &Session,
    app_id: u32,
    library: Option<&Path>,
    app_info: &str,
) -> Result<RestoreOutcome, CoreError> {
    let app = AppCloud::load(session, app_id, library, app_info).await?;
    // Planned in its own scope: the folder lookup is not `Send` and must be
    // gone before the downloads are awaited.
    let planned = {
        let resolve = |root: &str| app.resolve(root);
        let place = app.place(&resolve);
        plan_restore(&app.files, &place, app.steam_id)
    };
    let plan = match planned {
        Ok(plan) => plan,
        Err(blocked) => return Ok(RestoreOutcome::Blocked(blocked)),
    };
    let mut report = RestoreReport {
        unchanged: plan.unchanged,
        not_mapped: plan.not_mapped,
        ..RestoreReport::default()
    };
    if plan.tasks.is_empty() {
        return Ok(RestoreOutcome::Done(report));
    }
    let client = CdnClient::new().map_err(|error| CoreError::Http(error.to_string()))?;
    for task in plan.tasks {
        let Some(url) = &task.file.download_url else {
            report
                .failed
                .push((task.file.name, "Steam sent no download link".to_owned()));
            continue;
        };
        let reference = CloudFileRef {
            name: &task.file.name,
            url: url.expose(),
            size: task.file.size,
            sha1: &task.file.sha1,
        };
        match download_cloud_file_into(&client, &task.base, &task.relative, reference).await {
            Ok(CloudOutcome::Written(path)) => report.restored.push(path),
            Ok(CloudOutcome::AlreadyThere(path)) => report.unchanged.push(path),
            Err(error) => report.failed.push((task.file.name, error.to_string())),
        }
    }
    Ok(RestoreOutcome::Done(report))
}

/// Everything about one app's cloud, loaded once.
struct AppCloud {
    has_cloud: bool,
    config: CloudConfig,
    files: Vec<CloudFile>,
    os: CloudOs,
    dirs: Option<KnownDirs>,
    game_dir: Option<PathBuf>,
    steam_id: u64,
}

impl AppCloud {
    async fn load(
        session: &Session,
        app_id: u32,
        library: Option<&Path>,
        app_info: &str,
    ) -> Result<Self, CoreError> {
        let document = vdf::parse(app_info)
            .map_err(|error| reborn_cm::Error::AppInfo(app_id, error.to_string()))?;
        let found = cloud_config(&document);
        let files = session.cloud_files(app_id).await?;
        let library = match library {
            Some(library) => library.to_path_buf(),
            None => settings::default_library()?,
        };
        Ok(Self {
            has_cloud: found.is_some(),
            config: found.unwrap_or_default(),
            files,
            os: CloudOs::current(),
            dirs: KnownDirs::current(),
            game_dir: installed_game_dir(&library, app_id).ok(),
            steam_id: session.steam_id(),
        })
    }

    fn resolve(&self, root: &str) -> Option<PathBuf> {
        self.dirs
            .as_ref()
            .and_then(|dirs| cloud_root_dir(root, self.os, dirs, self.game_dir.as_deref()))
    }

    fn place<'a>(&'a self, resolve: &'a dyn Fn(&str) -> Option<PathBuf>) -> CloudPlace<'a> {
        CloudPlace {
            os: self.os,
            config: &self.config,
            resolve,
        }
    }
}

/// What the comparison needs to know about this computer.
pub struct CloudPlace<'a> {
    /// The running system.
    pub os: CloudOs,
    /// The app's `ufs` settings.
    pub config: &'a CloudConfig,
    /// The folder of a root name, if supported (see
    /// [`reborn_platform::cloud_roots::cloud_root_dir`]).
    pub resolve: &'a dyn Fn(&str) -> Option<PathBuf>,
}

/// The local path for a cloud file name.
pub fn local_path(name: &str, place: &CloudPlace<'_>) -> Result<PathBuf, NotMapped> {
    let (folder, relative) = local_parts(name, place)?;
    Ok(folder.join(relative))
}

/// The root folder and the checked path below it for a cloud file name.
fn local_parts(name: &str, place: &CloudPlace<'_>) -> Result<(PathBuf, PathBuf), NotMapped> {
    let (root, rest) = split_root(name).ok_or(NotMapped::NoRootToken)?;
    let relative = install_path(rest).map_err(|_| NotMapped::UnsafeName)?;
    let folder = root_folder(root, place)?;
    Ok((folder, relative))
}

/// Compares every cloud file with its local counterpart.
pub fn compare_cloud(files: &[CloudFile], place: &CloudPlace<'_>) -> Vec<CloudFileState> {
    files
        .iter()
        .map(|file| match local_path(&file.name, place) {
            Err(reason) => CloudFileState {
                name: file.name.clone(),
                local: None,
                state: CloudState::NotMapped(reason),
            },
            Ok(path) => CloudFileState {
                name: file.name.clone(),
                state: compare_one(&path, file),
                local: Some(path),
            },
        })
        .collect()
}

/// Local files that match a `savefiles` rule for this system but are not
/// in the cloud, sorted. Rules whose root is not supported here are
/// skipped.
pub fn local_only_files(
    place: &CloudPlace<'_>,
    steam_id: u64,
    cloud: &[CloudFile],
) -> Vec<PathBuf> {
    let in_cloud: HashSet<PathBuf> = cloud
        .iter()
        .filter_map(|file| local_path(&file.name, place).ok())
        .collect();
    let mut found = Vec::new();
    for rule in &place.config.save_files {
        if !rule_applies(&rule.platforms, place.os) {
            continue;
        }
        let Ok(folder) = root_folder(&rule.root, place) else {
            continue;
        };
        let Some(start) = rule_folder(&folder, &rule.path, steam_id) else {
            continue;
        };
        collect_matches(&start, &rule.pattern, rule.recursive, place.os, &mut found);
    }
    found.retain(|path| !in_cloud.contains(path));
    found.sort();
    found.dedup();
    found
}

/// Splits `%Root%rest` into the root and the rest (a separator right
/// after the token is dropped).
pub(crate) fn split_root(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix('%')?;
    let end = rest.find('%')?;
    let root = &rest[..end];
    if root.is_empty() {
        return None;
    }
    let after = &rest[end + 1..];
    Some((root, after.strip_prefix(['/', '\\']).unwrap_or(after)))
}

fn root_folder(root: &str, place: &CloudPlace<'_>) -> Result<PathBuf, NotMapped> {
    let overridden = place.config.root_overrides.iter().any(|entry| {
        entry.root.eq_ignore_ascii_case(root)
            && entry.os_compare == "="
            && place
                .os
                .steam_name()
                .is_some_and(|os| entry.os.eq_ignore_ascii_case(os))
    });
    if overridden {
        return Err(NotMapped::RootOverride(root.to_owned()));
    }
    if place.os == CloudOs::Linux && is_windows_root(root) {
        return Err(NotMapped::WindowsRootOnLinux(root.to_owned()));
    }
    (place.resolve)(root).ok_or_else(|| NotMapped::UnknownRoot(root.to_owned()))
}

fn rule_applies(platforms: &[String], os: CloudOs) -> bool {
    platforms.is_empty()
        || platforms.iter().any(|platform| {
            platform.eq_ignore_ascii_case("all")
                || os
                    .steam_name()
                    .is_some_and(|name| platform.eq_ignore_ascii_case(name))
        })
}

/// The folder a rule points at, with the account placeholders filled in;
/// `None` if the path is unsafe.
fn rule_folder(root: &Path, path: &str, steam_id: u64) -> Option<PathBuf> {
    let path = path
        .replace("{64BitSteamID}", &steam_id.to_string())
        .replace("{Steam3AccountID}", &(steam_id & 0xFFFF_FFFF).to_string());
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed == "." {
        return Some(root.to_path_buf());
    }
    install_path(trimmed)
        .ok()
        .map(|relative| root.join(relative))
}

fn collect_matches(
    start: &Path,
    pattern: &str,
    recursive: bool,
    os: CloudOs,
    out: &mut Vec<PathBuf>,
) {
    let mut pending = vec![start.to_path_buf()];
    let mut seen = 0;
    while let Some(folder) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && recursive {
                pending.push(entry.path());
            } else if kind.is_file()
                && wildcard_match(pattern, &entry.file_name().to_string_lossy(), os)
            {
                out.push(entry.path());
            }
        }
    }
}

/// `*` matches any run of characters (Valve's documentation: "file mask
/// with `*` wildcard"). Case-insensitive on Windows.
fn wildcard_match(pattern: &str, name: &str, os: CloudOs) -> bool {
    let (pattern, name) = if os == CloudOs::Windows {
        (pattern.to_lowercase(), name.to_lowercase())
    } else {
        (pattern.to_owned(), name.to_owned())
    };
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || name.len() < first.len() + last.len() || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(index) => rest = &rest[index + part.len()..],
            None => return false,
        }
    }
    true
}

fn compare_one(path: &Path, file: &CloudFile) -> CloudState {
    let metadata = match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return CloudState::MissingLocally;
        }
        Err(error) => return CloudState::NotMapped(NotMapped::Unreadable(error.to_string())),
        Ok(metadata) => metadata,
    };
    if metadata.file_type().is_symlink() {
        return CloudState::NotMapped(NotMapped::Symlink);
    }
    if !metadata.is_file() {
        return CloudState::NotMapped(NotMapped::Unreadable("not a file".to_owned()));
    }
    match sha1_hex(path) {
        Err(error) => CloudState::NotMapped(NotMapped::Unreadable(error.to_string())),
        Ok(local) if local.eq_ignore_ascii_case(&file.sha1) => CloudState::Same,
        Ok(_) => CloudState::Differs {
            local_modified: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs()),
            cloud_modified: file.timestamp,
        },
    }
}

fn sha1_hex(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buffer = vec![0; 1 << 16];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use reborn_formats::{cloud_config, vdf};

    use super::*;

    /// Recorded app info and cloud name of Coin Maker Simulator Demo
    /// (`crates/formats/tests/data/README.md`, `crates/cm/tests/data/README.md`).
    const APP_INFO: &str = include_str!("../../formats/tests/data/appinfo_5191630.vdf");
    const RECORDED_NAME: &str =
        "%WinAppDataLocalLow%Studio312/Coin Maker Simulator Demo/save_slot_demo_1.json";

    /// A made-up SteamID: the individual base plus account 123456.
    const TEST_STEAM_ID: u64 = 0x0110_0001_0000_0000 + 123_456;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-core-cloud-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let dir = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn config() -> CloudConfig {
        cloud_config(&vdf::parse(APP_INFO).unwrap()).unwrap()
    }

    fn cloud_file(name: &str, content: &[u8]) -> CloudFile {
        CloudFile {
            name: name.to_owned(),
            size: u32::try_from(content.len()).unwrap(),
            timestamp: 1_795_213_402,
            sha1: Sha1::digest(content)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            platforms: vec!["All".to_owned()],
            download_url: None,
        }
    }

    fn save_folder(root: &Path) -> PathBuf {
        root.join("Studio312").join("Coin Maker Simulator Demo")
    }

    #[test]
    fn maps_the_recorded_name_on_windows() {
        let dir = TempDir::new("map");
        let config = config();
        let resolve = |root: &str| (root == "WinAppDataLocalLow").then(|| dir.0.clone());
        let place = CloudPlace {
            os: CloudOs::Windows,
            config: &config,
            resolve: &resolve,
        };
        assert_eq!(
            local_path(RECORDED_NAME, &place).unwrap(),
            save_folder(&dir.0).join("save_slot_demo_1.json")
        );
    }

    #[test]
    fn compares_same_differs_and_missing() {
        let dir = TempDir::new("compare");
        let config = config();
        let resolve = |_: &str| Some(dir.0.clone());
        let place = CloudPlace {
            os: CloudOs::Windows,
            config: &config,
            resolve: &resolve,
        };
        let folder = save_folder(&dir.0);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("save_slot_demo_1.json"), b"same").unwrap();
        std::fs::write(folder.join("save_slot_demo_2.json"), b"local").unwrap();
        let files = [
            cloud_file(RECORDED_NAME, b"same"),
            cloud_file(&RECORDED_NAME.replace("_1", "_2"), b"cloud"),
            cloud_file(&RECORDED_NAME.replace("_1", "_3"), b"new"),
        ];
        let states: Vec<CloudState> = compare_cloud(&files, &place)
            .into_iter()
            .map(|state| state.state)
            .collect();
        assert_eq!(states[0], CloudState::Same);
        assert!(matches!(
            states[1],
            CloudState::Differs {
                local_modified: Some(_),
                cloud_modified: 1_795_213_402
            }
        ));
        assert_eq!(states[2], CloudState::MissingLocally);
        // Nothing was written.
        assert!(!folder.join("save_slot_demo_3.json").exists());
        assert_eq!(
            std::fs::read(folder.join("save_slot_demo_2.json")).unwrap(),
            b"local"
        );
    }

    #[test]
    fn reports_what_cannot_be_mapped() {
        let config = config();
        let resolve = |root: &str| (root == "WinAppDataLocalLow").then(|| PathBuf::from("x"));
        let windows = CloudPlace {
            os: CloudOs::Windows,
            config: &config,
            resolve: &resolve,
        };
        assert_eq!(
            local_path("MainPlayerData.json", &windows),
            Err(NotMapped::NoRootToken)
        );
        assert_eq!(
            local_path("%WinAppDataLocalLow%../../evil.dll", &windows),
            Err(NotMapped::UnsafeName)
        );
        assert_eq!(
            local_path("%MacHome%save.json", &windows),
            Err(NotMapped::UnknownRoot("MacHome".to_owned()))
        );
        assert_eq!(
            local_path("%%save.json", &windows),
            Err(NotMapped::NoRootToken)
        );
        let linux = CloudPlace {
            os: CloudOs::Linux,
            config: &config,
            resolve: &resolve,
        };
        // The recorded app moves this root on Linux (`addpath`).
        assert_eq!(
            local_path(RECORDED_NAME, &linux),
            Err(NotMapped::RootOverride("WinAppDataLocalLow".to_owned()))
        );
        let no_overrides = CloudConfig::default();
        let linux = CloudPlace {
            os: CloudOs::Linux,
            config: &no_overrides,
            resolve: &resolve,
        };
        assert_eq!(
            local_path(RECORDED_NAME, &linux),
            Err(NotMapped::WindowsRootOnLinux(
                "WinAppDataLocalLow".to_owned()
            ))
        );
    }

    #[test]
    fn finds_local_saves_missing_in_the_cloud() {
        let dir = TempDir::new("local-only");
        let config = config();
        let resolve = |_: &str| Some(dir.0.clone());
        let place = CloudPlace {
            os: CloudOs::Windows,
            config: &config,
            resolve: &resolve,
        };
        let folder = save_folder(&dir.0);
        std::fs::create_dir_all(folder.join("sub")).unwrap();
        for name in [
            "save_slot_demo_1.json",
            "save_slot_demo_2.json",
            "Player.log",
            "steam_autocloud.vdf",
        ] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        // The rule is not recursive.
        std::fs::write(folder.join("sub").join("save_slot_demo_9.json"), b"x").unwrap();
        let cloud = [cloud_file(RECORDED_NAME, b"x")];
        assert_eq!(
            local_only_files(&place, TEST_STEAM_ID, &cloud),
            vec![folder.join("save_slot_demo_2.json")]
        );
    }

    fn windows_place<'a>(
        config: &'a CloudConfig,
        resolve: &'a dyn Fn(&str) -> Option<PathBuf>,
    ) -> CloudPlace<'a> {
        CloudPlace {
            os: CloudOs::Windows,
            config,
            resolve,
        }
    }

    #[test]
    fn plans_missing_files_into_a_clean_folder() {
        let dir = TempDir::new("plan");
        let config = config();
        let resolve = |_: &str| Some(dir.0.clone());
        let place = windows_place(&config, &resolve);
        let folder = save_folder(&dir.0);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("save_slot_demo_1.json"), b"same").unwrap();
        let files = [
            cloud_file(RECORDED_NAME, b"same"),
            cloud_file(&RECORDED_NAME.replace("_1", "_2"), b"new"),
            cloud_file("MainPlayerData.json", b"other"),
        ];
        let plan = plan_restore(&files, &place, TEST_STEAM_ID).unwrap();
        assert_eq!(plan.tasks.len(), 1);
        assert_eq!(plan.tasks[0].base, dir.0);
        assert_eq!(
            plan.tasks[0].relative,
            Path::new("Studio312")
                .join("Coin Maker Simulator Demo")
                .join("save_slot_demo_2.json")
        );
        assert_eq!(plan.unchanged, vec![folder.join("save_slot_demo_1.json")]);
        assert_eq!(
            plan.not_mapped,
            vec![("MainPlayerData.json".to_owned(), NotMapped::NoRootToken)]
        );
        // Planning writes nothing.
        assert!(!folder.join("save_slot_demo_2.json").exists());
    }

    #[test]
    fn a_differing_file_blocks_everything() {
        let dir = TempDir::new("plan-differs");
        let config = config();
        let resolve = |_: &str| Some(dir.0.clone());
        let place = windows_place(&config, &resolve);
        let folder = save_folder(&dir.0);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("save_slot_demo_1.json"), b"local progress").unwrap();
        let files = [
            cloud_file(RECORDED_NAME, b"cloud progress"),
            cloud_file(&RECORDED_NAME.replace("_1", "_2"), b"new"),
        ];
        assert_eq!(
            plan_restore(&files, &place, TEST_STEAM_ID),
            Err(RestoreBlocked::Differs(vec![RECORDED_NAME.to_owned()]))
        );
    }

    #[test]
    fn local_saves_not_in_the_cloud_block_everything() {
        let dir = TempDir::new("plan-local");
        let config = config();
        let resolve = |_: &str| Some(dir.0.clone());
        let place = windows_place(&config, &resolve);
        let folder = save_folder(&dir.0);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("save_slot_demo_7.json"), b"someone's save").unwrap();
        let files = [cloud_file(RECORDED_NAME, b"cloud")];
        assert_eq!(
            plan_restore(&files, &place, TEST_STEAM_ID),
            Err(RestoreBlocked::LocalOnly(vec![
                folder.join("save_slot_demo_7.json")
            ]))
        );
    }

    #[test]
    fn a_missing_root_folder_is_not_created() {
        let dir = TempDir::new("plan-root");
        let missing = dir.0.join("no LocalLow here");
        let config = config();
        let resolve = |_: &str| Some(missing.clone());
        let place = windows_place(&config, &resolve);
        let files = [cloud_file(RECORDED_NAME, b"cloud")];
        assert_eq!(
            plan_restore(&files, &place, TEST_STEAM_ID),
            Err(RestoreBlocked::RootMissing(missing.clone()))
        );
        assert!(!missing.exists());
    }

    #[test]
    fn fills_in_account_placeholders() {
        let root = Path::new("root");
        assert_eq!(
            rule_folder(root, "Game/{64BitSteamID}/", TEST_STEAM_ID),
            Some(root.join("Game").join(TEST_STEAM_ID.to_string()))
        );
        assert_eq!(
            rule_folder(root, "{Steam3AccountID}", TEST_STEAM_ID),
            Some(root.join("123456"))
        );
        assert_eq!(rule_folder(root, ".", 1), Some(root.to_path_buf()));
        assert_eq!(rule_folder(root, "../evil", 1), None);
    }

    #[test]
    fn matches_wildcards() {
        let windows = CloudOs::Windows;
        let linux = CloudOs::Linux;
        assert!(wildcard_match(
            "save_slot_demo_*.json",
            "save_slot_demo_1.json",
            linux
        ));
        assert!(!wildcard_match(
            "save_slot_demo_*.json",
            "Player.log",
            linux
        ));
        assert!(wildcard_match("*", "anything", linux));
        assert!(wildcard_match("*.sav", ".sav", linux));
        assert!(wildcard_match("a*b*c", "aXbYc", linux));
        assert!(!wildcard_match("a*b*c", "aXc", linux));
        assert!(!wildcard_match("ab*ba", "aba", linux), "no overlap");
        assert!(wildcard_match("*.SAV", "slot.sav", windows));
        assert!(!wildcard_match("*.SAV", "slot.sav", linux));
        assert!(wildcard_match("exact.dat", "exact.dat", linux));
    }
}
