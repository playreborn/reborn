//! Filling the run's cloud folder from Valve's cloud when a game starts
//! in shim mode (ADR 0055, "Cloud", "At launch"; extends ADR 0035).
//!
//! For each file in the app's cloud list, with L the SHA-1 of the file in
//! the run's folder ([`RunCloud`]), C the cloud's and B the **base** (the
//! version last downloaded, kept in a small state file next to the run's
//! folder):
//!
//! - L = C: nothing to do;
//! - no local file: downloaded;
//! - L = B, C ≠ B: only the cloud changed; the local file is moved to a
//!   backup folder first, then the cloud version is downloaded;
//! - C = B, L ≠ B: only the local file changed; kept;
//! - otherwise (both changed, or no base and they differ): a **true
//!   conflict**. Without a choice by the user nothing at all is written
//!   and the caller gets the conflicts ([`FillOutcome::Conflicts`]) so a
//!   frontend can ask before the game starts. With a choice, the chosen
//!   version is played with and the other is kept in the backup folder.
//!
//! Nothing is ever uploaded, deleted or overwritten: a file is replaced
//! only after it was moved to the backup folder, and downloads go through
//! [`download_cloud_file_into`] (SHA-1 checked, never over an existing
//! file). The cloud list is fetched once by the caller and handed in;
//! only the files that need it are downloaded (red line 8).
//!
//! **Where things are**, all below the Reborn data folder:
//!
//! - the run's folder: `cloud/<account ID>/<app ID>/` ([`RunCloud`]);
//! - the base file: `cloud/<account ID>/<app ID>.base.json` ("next to it",
//!   ADR 0055), JSON with the SHA-1, size and cloud time of each file's
//!   last downloaded version. The user's save state, not a secret;
//! - backups: `cloud-backup/<account ID>/<app ID>/<unix time>/local/<name>`
//!   for local files moved aside, `.../cloud/<name>` for the cloud version
//!   when the user chose the local one. ADR 0055 says only that the other
//!   version "stays as a backup"; the place is Reborn's choice.
//!
//! **Names** come from Valve and are untrusted (AGENTS.md section 5): only
//! names that pass [`api_name_path`] (plain relative path, no root token,
//! no `..`, no device names) are used. Names with a root token are
//! Auto-Cloud files for the game's own save folders (ADR 0035) and are
//! skipped here. Two names that differ only in case are refused, because
//! they would meet in one file on Windows. No link is followed: the local
//! file is opened as in [`RunCloud::read`], and every folder from the data
//! folder down is checked before anything is created or moved. The same
//! limit as in [`crate::run_cloud`] applies: a folder swapped for a link
//! between the check and the write is not caught (std has no
//! handle-relative open).

use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};

use reborn_cm::CloudFile;
use reborn_content::cdn::CdnClient;
use reborn_content::cloud::{CloudFileRef, CloudOutcome, download_cloud_file_into};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::CoreError;
use crate::run_cloud::{LocalFile, RunCloud, RunCloudError, api_name_path};

/// Suffix of the base file next to the run's folder.
pub const BASE_SUFFIX: &str = ".base.json";

/// Folder below the Reborn data folder that holds backups.
pub const BACKUP_DIR: &str = "cloud-backup";

/// Format version of the base file.
const BASE_VERSION: u32 = 1;

/// The largest base file read; Reborn's own bound against a damaged file.
const MAX_BASE: u64 = 4 * 1024 * 1024;

/// Suffix of a file while it is being written.
const PART_SUFFIX: &str = ".reborn-part";

/// The base of one file: the version last downloaded from the cloud.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseEntry {
    /// SHA-1, 40 lowercase hex digits.
    pub sha1: String,
    /// Size in bytes.
    pub size: u64,
    /// Change time in the cloud, seconds since the Unix epoch.
    pub timestamp: u64,
}

impl BaseEntry {
    fn of(file: &CloudFile) -> Self {
        Self {
            sha1: file.sha1.to_ascii_lowercase(),
            size: u64::from(file.size),
            timestamp: file.timestamp,
        }
    }
}

/// The base file of a run's folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseFile {
    /// Format version.
    pub version: u32,
    /// The base of each file, by its cloud name.
    pub files: BTreeMap<String, BaseEntry>,
}

impl Default for BaseFile {
    fn default() -> Self {
        Self {
            version: BASE_VERSION,
            files: BTreeMap::new(),
        }
    }
}

/// Which version the user plays with after a true conflict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudChoice {
    /// The file in Reborn's folder; the cloud version is kept as a backup.
    Local,
    /// The cloud version; the local file is kept as a backup.
    Cloud,
}

/// What a file needs at launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// L = C.
    Same,
    /// No local file: download.
    Download,
    /// L = B, C ≠ B: back up the local file, download.
    CloudChanged,
    /// C = B, L ≠ B: keep the local file.
    LocalChanged,
    /// Both changed, or no base and L ≠ C.
    Conflict,
}

/// The rule of ADR 0055 for one file. SHA-1s are compared without regard
/// to case.
pub fn decide(local: Option<&str>, cloud: &str, base: Option<&str>) -> Action {
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let Some(local) = local else {
        return Action::Download;
    };
    if same(local, cloud) {
        return Action::Same;
    }
    match base {
        Some(base) if same(local, base) => Action::CloudChanged,
        Some(base) if same(cloud, base) => Action::LocalChanged,
        _ => Action::Conflict,
    }
}

/// Why a cloud file is not handled here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Skip {
    /// The name has a root token: an Auto-Cloud file for the game's own
    /// save folders (ADR 0035), not for the run's folder.
    RootToken,
    /// The name is not a safe relative path.
    UnsafeName,
    /// The file is larger than the run's limit (the app's quota, at most
    /// 16 MiB), so the run would not hand it out.
    TooLarge,
    /// Steam's SHA-1 is not 40 hex digits.
    BadSha,
}

/// One file and what it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// The cloud file.
    pub file: CloudFile,
    /// The local file, if there is one.
    pub local: Option<LocalFile>,
    /// What it needs.
    pub action: Action,
}

/// What filling the run's folder would do. Only read to make it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FillPlan {
    /// The files for the run's folder.
    pub files: Vec<Planned>,
    /// Cloud files not handled here, with the reason.
    pub skipped: Vec<(String, Skip)>,
}

impl FillPlan {
    /// The true conflicts.
    pub fn conflicts(&self) -> Vec<Conflict> {
        self.files
            .iter()
            .filter(|planned| planned.action == Action::Conflict)
            .filter_map(|planned| {
                let local = planned.local.as_ref()?;
                Some(Conflict {
                    name: planned.file.name.clone(),
                    local_size: local.size,
                    local_modified: local.modified,
                    cloud_size: u64::from(planned.file.size),
                    cloud_modified: planned.file.timestamp,
                })
            })
            .collect()
    }
}

/// A true conflict, for asking the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// The file's name in the cloud (and in the run's folder).
    pub name: String,
    /// Size of the local file.
    pub local_size: u64,
    /// Change time of the local file, seconds since the Unix epoch.
    pub local_modified: Option<u64>,
    /// Size of the cloud version.
    pub cloud_size: u64,
    /// Change time in the cloud, seconds since the Unix epoch.
    pub cloud_modified: u64,
}

/// Why the run's folder could not be looked at or filled. Messages carry
/// no file names (they may be logged).
#[derive(Debug, thiserror::Error)]
pub enum FillError {
    /// A local file could not be looked at (a link, a folder where a file
    /// should be, or a file system error).
    #[error("a file in the run's cloud folder cannot be used: {0}")]
    Local(#[from] RunCloudError),
    /// Two cloud names differ only in case.
    #[error("two cloud files differ only in case; refused")]
    DuplicateName,
    /// A folder on the way to the base file or a backup is a link.
    #[error("a folder below the Reborn data folder is a link; not followed")]
    Symlink,
    /// Reading or writing the base file or a backup failed.
    #[error("cloud folder: {0}")]
    Io(String),
}

impl From<io::Error> for FillError {
    fn from(error: io::Error) -> Self {
        FillError::Io(error.to_string())
    }
}

/// What [`fill_run_cloud`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FillOutcome {
    /// True conflicts and no choice: nothing was written. The game must
    /// not start in shim mode until the user chose.
    Conflicts(Vec<Conflict>),
    /// The folder was filled.
    Done(FillReport),
}

/// The result of filling the run's folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FillReport {
    /// Files that were missing locally and were downloaded.
    pub downloaded: usize,
    /// Files only the cloud changed: the local one was backed up, the
    /// cloud version downloaded.
    pub replaced: usize,
    /// Files only changed locally, kept.
    pub kept_local: usize,
    /// Files already the same.
    pub unchanged: usize,
    /// Conflicts resolved with the cloud version (local backed up).
    pub chose_cloud: usize,
    /// Conflicts resolved with the local file (cloud version backed up).
    pub chose_local: usize,
    /// Cloud files not handled here.
    pub skipped: Vec<(String, Skip)>,
    /// Files that failed, with the reason; their local state is as
    /// before (a moved local file is moved back).
    pub failed: Vec<(String, String)>,
    /// The backup folder of this launch, if one was used.
    pub backup: Option<PathBuf>,
}

/// Downloads a cloud file; a trait so tests need no network.
pub trait CloudFetch {
    /// Downloads `file` to `folder`/`relative`: SHA-1 checked against the
    /// listing, never over an existing file, creating folders as needed.
    fn fetch(
        &self,
        folder: &Path,
        relative: &Path,
        file: &CloudFile,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

/// Downloads from Valve with [`download_cloud_file_into`] (ADR 0032:
/// plain GET on the listing's link, SHA-1 checked).
pub struct CdnFetch(CdnClient);

impl CdnFetch {
    /// A client with Reborn's timeouts.
    pub fn new() -> Result<Self, CoreError> {
        CdnClient::new()
            .map(Self)
            .map_err(|error| CoreError::Http(error.to_string()))
    }
}

impl CloudFetch for CdnFetch {
    async fn fetch(&self, folder: &Path, relative: &Path, file: &CloudFile) -> Result<(), String> {
        let Some(url) = &file.download_url else {
            return Err("Steam sent no download link".to_owned());
        };
        let reference = CloudFileRef {
            name: &file.name,
            url: url.expose(),
            size: file.size,
            sha1: &file.sha1,
        };
        match download_cloud_file_into(&self.0, folder, relative, reference).await {
            Ok(CloudOutcome::Written(_) | CloudOutcome::AlreadyThere(_)) => Ok(()),
            Err(error) => Err(error.to_string()),
        }
    }
}

/// The base file's path relative to the data folder.
fn base_relative(cloud: &RunCloud) -> PathBuf {
    let mut name = cloud
        .relative()
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(BASE_SUFFIX);
    cloud.relative().with_file_name(name)
}

/// Reads the base file. A missing file is an empty base. A damaged one is
/// also treated as empty, with a warning: without a base, every file that
/// differs is a conflict the user decides, so nothing can be overwritten
/// because of it.
pub fn load_base(cloud: &RunCloud) -> Result<BaseFile, FillError> {
    let relative = base_relative(cloud);
    check_chain(cloud.data_dir(), &relative)?;
    let path = cloud.data_dir().join(&relative);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(BaseFile::default()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.len() > MAX_BASE {
        warn!("the run's cloud base file is not usable; treated as empty");
        return Ok(BaseFile::default());
    }
    let text = std::fs::read(&path)?;
    match serde_json::from_slice::<BaseFile>(&text) {
        Ok(base) if base.version == BASE_VERSION => Ok(base),
        _ => {
            warn!("the run's cloud base file is damaged or of another version; treated as empty");
            Ok(BaseFile::default())
        }
    }
}

/// Writes the base file: to a part file first, then renamed over the old
/// one (the base is Reborn's own state, not a save).
pub fn save_base(cloud: &RunCloud, base: &BaseFile) -> Result<(), FillError> {
    let relative = base_relative(cloud);
    let path = cloud.data_dir().join(&relative);
    create_dirs(cloud.data_dir(), relative.parent().unwrap_or(Path::new("")))?;
    if std::fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.is_file()) {
        return Err(FillError::Symlink);
    }
    let text = serde_json::to_vec_pretty(base).map_err(|error| FillError::Io(error.to_string()))?;
    let mut part = path.as_os_str().to_owned();
    part.push(PART_SUFFIX);
    let part = PathBuf::from(part);
    std::fs::write(&part, text)?;
    std::fs::rename(&part, &path)?;
    Ok(())
}

/// Plans filling the run's folder: looks at every local file the cloud
/// list names. Only reads.
pub fn plan_fill(
    cloud: &RunCloud,
    files: &[CloudFile],
    base: &BaseFile,
) -> Result<FillPlan, FillError> {
    let mut plan = FillPlan::default();
    let mut seen = HashSet::new();
    for file in files {
        match api_name_path(&file.name) {
            Ok(_) => {}
            Err(_) if crate::cloud::split_root(&file.name).is_some() => {
                plan.skipped.push((file.name.clone(), Skip::RootToken));
                continue;
            }
            Err(_) => {
                plan.skipped.push((file.name.clone(), Skip::UnsafeName));
                continue;
            }
        }
        if !seen.insert(file.name.to_lowercase()) {
            return Err(FillError::DuplicateName);
        }
        if !is_sha1(&file.sha1) {
            plan.skipped.push((file.name.clone(), Skip::BadSha));
            continue;
        }
        if u64::from(file.size) > cloud.limit() {
            plan.skipped.push((file.name.clone(), Skip::TooLarge));
            continue;
        }
        let local = cloud.local_file(&file.name)?;
        let action = decide(
            local.as_ref().map(|local| local.sha1.as_str()),
            &file.sha1,
            base.files.get(&file.name).map(|entry| entry.sha1.as_str()),
        );
        plan.files.push(Planned {
            file: file.clone(),
            local,
            action,
        });
    }
    Ok(plan)
}

/// Fills the run's folder from the cloud list `files` (fetched once by
/// the caller), applying `choice` to true conflicts. `now` (seconds since
/// the Unix epoch) names the backup folder. See the module documentation.
pub async fn fill_run_cloud<F: CloudFetch>(
    cloud: &RunCloud,
    files: &[CloudFile],
    choice: Option<CloudChoice>,
    fetch: &F,
    now: u64,
) -> Result<FillOutcome, FillError> {
    let mut base = load_base(cloud)?;
    let plan = plan_fill(cloud, files, &base)?;
    let conflicts = plan.conflicts();
    if choice.is_none() && !conflicts.is_empty() {
        return Ok(FillOutcome::Conflicts(conflicts));
    }
    // Only used for conflicts, and there are none without a choice.
    let choice = choice.unwrap_or(CloudChoice::Local);
    let mut report = FillReport {
        skipped: plan.skipped,
        ..FillReport::default()
    };
    let mut backup = Backup::new(cloud, now);
    let before = base.clone();
    let run_folder = cloud.relative().to_path_buf();
    for planned in plan.files {
        let name = planned.file.name.clone();
        let entry = BaseEntry::of(&planned.file);
        // Checked in the plan already.
        let Ok(relative) = api_name_path(&name) else {
            continue;
        };
        let result = match (planned.action, choice) {
            (Action::Same, _) => {
                report.unchanged += 1;
                Ok(true)
            }
            (Action::LocalChanged, _) => {
                report.kept_local += 1;
                Ok(false)
            }
            (Action::Download, _) => {
                // It was free when planned; something there now is not
                // overwritten, and the fill stops.
                if !cloud.is_free(&name)? {
                    return Err(FillError::Io(
                        "a cloud file appeared in the run's folder during the launch".to_owned(),
                    ));
                }
                match fetch
                    .fetch(cloud.data_dir(), &run_folder.join(&relative), &planned.file)
                    .await
                {
                    Ok(()) => {
                        report.downloaded += 1;
                        Ok(true)
                    }
                    Err(error) => Err(error),
                }
            }
            (Action::CloudChanged, _) | (Action::Conflict, CloudChoice::Cloud) => {
                match replace(cloud, fetch, &mut backup, &relative, &planned).await {
                    Ok(()) => {
                        if planned.action == Action::Conflict {
                            report.chose_cloud += 1;
                        } else {
                            report.replaced += 1;
                        }
                        Ok(true)
                    }
                    Err(error) => Err(error),
                }
            }
            (Action::Conflict, CloudChoice::Local) => {
                // The local file stays; the cloud version is kept as a
                // backup, and becomes the base, so the next launch sees
                // "only the local file changed" instead of asking again.
                match backup.folder(cloud, "cloud") {
                    Ok(folder) => {
                        let target = folder.join(&relative);
                        match fetch.fetch(cloud.data_dir(), &target, &planned.file).await {
                            Ok(()) => {
                                report.chose_local += 1;
                                Ok(true)
                            }
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error.to_string()),
                }
            }
        };
        match result {
            Ok(true) => {
                base.files.insert(name, entry);
            }
            Ok(false) => {}
            Err(error) => report.failed.push((name, error)),
        }
    }
    if base != before {
        save_base(cloud, &base)?;
    }
    report.backup = backup.used.then(|| cloud.data_dir().join(&backup.relative));
    Ok(FillOutcome::Done(report))
}

/// Moves the local file to the backup folder, then downloads the cloud
/// version in its place; on a failed download the local file is moved
/// back (if its place is still free).
async fn replace<F: CloudFetch>(
    cloud: &RunCloud,
    fetch: &F,
    backup: &mut Backup,
    relative: &Path,
    planned: &Planned,
) -> Result<(), String> {
    let name = &planned.file.name;
    // The file must still be the one planned with.
    let now = cloud.local_file(name).map_err(|error| error.to_string())?;
    let planned_sha = planned.local.as_ref().map(|local| &local.sha1);
    if now.as_ref().map(|local| &local.sha1) != planned_sha {
        return Err("the local file changed during the launch; nothing replaced".to_owned());
    }
    let from = cloud.dir().join(relative);
    let to_folder = backup
        .folder(cloud, "local")
        .map_err(|error| error.to_string())?;
    let to_relative = to_folder.join(relative);
    if let Some(parent) = to_relative.parent() {
        create_dirs(cloud.data_dir(), parent).map_err(|error| error.to_string())?;
    }
    let to = cloud.data_dir().join(&to_relative);
    if std::fs::symlink_metadata(&to).is_ok() {
        return Err("the backup already exists; nothing replaced".to_owned());
    }
    std::fs::rename(&from, &to).map_err(|error| format!("backing up failed: {error}"))?;
    match fetch
        .fetch(
            cloud.data_dir(),
            &cloud.relative().join(relative),
            &planned.file,
        )
        .await
    {
        Ok(()) => Ok(()),
        Err(error) => {
            // Put the local file back, but never over something new.
            if cloud.is_free(name).unwrap_or(false) {
                if let Err(back) = std::fs::rename(&to, &from) {
                    warn!(%back, "moving a backed-up cloud file back failed; it stays in the backup folder");
                }
            } else {
                warn!("a backed-up cloud file stays in the backup folder; its place is taken");
            }
            Err(error)
        }
    }
}

/// The backup folder of one launch, made when first needed.
struct Backup {
    /// Relative to the data folder.
    relative: PathBuf,
    used: bool,
}

impl Backup {
    fn new(cloud: &RunCloud, now: u64) -> Self {
        let account_and_app: PathBuf = cloud.relative().components().skip(1).collect();
        Self {
            relative: Path::new(BACKUP_DIR)
                .join(account_and_app)
                .join(now.to_string()),
            used: false,
        }
    }

    /// The folder for `kind` (`local` or `cloud`) relative to the data
    /// folder; the first call picks a launch folder that does not exist
    /// yet and creates it.
    fn folder(&mut self, cloud: &RunCloud, kind: &str) -> Result<PathBuf, FillError> {
        if !self.used {
            let start = self.relative.clone();
            let mut attempt = 0u32;
            loop {
                check_chain(cloud.data_dir(), &self.relative)?;
                if std::fs::symlink_metadata(cloud.data_dir().join(&self.relative)).is_err() {
                    break;
                }
                attempt += 1;
                if attempt > 100 {
                    return Err(FillError::Io("no free backup folder".to_owned()));
                }
                let mut name = start.as_os_str().to_owned();
                name.push(format!("-{attempt}"));
                self.relative = PathBuf::from(name);
            }
            create_dirs(cloud.data_dir(), &self.relative)?;
            self.used = true;
        }
        let folder = self.relative.join(kind);
        create_dirs(cloud.data_dir(), &folder)?;
        Ok(folder)
    }
}

/// Fails if an existing part of `relative` below `data_dir` is a link (or
/// a junction on Windows: `symlink_metadata` reports name-surrogate
/// reparse points as links).
fn check_chain(data_dir: &Path, relative: &Path) -> Result<(), FillError> {
    let mut path = data_dir.to_path_buf();
    for component in relative.components() {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Err(FillError::Symlink),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

/// Creates `relative` below `data_dir` one part at a time, refusing links
/// on the way.
fn create_dirs(data_dir: &Path, relative: &Path) -> Result<(), FillError> {
    let mut path = data_dir.to_path_buf();
    for component in relative.components() {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => return Err(FillError::Symlink),
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(FillError::Io(
                    "a file is where a folder should be".to_owned(),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                std::fs::create_dir(&path)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn is_sha1(text: &str) -> bool {
    text.len() == 40 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use sha1::{Digest, Sha1};

    use super::*;
    use crate::run_cloud::hex;

    const STEAM_ID: u64 = 0x0110_0001_0000_0000 + 123_456;
    const APP: u32 = 2_143_030;
    const QUOTA: u64 = 1_000_000;
    const NOW: u64 = 1_763_800_000;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-core-run-cloud-fill-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|time| time.as_nanos())
                    .unwrap_or_default()
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

    fn sha(data: &[u8]) -> String {
        hex(&Sha1::digest(data))
    }

    /// A cloud list entry as `cloud_files_from` makes it (no link: the
    /// fake fetch needs none).
    fn listed(name: &str, content: &[u8], timestamp: u64) -> CloudFile {
        CloudFile {
            name: name.to_owned(),
            size: content.len() as u32,
            timestamp,
            sha1: sha(content).to_uppercase(),
            platforms: vec!["All".to_owned()],
            download_url: None,
        }
    }

    /// Serves cloud contents by SHA-1 and records each download; files
    /// named in `fail` fail.
    #[derive(Default)]
    struct FakeCloud {
        contents: HashMap<String, Vec<u8>>,
        fail: HashSet<String>,
        fetched: Mutex<Vec<PathBuf>>,
    }

    impl FakeCloud {
        fn with(files: &[&[u8]]) -> Self {
            Self {
                contents: files
                    .iter()
                    .map(|content| (sha(content), content.to_vec()))
                    .collect(),
                ..Self::default()
            }
        }

        fn fetched(&self) -> Vec<PathBuf> {
            self.fetched.lock().unwrap().clone()
        }
    }

    impl CloudFetch for FakeCloud {
        async fn fetch(
            &self,
            folder: &Path,
            relative: &Path,
            file: &CloudFile,
        ) -> Result<(), String> {
            self.fetched.lock().unwrap().push(relative.to_path_buf());
            if self.fail.contains(&file.name) {
                return Err("HTTP 500".to_owned());
            }
            let content = self
                .contents
                .get(&file.sha1.to_ascii_lowercase())
                .ok_or("not in the fake cloud")?;
            let path = folder.join(relative);
            if path.exists() {
                return Err("exists; not overwritten".to_owned());
            }
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
            Ok(())
        }
    }

    fn run(
        cloud: &RunCloud,
        files: &[CloudFile],
        choice: Option<CloudChoice>,
        fake: &FakeCloud,
    ) -> FillOutcome {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(fill_run_cloud(cloud, files, choice, fake, NOW))
            .unwrap()
    }

    fn done(outcome: FillOutcome) -> FillReport {
        match outcome {
            FillOutcome::Done(report) => report,
            other => panic!("expected Done, got {other:?}"),
        }
    }

    fn setup(name: &str) -> (TempDir, RunCloud) {
        let temp = TempDir::new(name);
        let cloud = RunCloud::new(&temp.0, STEAM_ID, APP, QUOTA);
        (temp, cloud)
    }

    fn write_local(cloud: &RunCloud, name: &str, content: &[u8]) {
        let path = cloud.dir().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    fn read_local(cloud: &RunCloud, name: &str) -> Vec<u8> {
        std::fs::read(cloud.dir().join(name)).unwrap()
    }

    fn set_base(cloud: &RunCloud, name: &str, content: &[u8]) {
        let mut base = load_base(cloud).unwrap();
        base.files.insert(
            name.to_owned(),
            BaseEntry {
                sha1: sha(content),
                size: content.len() as u64,
                timestamp: 1,
            },
        );
        save_base(cloud, &base).unwrap();
    }

    fn base_sha(cloud: &RunCloud, name: &str) -> Option<String> {
        load_base(cloud)
            .unwrap()
            .files
            .get(name)
            .map(|entry| entry.sha1.clone())
    }

    fn backup(temp: &TempDir, kind: &str, name: &str) -> PathBuf {
        temp.0
            .join(BACKUP_DIR)
            .join("123456")
            .join(APP.to_string())
            .join(NOW.to_string())
            .join(kind)
            .join(name)
    }

    #[test]
    fn the_rule_of_adr_0055() {
        let (l, c, b) = ("aa", "cc", "bb");
        assert_eq!(decide(None, c, None), Action::Download);
        assert_eq!(decide(None, c, Some(b)), Action::Download);
        assert_eq!(decide(Some(c), c, None), Action::Same);
        assert_eq!(decide(Some("CC"), c, Some(b)), Action::Same);
        assert_eq!(decide(Some(b), c, Some(b)), Action::CloudChanged);
        assert_eq!(decide(Some(l), b, Some(b)), Action::LocalChanged);
        assert_eq!(decide(Some(l), c, Some(b)), Action::Conflict);
        assert_eq!(decide(Some(l), c, None), Action::Conflict);
    }

    #[test]
    fn missing_files_are_downloaded_and_become_the_base() {
        let (_temp, cloud) = setup("missing");
        let files = [
            listed("MainPlayerData.json", b"{\"level\":3}", 10),
            listed("slots/1.sav", b"slot", 11),
        ];
        let fake = FakeCloud::with(&[b"{\"level\":3}", b"slot"]);
        let report = done(run(&cloud, &files, None, &fake));
        assert_eq!(report.downloaded, 2);
        assert_eq!(read_local(&cloud, "MainPlayerData.json"), b"{\"level\":3}");
        assert_eq!(read_local(&cloud, "slots/1.sav"), b"slot");
        assert_eq!(
            base_sha(&cloud, "slots/1.sav"),
            Some(sha(b"slot")),
            "the base is lowercase"
        );
        assert_eq!(report.backup, None);
        let base = load_base(&cloud).unwrap();
        assert_eq!(base.files["MainPlayerData.json"].timestamp, 10);
        assert_eq!(base.files["MainPlayerData.json"].size, 11);

        // A second launch downloads nothing.
        let again = FakeCloud::with(&[]);
        let report = done(run(&cloud, &files, None, &again));
        assert_eq!(report.unchanged, 2);
        assert!(again.fetched().is_empty());
    }

    #[test]
    fn same_files_need_no_download_and_set_the_base() {
        let (_temp, cloud) = setup("same");
        write_local(&cloud, "MainPlayerData.json", b"same");
        let fake = FakeCloud::with(&[]);
        let report = done(run(
            &cloud,
            &[listed("MainPlayerData.json", b"same", 5)],
            None,
            &fake,
        ));
        assert_eq!(report.unchanged, 1);
        assert!(fake.fetched().is_empty());
        assert_eq!(base_sha(&cloud, "MainPlayerData.json"), Some(sha(b"same")));
    }

    #[test]
    fn only_the_cloud_changed_replaces_with_a_backup() {
        let (temp, cloud) = setup("cloud-changed");
        write_local(&cloud, "MainPlayerData.json", b"old");
        set_base(&cloud, "MainPlayerData.json", b"old");
        let fake = FakeCloud::with(&[b"new"]);
        let report = done(run(
            &cloud,
            &[listed("MainPlayerData.json", b"new", 20)],
            None,
            &fake,
        ));
        assert_eq!(report.replaced, 1);
        assert_eq!(read_local(&cloud, "MainPlayerData.json"), b"new");
        assert_eq!(
            std::fs::read(backup(&temp, "local", "MainPlayerData.json")).unwrap(),
            b"old"
        );
        assert_eq!(base_sha(&cloud, "MainPlayerData.json"), Some(sha(b"new")));
        assert!(report.backup.is_some());
    }

    #[test]
    fn only_the_local_file_changed_is_kept() {
        let (_temp, cloud) = setup("local-changed");
        write_local(&cloud, "MainPlayerData.json", b"played in reborn");
        set_base(&cloud, "MainPlayerData.json", b"old");
        let fake = FakeCloud::with(&[b"old"]);
        let report = done(run(
            &cloud,
            &[listed("MainPlayerData.json", b"old", 20)],
            None,
            &fake,
        ));
        assert_eq!(report.kept_local, 1);
        assert!(fake.fetched().is_empty());
        assert_eq!(
            read_local(&cloud, "MainPlayerData.json"),
            b"played in reborn"
        );
        assert_eq!(base_sha(&cloud, "MainPlayerData.json"), Some(sha(b"old")));
    }

    #[test]
    fn a_true_conflict_writes_nothing_without_a_choice() {
        let (temp, cloud) = setup("conflict");
        write_local(&cloud, "MainPlayerData.json", b"local");
        set_base(&cloud, "MainPlayerData.json", b"old");
        let base_before = load_base(&cloud).unwrap();
        // A missing file next to it is not downloaded either.
        let files = [
            listed("MainPlayerData.json", b"cloud", 30),
            listed("BackUp_MainPlayerData.json", b"cloud", 30),
        ];
        let fake = FakeCloud::with(&[b"cloud"]);
        let outcome = run(&cloud, &files, None, &fake);
        let FillOutcome::Conflicts(conflicts) = outcome else {
            panic!("expected a conflict");
        };
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].name, "MainPlayerData.json");
        assert_eq!(conflicts[0].local_size, 5);
        assert_eq!(conflicts[0].cloud_size, 5);
        assert_eq!(conflicts[0].cloud_modified, 30);
        assert!(fake.fetched().is_empty());
        assert_eq!(read_local(&cloud, "MainPlayerData.json"), b"local");
        assert!(!cloud.dir().join("BackUp_MainPlayerData.json").exists());
        assert_eq!(load_base(&cloud).unwrap(), base_before);
        assert!(!temp.0.join(BACKUP_DIR).exists());
    }

    #[test]
    fn no_base_and_different_files_is_a_conflict() {
        let (_temp, cloud) = setup("no-base");
        write_local(&cloud, "MainPlayerData.json", b"local");
        let fake = FakeCloud::with(&[b"cloud"]);
        let outcome = run(
            &cloud,
            &[listed("MainPlayerData.json", b"cloud", 30)],
            None,
            &fake,
        );
        assert!(matches!(outcome, FillOutcome::Conflicts(conflicts) if conflicts.len() == 1));
    }

    #[test]
    fn choosing_the_cloud_keeps_the_local_file_as_a_backup() {
        let (temp, cloud) = setup("choose-cloud");
        write_local(&cloud, "MainPlayerData.json", b"local");
        set_base(&cloud, "MainPlayerData.json", b"old");
        let fake = FakeCloud::with(&[b"cloud"]);
        let report = done(run(
            &cloud,
            &[listed("MainPlayerData.json", b"cloud", 30)],
            Some(CloudChoice::Cloud),
            &fake,
        ));
        assert_eq!(report.chose_cloud, 1);
        assert_eq!(read_local(&cloud, "MainPlayerData.json"), b"cloud");
        assert_eq!(
            std::fs::read(backup(&temp, "local", "MainPlayerData.json")).unwrap(),
            b"local"
        );
        assert_eq!(base_sha(&cloud, "MainPlayerData.json"), Some(sha(b"cloud")));
    }

    #[test]
    fn choosing_the_local_file_keeps_the_cloud_version_as_a_backup() {
        let (temp, cloud) = setup("choose-local");
        write_local(&cloud, "slots/1.sav", b"local");
        set_base(&cloud, "slots/1.sav", b"old");
        let files = [listed("slots/1.sav", b"cloud", 30)];
        let fake = FakeCloud::with(&[b"cloud"]);
        let report = done(run(&cloud, &files, Some(CloudChoice::Local), &fake));
        assert_eq!(report.chose_local, 1);
        assert_eq!(read_local(&cloud, "slots/1.sav"), b"local");
        assert_eq!(
            std::fs::read(backup(&temp, "cloud", "slots/1.sav")).unwrap(),
            b"cloud"
        );
        // The cloud version is the base now: the next launch keeps the
        // local file without asking again.
        assert_eq!(base_sha(&cloud, "slots/1.sav"), Some(sha(b"cloud")));
        let again = FakeCloud::with(&[]);
        let report = done(run(&cloud, &files, None, &again));
        assert_eq!(report.kept_local, 1);
        assert!(again.fetched().is_empty());
    }

    #[test]
    fn a_failed_download_puts_the_local_file_back() {
        let (temp, cloud) = setup("fail");
        write_local(&cloud, "MainPlayerData.json", b"old");
        set_base(&cloud, "MainPlayerData.json", b"old");
        let mut fake = FakeCloud::with(&[b"new"]);
        fake.fail.insert("MainPlayerData.json".to_owned());
        let report = done(run(
            &cloud,
            &[listed("MainPlayerData.json", b"new", 20)],
            None,
            &fake,
        ));
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.replaced, 0);
        assert_eq!(read_local(&cloud, "MainPlayerData.json"), b"old");
        assert!(!backup(&temp, "local", "MainPlayerData.json").exists());
        assert_eq!(base_sha(&cloud, "MainPlayerData.json"), Some(sha(b"old")));
    }

    #[test]
    fn a_failed_missing_download_is_reported() {
        let (_temp, cloud) = setup("fail-missing");
        let mut fake = FakeCloud::with(&[b"new"]);
        fake.fail.insert("MainPlayerData.json".to_owned());
        let report = done(run(
            &cloud,
            &[listed("MainPlayerData.json", b"new", 20)],
            None,
            &fake,
        ));
        assert_eq!(report.failed.len(), 1);
        assert_eq!(base_sha(&cloud, "MainPlayerData.json"), None);
    }

    #[test]
    fn local_files_not_in_the_cloud_are_left_alone() {
        let (_temp, cloud) = setup("local-only");
        write_local(&cloud, "extra.json", b"mine");
        let fake = FakeCloud::with(&[]);
        let report = done(run(&cloud, &[], None, &fake));
        assert_eq!(report, FillReport::default());
        assert_eq!(read_local(&cloud, "extra.json"), b"mine");
    }

    #[test]
    fn unsafe_root_token_large_and_bad_entries_are_skipped() {
        let (temp, cloud) = setup("skip");
        let mut bad_sha = listed("bad.json", b"x", 1);
        bad_sha.sha1 = "zz".to_owned();
        let big = CloudFile {
            size: (QUOTA + 1) as u32,
            ..listed("big.json", b"x", 1)
        };
        let files = [
            listed("../escape.json", b"x", 1),
            listed("C:\\Windows\\x", b"x", 1),
            listed("%WinAppDataLocalLow%Studio/save.json", b"x", 1),
            bad_sha,
            big,
        ];
        let fake = FakeCloud::with(&[b"x"]);
        let report = done(run(&cloud, &files, None, &fake));
        assert_eq!(
            report.skipped,
            vec![
                ("../escape.json".to_owned(), Skip::UnsafeName),
                ("C:\\Windows\\x".to_owned(), Skip::UnsafeName),
                (
                    "%WinAppDataLocalLow%Studio/save.json".to_owned(),
                    Skip::RootToken
                ),
                ("bad.json".to_owned(), Skip::BadSha),
                ("big.json".to_owned(), Skip::TooLarge),
            ]
        );
        assert!(fake.fetched().is_empty());
        assert!(
            !temp
                .0
                .join("cloud")
                .join("123456")
                .join("escape.json")
                .exists()
        );
    }

    #[test]
    fn names_differing_only_in_case_are_refused() {
        let (_temp, cloud) = setup("case");
        let files = [listed("Save.json", b"a", 1), listed("save.json", b"b", 1)];
        let fake = FakeCloud::with(&[b"a", b"b"]);
        let result = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(fill_run_cloud(&cloud, &files, None, &fake, NOW));
        assert!(matches!(result, Err(FillError::DuplicateName)));
        assert!(fake.fetched().is_empty());
    }

    #[test]
    fn a_link_in_the_run_folder_stops_the_fill() {
        let (temp, cloud) = setup("link");
        let outside = temp.0.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("MainPlayerData.json"), b"secret").unwrap();
        std::fs::create_dir_all(cloud.dir()).unwrap();
        folder_link(&outside, &cloud.dir().join("slots"));
        let fake = FakeCloud::with(&[b"x"]);
        let result = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(fill_run_cloud(
                &cloud,
                &[listed("slots/MainPlayerData.json", b"x", 1)],
                None,
                &fake,
                NOW,
            ));
        assert!(matches!(
            result,
            Err(FillError::Local(RunCloudError::Symlink))
        ));
        assert!(fake.fetched().is_empty());
        assert_eq!(
            std::fs::read(outside.join("MainPlayerData.json")).unwrap(),
            b"secret"
        );
    }

    #[test]
    fn a_link_above_the_backup_folder_is_refused() {
        let (temp, cloud) = setup("backup-link");
        write_local(&cloud, "MainPlayerData.json", b"old");
        set_base(&cloud, "MainPlayerData.json", b"old");
        let outside = temp.0.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        folder_link(&outside, &temp.0.join(BACKUP_DIR));
        let fake = FakeCloud::with(&[b"new"]);
        let report = done(run(
            &cloud,
            &[listed("MainPlayerData.json", b"new", 20)],
            None,
            &fake,
        ));
        assert_eq!(report.failed.len(), 1);
        assert_eq!(read_local(&cloud, "MainPlayerData.json"), b"old");
        assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
    }

    #[test]
    fn a_damaged_base_counts_as_none() {
        let (temp, cloud) = setup("damaged");
        let path = temp
            .0
            .join("cloud")
            .join("123456")
            .join("2143030.base.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(load_base(&cloud).unwrap(), BaseFile::default());
        // So a differing local file is a conflict, never overwritten.
        write_local(&cloud, "MainPlayerData.json", b"local");
        let fake = FakeCloud::with(&[b"cloud"]);
        let outcome = run(
            &cloud,
            &[listed("MainPlayerData.json", b"cloud", 1)],
            None,
            &fake,
        );
        assert!(matches!(outcome, FillOutcome::Conflicts(_)));
        assert_eq!(read_local(&cloud, "MainPlayerData.json"), b"local");
    }

    #[test]
    fn the_base_file_is_next_to_the_run_folder() {
        let (temp, cloud) = setup("base-path");
        save_base(&cloud, &BaseFile::default()).unwrap();
        assert!(
            temp.0
                .join("cloud")
                .join("123456")
                .join("2143030.base.json")
                .is_file()
        );
        assert!(!cloud.dir().exists(), "the run's folder is not touched");
    }

    #[test]
    fn a_second_backup_in_the_same_second_gets_its_own_folder() {
        let (temp, cloud) = setup("backup-twice");
        let mut first = Backup::new(&cloud, NOW);
        first.folder(&cloud, "local").unwrap();
        let mut second = Backup::new(&cloud, NOW);
        second.folder(&cloud, "local").unwrap();
        assert_ne!(first.relative, second.relative);
        assert!(temp.0.join(&second.relative).is_dir());
    }

    /// Makes the folder link `link` point at `target`: a symlink on Unix,
    /// a junction on Windows (`mklink /J`, needs no privilege).
    fn folder_link(target: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        {
            let status = std::process::Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();
            assert!(status.success(), "mklink /J failed");
        }
    }
}
