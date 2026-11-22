//! The run's cloud folder (ADR 0055): where the files a game reads (and
//! later writes) through the Steam interface live while it runs on the
//! shim, `<Reborn data>/cloud/<account ID>/<app ID>/`, separate per
//! account.
//!
//! This module only **reads**: the size of a file and its first bytes.
//! Nothing here writes, creates or deletes anything, and nothing here
//! talks to Valve except [`cloud_quota`], which reads the app's public app
//! info once.
//!
//! File names come from the game and are untrusted (AGENTS.md section 5).
//! A name is used only if it is a plain relative path
//! ([`reborn_content::path::install_path`]: no `..`, no absolute path, no
//! drive letters or unsafe characters) and has no Auto-Cloud root token
//! (`%WinAppDataLocalLow%...`, `docs/protocol/cloud.md`). No symlink is
//! followed, neither in the folder above the run's folder (below the
//! Reborn data folder) nor inside it.
//!
//! What is **not** here yet (ADR 0055, "What is cached, and where" and
//! "Cloud"): the base file (SHA-1, size and time of the version last
//! downloaded), filling the folder at launch, and writes. Until the launch
//! side exists, the folder holds only what was put there by hand, so a
//! game finds its files missing.

use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use reborn_cm::Session;
use reborn_content::path::install_path;
use reborn_formats::{CloudConfig, cloud_config, vdf};

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

/// The app's cloud size limit ([`limit_from`]), read from its app info
/// (one PICS request, public data).
pub async fn cloud_quota(session: &Session, app_id: u32) -> Result<Option<u64>, CoreError> {
    let text = session.app_info_text(app_id).await?;
    let document =
        vdf::parse(&text).map_err(|error| reborn_cm::Error::AppInfo(app_id, error.to_string()))?;
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
    /// there is none. A file above the limit is refused.
    pub fn read(&self, name: &str, max: u64) -> Result<Option<Vec<u8>>, RunCloudError> {
        let Some(found) = self.find(name)? else {
            return Ok(None);
        };
        self.check(&found)?;
        let path = self.path_of(name)?;
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        // Checked again on the open file: it may have changed since.
        let metadata = file.metadata()?;
        self.check(&metadata)?;
        let wanted = metadata.len().min(max).min(self.limit);
        let mut data = Vec::with_capacity(usize::try_from(wanted).unwrap_or(0));
        file.take(wanted).read_to_end(&mut data)?;
        Ok(Some(data))
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

/// A game's cloud file name as a checked relative path: a plain relative
/// path ([`install_path`]), no root token, at most [`MAX_NAME`] bytes.
pub fn api_name_path(name: &str) -> Result<PathBuf, RunCloudError> {
    if name.len() > MAX_NAME || split_root(name).is_some() {
        return Err(RunCloudError::UnsafeName);
    }
    install_path(name).map_err(|_| RunCloudError::UnsafeName)
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
        let config = cloud_config(&vdf::parse(text).unwrap());
        assert_eq!(limit_from(config.as_ref()), Some(QUOTA));
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

    /// Makes `link` point at `target`; `false` where the system does not
    /// let the test make symlinks (Windows without the privilege).
    fn symlink(target: &Path, link: &Path, dir: bool) -> bool {
        #[cfg(unix)]
        {
            let _ = dir;
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(windows)]
        {
            if dir {
                std::os::windows::fs::symlink_dir(target, link).is_ok()
            } else {
                std::os::windows::fs::symlink_file(target, link).is_ok()
            }
        }
    }

    #[test]
    fn symlinks_are_not_followed() {
        let (temp, cloud) = with_files("symlink");
        let outside = temp.0.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret"), b"secret").unwrap();
        if !symlink(
            &outside.join("secret"),
            &cloud.dir().join("link.json"),
            false,
        ) {
            eprintln!("skipped: cannot make symlinks here");
            return;
        }
        assert_eq!(cloud.file_size("link.json"), Err(RunCloudError::Symlink));
        assert_eq!(cloud.read("link.json", 10), Err(RunCloudError::Symlink));

        assert!(symlink(&outside, &cloud.dir().join("linked"), true));
        assert_eq!(cloud.read("linked/secret", 10), Err(RunCloudError::Symlink));

        // A symlink above the run's folder (the account's folder).
        let account = temp.0.join("cloud").join("654321");
        assert!(symlink(&cloud.dir().join(".."), &account, true));
        let through = RunCloud::new(&temp.0, STEAM_ID - 123_456 + 654_321, APP, QUOTA);
        assert_eq!(
            through.read("MainPlayerData.json", 10),
            Err(RunCloudError::Symlink)
        );
    }
}
