//! The run's cloud folder (ADR 0055): where the files a game reads (and
//! later writes) through the Steam interface live while it runs on the
//! shim, `<Reborn data>/cloud/<account ID>/<app ID>/`, separate per
//! account.
//!
//! This module only **reads**: the size of a file and its first bytes.
//! Nothing here writes, creates or deletes anything, and nothing here
//! talks to Valve: the quota comes from app info the caller already
//! fetched ([`quota_from_app_info`]).
//!
//! File names come from the game and are untrusted (AGENTS.md section 5).
//! A name is used only if it is a plain relative path
//! ([`reborn_content::path::install_path`]: no `..`, no absolute path, no
//! drive letters, streams, unsafe characters or device names) and has no
//! Auto-Cloud root token (`%WinAppDataLocalLow%...`,
//! `docs/protocol/cloud.md`). No symlink (on Windows also no junction or
//! other name-surrogate reparse point) is followed, neither in the folder
//! above the run's folder (below the Reborn data folder) nor inside it:
//! every part of the path is looked at without following it first, and
//! the last part is then opened without following a link
//! ([`RunCloud::read`]).
//!
//! **Limits of these checks.** The threat is another process of the same
//! user swapping things in the folder while Reborn reads (anything with
//! more rights could read the files directly anyway). A link swapped into
//! the last part after the check is caught when the file is opened. A
//! *folder* on the way swapped for a link between the check and the open
//! is not caught: std has no handle-relative open (`openat`, or
//! `NtCreateFile` with a root directory). The launch side
//! ([`crate::run_cloud_fill`]) writes with the same limit: it checks every
//! part of the path for links right before creating, moving or
//! downloading, and accepts that a folder swapped in between is not
//! caught (the threat is a process of the same user, which could change
//! the saves directly anyway). Writes by the game during a run are a
//! separate stage and decide this again. Hard links are not
//! detected: a hard link in the folder is a regular file, and its content
//! is handed out like any other (making one needs write access to the
//! folder and, on Windows, to the target).
//!
//! Filling the folder at launch from Valve's cloud, with the base file
//! (SHA-1, size and time of the version last downloaded), is
//! [`crate::run_cloud_fill`]. Writes by the game are not here yet.

use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use reborn_content::path::install_path;
use reborn_formats::{CloudConfig, cloud_config, vdf};
use sha1::{Digest, Sha1};

use crate::CoreError;
use crate::cloud::split_root;

/// The largest file the run's cloud folder hands out or (later) takes,
/// whatever the app's quota says (ADR 0055: "at most 16 MiB").
pub const MAX_FILE: u64 = 16 * 1024 * 1024;

/// The longest file name accepted, in bytes. Reborn's own bound against
/// absurd input, not a rule recorded from the official client.
pub const MAX_NAME: usize = 1024;

/// The account ID of a SteamID64: its low 32 bits
/// (`docs/protocol/friends.md`, SteamID64 of an individual account).
pub fn account_id(steam_id: u64) -> u32 {
    (steam_id & 0xFFFF_FFFF) as u32
}

/// The size limit for the run's cloud files from the app's `ufs`
/// settings: its `quota`, at most [`MAX_FILE`] (ADR 0055). `None` when the
/// app has no `ufs` section or no quota: then it has no cloud, and the run
/// gets no cloud folder.
pub fn limit_from(config: Option<&CloudConfig>) -> Option<u64> {
    config?.quota.map(|quota| quota.min(MAX_FILE))
}

/// The app's cloud size limit ([`limit_from`]) from its app info text
/// ([`reborn_cm::Session::app_info_text`], already fetched; no request).
pub fn quota_from_app_info(app_id: u32, app_info: &str) -> Result<Option<u64>, CoreError> {
    let document = vdf::parse(app_info)
        .map_err(|error| reborn_cm::Error::AppInfo(app_id, error.to_string()))?;
    Ok(limit_from(cloud_config(&document).as_ref()))
}

/// Why a read in the run's cloud folder was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RunCloudError {
    /// The name is not a plain relative path, has a root token, or is too
    /// long.
    #[error("unsafe cloud file name")]
    UnsafeName,
    /// The path leads through or to a symlink; not followed.
    #[error("the cloud file's path contains a symlink; not followed")]
    Symlink,
    /// The name names something that is not a regular file (a folder).
    #[error("the cloud file name names a folder")]
    NotAFile,
    /// The file is larger than the run's limit.
    #[error("the cloud file is larger than the app's quota")]
    TooLarge,
    /// The file system failed.
    #[error("reading the cloud file failed: {0}")]
    Io(String),
}

impl From<io::Error> for RunCloudError {
    fn from(error: io::Error) -> Self {
        RunCloudError::Io(error.to_string())
    }
}

/// One run's cloud folder and its size limit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunCloud {
    /// The Reborn data folder (trusted; the checks start below it).
    data_dir: PathBuf,
    /// The run's folder below it: `cloud/<account ID>/<app ID>`.
    relative: PathBuf,
    /// The largest file handed out.
    limit: u64,
}

impl RunCloud {
    /// The cloud folder of `app_id` for the account `steam_id` below the
    /// Reborn data folder `data_dir`, with the size limit `limit` (at most
    /// [`MAX_FILE`]). Creates nothing.
    pub fn new(data_dir: &Path, steam_id: u64, app_id: u32, limit: u64) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            relative: Path::new("cloud")
                .join(account_id(steam_id).to_string())
                .join(app_id.to_string()),
            limit: limit.min(MAX_FILE),
        }
    }

    /// The run's folder.
    pub fn dir(&self) -> PathBuf {
        self.data_dir.join(&self.relative)
    }

    /// The largest file handed out, in bytes.
    pub fn limit(&self) -> u64 {
        self.limit
    }

    /// The size of the file `name`, `None` if there is none. A file above
    /// the limit is refused ([`RunCloudError::TooLarge`]): it is never
    /// handed out, so its size is not either.
    pub fn file_size(&self, name: &str) -> Result<Option<u64>, RunCloudError> {
        let Some(metadata) = self.find(name)? else {
            return Ok(None);
        };
        self.check(&metadata)?;
        Ok(Some(metadata.len()))
    }

    /// At most `max` bytes from the start of the file `name`, `None` if
    /// there is none. A file above the limit is refused. The file is
    /// opened without following a link in its last part
    /// (`open_no_follow`); see the module documentation for what is not
    /// caught.
    pub fn read(&self, name: &str, max: u64) -> Result<Option<Vec<u8>>, RunCloudError> {
        let Some(found) = self.find(name)? else {
            return Ok(None);
        };
        self.check(&found)?;
        let path = self.path_of(name)?;
        let Some(file) = open_no_follow(&path, &found)? else {
            return Ok(None);
        };
        // Checked again on the open file: it may have changed since.
        let metadata = file.metadata()?;
        self.check(&metadata)?;
        let wanted = metadata.len().min(max).min(self.limit);
        let mut data = Vec::with_capacity(usize::try_from(wanted).unwrap_or(0));
        file.take(wanted).read_to_end(&mut data)?;
        Ok(Some(data))
    }

    /// The Reborn data folder the run's folder lies below.
    pub(crate) fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The run's folder relative to the data folder:
    /// `cloud/<account ID>/<app ID>`.
    pub(crate) fn relative(&self) -> &Path {
        &self.relative
    }

    /// SHA-1 (40 lowercase hex digits), size and change time of the local
    /// file `name`, `None` if there is none. Hashes the whole file,
    /// whatever its size (it is only read here, never handed out); opened
    /// like [`RunCloud::read`], without following links.
    pub(crate) fn local_file(&self, name: &str) -> Result<Option<LocalFile>, RunCloudError> {
        let Some(found) = self.find(name)? else {
            return Ok(None);
        };
        if !found.is_file() {
            return Err(RunCloudError::NotAFile);
        }
        let path = self.path_of(name)?;
        let Some(mut file) = open_no_follow(&path, &found)? else {
            return Ok(None);
        };
        let metadata = file.metadata()?;
        let mut hasher = Sha1::new();
        let mut buffer = vec![0u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            size += read as u64;
        }
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|time| time.as_secs());
        Ok(Some(LocalFile {
            sha1: hex(&hasher.finalize()),
            size,
            modified,
        }))
    }

    /// Whether nothing at all (no file, folder or link) is at `name`; a
    /// link on the way is an error.
    pub(crate) fn is_free(&self, name: &str) -> Result<bool, RunCloudError> {
        Ok(self.find(name)?.is_none())
    }

    fn check(&self, metadata: &Metadata) -> Result<(), RunCloudError> {
        if !metadata.is_file() {
            return Err(RunCloudError::NotAFile);
        }
        if metadata.len() > self.limit {
            return Err(RunCloudError::TooLarge);
        }
        Ok(())
    }

    /// The checked path of `name` inside the run's folder.
    fn path_of(&self, name: &str) -> Result<PathBuf, RunCloudError> {
        Ok(self.dir().join(api_name_path(name)?))
    }

    /// The metadata of `name` without following any symlink, `None` if it
    /// or a folder on the way does not exist (or a folder on the way is a
    /// file).
    fn find(&self, name: &str) -> Result<Option<Metadata>, RunCloudError> {
        let relative = self.relative.join(api_name_path(name)?);
        let mut path = self.data_dir.clone();
        let mut components = relative.components().peekable();
        while let Some(component) = components.next() {
            path.push(component);
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            if metadata.file_type().is_symlink() {
                return Err(RunCloudError::Symlink);
            }
            if components.peek().is_none() {
                return Ok(Some(metadata));
            }
            if !metadata.is_dir() {
                return Ok(None);
            }
        }
        Ok(None)
    }
}

/// A file in the run's folder as found on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalFile {
    /// SHA-1, 40 lowercase hex digits.
    pub sha1: String,
    /// Size in bytes.
    pub size: u64,
    /// Change time, seconds since the Unix epoch, if the system has one.
    pub modified: Option<u64>,
}

/// Lowercase hex digits of `bytes`.
pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Opens the file at `path` for reading without following a link in its
/// last part, and refuses it unless it is still the file `found` (its
/// metadata from [`RunCloud::find`], taken without following links).
/// `None` if it is gone.
///
/// - Windows: opened with `FILE_FLAG_OPEN_REPARSE_POINT` (0x00200000), so
///   a symlink or junction is opened itself instead of its target, and
///   refused if the opened handle has `FILE_ATTRIBUTE_REPARSE_POINT`
///   (0x400). Both values from Microsoft's documentation of `CreateFileW`
///   (<https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew>)
///   and of the file attribute constants
///   (<https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants>).
/// - Unix: std has no `O_NOFOLLOW` without the `libc` crate, so the opened
///   file's device and inode must equal those of `found`; a link swapped
///   in opens its target, whose inode differs.
fn open_no_follow(path: &Path, found: &Metadata) -> Result<Option<File>, RunCloudError> {
    let opened = open_last_part(path);
    let file = match opened {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !same_file(&file.metadata()?, found) {
        return Err(RunCloudError::Symlink);
    }
    Ok(Some(file))
}

#[cfg(windows)]
fn open_last_part(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_last_part(path: &Path) -> io::Result<File> {
    File::open(path)
}

/// Whether the opened file is no link and still the file found before.
#[cfg(windows)]
fn same_file(opened: &Metadata, _found: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    opened.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0 && !opened.file_type().is_symlink()
}

/// Whether the opened file is still the file found before.
#[cfg(unix)]
fn same_file(opened: &Metadata, found: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    (opened.dev(), opened.ino()) == (found.dev(), found.ino())
}

/// Other systems: no way to tell with std; only the type is compared.
#[cfg(not(any(unix, windows)))]
fn same_file(opened: &Metadata, found: &Metadata) -> bool {
    opened.file_type() == found.file_type() && !opened.file_type().is_symlink()
}

/// The suffix `download_cloud_file_into` gives a file while writing it
/// (`crates/content/src/cloud.rs`, `PART_SUFFIX`).
pub const PART_SUFFIX: &str = ".reborn-part";

/// A game's cloud file name as a checked relative path: a plain relative
/// path ([`install_path`]), no root token, at most [`MAX_NAME`] bytes, and
/// a last part that does not end in [`PART_SUFFIX`] (any case): such a
/// file would be truncated when the file without the suffix is
/// downloaded.
pub fn api_name_path(name: &str) -> Result<PathBuf, RunCloudError> {
    if name.len() > MAX_NAME || split_root(name).is_some() {
        return Err(RunCloudError::UnsafeName);
    }
    let path = install_path(name).map_err(|_| RunCloudError::UnsafeName)?;
    let part = path.file_name().is_some_and(|last| {
        last.to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(PART_SUFFIX)
    });
    if part {
        return Err(RunCloudError::UnsafeName);
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up SteamID: the individual base plus account 123456.
    const STEAM_ID: u64 = 0x0110_0001_0000_0000 + 123_456;
    /// Cave Crawlers, whose recorded cloud file names have no root token
    /// (`docs/protocol/cloud.md`, "Auto-Cloud paths"); its quota is
    /// 1 000 000.
    const APP: u32 = 2_143_030;
    const QUOTA: u64 = 1_000_000;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-core-run-cloud-{name}-{}-{}",
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

    /// A run folder with `MainPlayerData.json` and `slots/1.sav` in it.
    fn with_files(name: &str) -> (TempDir, RunCloud) {
        let temp = TempDir::new(name);
        let cloud = RunCloud::new(&temp.0, STEAM_ID, APP, QUOTA);
        std::fs::create_dir_all(cloud.dir().join("slots")).unwrap();
        std::fs::write(cloud.dir().join("MainPlayerData.json"), b"{\"level\":3}").unwrap();
        std::fs::write(cloud.dir().join("slots").join("1.sav"), b"slot").unwrap();
        (temp, cloud)
    }

    #[test]
    fn the_folder_is_per_account_and_app() {
        let cloud = RunCloud::new(Path::new("data"), STEAM_ID, APP, QUOTA);
        assert_eq!(
            cloud.dir(),
            Path::new("data")
                .join("cloud")
                .join("123456")
                .join("2143030")
        );
        assert_eq!(account_id(STEAM_ID), 123_456);
    }

    #[test]
    fn the_limit_is_the_quota_at_most_16_mib() {
        let config = |quota| CloudConfig {
            quota,
            ..CloudConfig::default()
        };
        assert_eq!(limit_from(Some(&config(Some(QUOTA)))), Some(QUOTA));
        assert_eq!(limit_from(Some(&config(Some(u64::MAX)))), Some(MAX_FILE));
        assert_eq!(limit_from(Some(&config(None))), None);
        assert_eq!(limit_from(None), None);
        assert_eq!(
            RunCloud::new(Path::new("d"), STEAM_ID, APP, u64::MAX).limit(),
            MAX_FILE
        );
    }

    #[test]
    fn the_recorded_quota_comes_from_the_app_info() {
        // Recorded app info of app 2143030 (`ufs`: quota 1000000).
        let text = include_str!("../../formats/tests/data/appinfo_2143030.vdf");
        assert!(matches!(quota_from_app_info(APP, text), Ok(Some(QUOTA))));
        assert!(quota_from_app_info(APP, "\"broken").is_err());
    }

    #[test]
    fn reads_sizes_and_bytes() {
        let (_temp, cloud) = with_files("read");
        assert_eq!(cloud.file_size("MainPlayerData.json"), Ok(Some(11)));
        assert_eq!(cloud.file_size("slots/1.sav"), Ok(Some(4)));
        assert_eq!(cloud.file_size("slots\\1.sav"), Ok(Some(4)));
        assert_eq!(
            cloud.read("MainPlayerData.json", 4096),
            Ok(Some(b"{\"level\":3}".to_vec()))
        );
        // At most the game's buffer.
        assert_eq!(
            cloud.read("MainPlayerData.json", 3),
            Ok(Some(b"{\"l".to_vec()))
        );
        assert_eq!(cloud.read("MainPlayerData.json", 0), Ok(Some(Vec::new())));
    }

    #[test]
    fn missing_files_are_none() {
        let (_temp, cloud) = with_files("missing");
        assert_eq!(cloud.file_size("other.json"), Ok(None));
        assert_eq!(cloud.read("other.json", 10), Ok(None));
        assert_eq!(cloud.file_size("nothing/1.sav"), Ok(None));
        // A file on the way is not a folder.
        assert_eq!(cloud.file_size("MainPlayerData.json/x"), Ok(None));
        // No folder at all.
        let temp = TempDir::new("empty");
        let empty = RunCloud::new(&temp.0, STEAM_ID, APP, QUOTA);
        assert_eq!(empty.file_size("MainPlayerData.json"), Ok(None));
        assert_eq!(empty.read("MainPlayerData.json", 10), Ok(None));
        assert!(!empty.dir().exists(), "reading created nothing");
    }

    #[test]
    fn other_accounts_and_apps_are_apart() {
        let (temp, _cloud) = with_files("apart");
        let other_account = RunCloud::new(&temp.0, STEAM_ID + 1, APP, QUOTA);
        assert_eq!(other_account.file_size("MainPlayerData.json"), Ok(None));
        let other_app = RunCloud::new(&temp.0, STEAM_ID, APP + 1, QUOTA);
        assert_eq!(other_app.read("MainPlayerData.json", 10), Ok(None));
    }

    #[test]
    fn unsafe_names_are_refused() {
        let (temp, cloud) = with_files("unsafe");
        // A file next to the run's folder that a traversal would reach.
        std::fs::write(temp.0.join("cloud").join("secret"), b"x").unwrap();
        let long = "a".repeat(MAX_NAME + 1);
        for name in [
            "",
            "../secret",
            "..\\..\\secret",
            "slots/../MainPlayerData.json",
            "./MainPlayerData.json",
            "/etc/passwd",
            "\\Windows\\win.ini",
            "C:\\Windows\\win.ini",
            "C:/x",
            "slots//1.sav",
            "%WinAppDataLocalLow%Studio/save.json",
            "%GameInstall%save.json",
            "NUL",
            "a\0b",
            "file.",
            "file ",
            "a:stream",
            "MainPlayerData.json:secret",
            "CON",
            "CON.txt",
            "COM1",
            "slots/LPT1.sav",
            "MainPlayerData.json.reborn-part",
            "slots/1.sav.Reborn-Part",
            "\\\\?\\C:\\x",
            "\\\\.\\NUL",
            "//server/share/x",
            long.as_str(),
        ] {
            assert_eq!(
                cloud.file_size(name),
                Err(RunCloudError::UnsafeName),
                "{name:?}"
            );
            assert_eq!(
                cloud.read(name, 10),
                Err(RunCloudError::UnsafeName),
                "{name:?}"
            );
        }
    }

    #[test]
    fn folders_and_large_files_are_refused() {
        let (_temp, cloud) = with_files("large");
        assert_eq!(cloud.file_size("slots"), Err(RunCloudError::NotAFile));
        assert_eq!(cloud.read("slots", 10), Err(RunCloudError::NotAFile));
        let small = RunCloud::new(&_temp.0, STEAM_ID, APP, 10);
        assert_eq!(
            small.file_size("MainPlayerData.json"),
            Err(RunCloudError::TooLarge)
        );
        assert_eq!(
            small.read("MainPlayerData.json", 1),
            Err(RunCloudError::TooLarge)
        );
        assert_eq!(small.read("slots/1.sav", 10), Ok(Some(b"slot".to_vec())));
    }

    /// Makes the file link `link` point at `target`; `false` where the
    /// system does not let the test make one (Windows without the
    /// privilege or developer mode).
    fn file_symlink(target: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link).is_ok()
        }
    }

    /// Makes the folder link `link` point at the folder `target`: a symlink
    /// on Unix, a junction on Windows (`mklink /J`, which needs no
    /// privilege), so this always works.
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

    fn outside(temp: &TempDir) -> PathBuf {
        let outside = temp.0.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret"), b"secret").unwrap();
        outside
    }

    #[test]
    fn folder_links_are_not_followed() {
        let (temp, cloud) = with_files("folder-link");
        let outside = outside(&temp);
        folder_link(&outside, &cloud.dir().join("linked"));
        assert_eq!(
            cloud.file_size("linked/secret"),
            Err(RunCloudError::Symlink)
        );
        assert_eq!(cloud.read("linked/secret", 10), Err(RunCloudError::Symlink));
        // The link itself, named as a file.
        assert_eq!(cloud.read("linked", 10), Err(RunCloudError::Symlink));

        // A link above the run's folder (another account's folder).
        let account = temp.0.join("cloud").join("654321");
        folder_link(&temp.0.join("cloud").join("123456"), &account);
        let through = RunCloud::new(&temp.0, STEAM_ID - 123_456 + 654_321, APP, QUOTA);
        assert_eq!(
            through.read("MainPlayerData.json", 10),
            Err(RunCloudError::Symlink)
        );
        assert_eq!(
            through.file_size("MainPlayerData.json"),
            Err(RunCloudError::Symlink)
        );
    }

    #[test]
    fn file_links_are_not_followed() {
        let (temp, cloud) = with_files("file-link");
        let outside = outside(&temp);
        if !file_symlink(&outside.join("secret"), &cloud.dir().join("link.json")) {
            eprintln!("skipped: cannot make file symlinks here");
            return;
        }
        assert_eq!(cloud.file_size("link.json"), Err(RunCloudError::Symlink));
        assert_eq!(cloud.read("link.json", 10), Err(RunCloudError::Symlink));
    }

    #[test]
    fn a_link_swapped_in_after_the_check_is_refused_at_the_open() {
        // `found` is the checked save; the path now names a link to a file
        // outside, as if it was swapped in between check and open.
        let (temp, cloud) = with_files("swap");
        let outside = outside(&temp);
        let found = std::fs::symlink_metadata(cloud.dir().join("MainPlayerData.json")).unwrap();
        let folder = cloud.dir().join("swapped-folder");
        folder_link(&outside, &folder);
        // A folder link is never opened as the file (Windows refuses to
        // open it without backup semantics, Unix opens the folder, whose
        // inode differs).
        assert!(open_no_follow(&folder, &found).is_err());
        let link = cloud.dir().join("swapped.json");
        if !file_symlink(&outside.join("secret"), &link) {
            eprintln!("skipped: cannot make file symlinks here");
            return;
        }
        assert!(matches!(
            open_no_follow(&link, &found),
            Err(RunCloudError::Symlink)
        ));
        // The checked file itself opens.
        assert!(matches!(
            open_no_follow(&cloud.dir().join("MainPlayerData.json"), &found),
            Ok(Some(_))
        ));
    }
}
