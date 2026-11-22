//! The shadow folder for cloud reads in shadow mode (ADR 0055, stage 1).
//!
//! In shadow mode the official client runs the game and serves its cloud
//! files; the observation proxy compares the official `GetFileSize` and
//! `FileRead` results with Reborn's answers from `rebornd`'s shadow
//! session. ADR 0055 says where Reborn's answers come from: "`CloudRead`
//! against a separate shadow folder", "Shadow runs use a separate shadow
//! folder that no game writes to", filled with "one cloud listing plus a
//! download of each file the proxy compares for `FileRead` into the
//! shadow folder", bounded per run.
//!
//! [`fill_shadow_folder`] does that once per shadow run (one
//! `reborn-observe arm --app`):
//!
//! 1. the app's cloud quota from its app info (already fetched by the
//!    caller); an app without one has no cloud and gets no folder;
//! 2. one listing of the app's cloud files (`Session::cloud_files`,
//!    `Cloud.EnumerateUserFiles`, `docs/protocol/cloud.md`);
//! 3. the shadow folder of an earlier run is removed ([`clear`]): the
//!    download never overwrites, and a file left from an earlier run
//!    would be compared instead of the cloud's current version;
//! 4. each file a game can read through the API ([`shadow_files`]: a name
//!    without a root token that passes the install-path checks, at most
//!    the quota, at most [`MAX_SHADOW_FILES`] files and [`MAX_SHADOW_BYTES`]
//!    in all) is downloaded with the existing, sourced download
//!    ([`reborn_content::cloud::download_cloud_file_into`]: plain GET on
//!    the listed URL, SHA-1 checked against the listing, written as a part
//!    file and renamed, never overwriting, no symlink followed).
//!
//! **Deviation, stated:** ADR 0055 counts "a download of each file the
//! proxy compares". Which files the game will read is known only while it
//! runs, and the proxy must not make the game wait for a download, so the
//! fill downloads every file a game *could* read through the API, up
//! front, within the bounds above. For the recorded game (Cave Crawlers)
//! that is the same two files.
//!
//! The shadow folder stays on disk after the run (it is replaced by the
//! next fill of the same account and app); it holds copies of the user's
//! cloud saves, below Reborn's own data folder.
//!
//! Nothing is written to the official client's folders or to the run's
//! cloud folder ([`crate::run_cloud`]), and nothing is uploaded or sent
//! to Valve except the listing and the downloads (reads). Only counts are
//! logged by callers: names are the game's, contents the user's saves.

use std::path::{Path, PathBuf};

use reborn_cm::{CloudFile, Session};
use reborn_content::cdn::CdnClient;
use reborn_content::cloud::{CloudFileRef, CloudOutcome, download_cloud_file_into};

use crate::CoreError;
use crate::run_cloud::{RunCloud, RunCloudError, api_name_path, quota_from_app_info};

/// Most files downloaded into a shadow folder per run. Reborn's own bound
/// (red line 8); the recorded game with API cloud files (Cave Crawlers)
/// has two (`docs/protocol/cloud.md`).
pub const MAX_SHADOW_FILES: usize = 64;

/// Most bytes (listed sizes) downloaded into a shadow folder per run, all
/// files together. Reborn's own bound (red line 8).
pub const MAX_SHADOW_BYTES: u64 = 64 * 1024 * 1024;

/// What filling a shadow folder did; counts only (names are the game's).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShadowFill {
    /// Files downloaded, checked and written.
    pub written: usize,
    /// Listed files a game cannot read through the API (a root token or an
    /// unsafe name), above the quota, or beyond [`MAX_SHADOW_FILES`] or
    /// [`MAX_SHADOW_BYTES`]: not downloaded.
    pub skipped: usize,
    /// Downloads that failed (no download link, network, SHA-1, a name
    /// clash): not written.
    pub failed: usize,
}

/// Why the shadow folder could not be filled.
#[derive(Debug, thiserror::Error)]
pub enum ShadowCloudError {
    /// Talking to Valve, or the app info, failed.
    #[error(transparent)]
    Core(#[from] CoreError),
    /// The shadow folder is not usable (a link on the way, a file in its
    /// place, a file system error).
    #[error("shadow folder: {0}")]
    Folder(#[from] RunCloudError),
}

/// The listed files that go into the shadow folder, with their checked
/// relative paths, and how many were left out (see [`ShadowFill::skipped`]).
pub fn shadow_files(files: &[CloudFile], limit: u64) -> (Vec<(&CloudFile, PathBuf)>, usize) {
    let mut chosen = Vec::new();
    let mut skipped = 0;
    let mut total = 0u64;
    for file in files {
        let path = match api_name_path(&file.name) {
            Ok(path) => path,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let size = u64::from(file.size);
        if size > limit
            || chosen.len() >= MAX_SHADOW_FILES
            || total.saturating_add(size) > MAX_SHADOW_BYTES
        {
            skipped += 1;
            continue;
        }
        total += size;
        chosen.push((file, path));
    }
    (chosen, skipped)
}

/// Removes the shadow folder `shadow` with everything in it, after
/// checking that no part of its path below the Reborn data folder is a
/// link ([`RunCloud::dir_exists`]). Links inside it are removed, not
/// followed (`std::fs::remove_dir_all`). Refuses a folder that is not a
/// shadow folder ([`RunCloudError::NotShadow`]): the run's cloud folder
/// holds the user's saves.
///
/// **Race, not caught:** the check and the removal are two steps on paths
/// (std has no handle-relative removal). Another process of the same user
/// that swaps a folder *above* the shadow folder (`shadow-cloud` or the
/// account's folder) for a link or junction between them makes the
/// removal go through it; the shadow folder itself swapped for a link is
/// safe (`remove_dir_all` removes the link, not its target). The threat is
/// the one `crate::run_cloud` describes: a process that can do this can
/// already delete those files itself.
pub fn clear(shadow: &RunCloud) -> Result<(), RunCloudError> {
    if !shadow.is_shadow() {
        return Err(RunCloudError::NotShadow);
    }
    if shadow.dir_exists()? {
        std::fs::remove_dir_all(shadow.dir())?;
    }
    Ok(())
}

/// Fills the shadow folder of `app_id` below `data_dir` from the account's
/// cloud (see the module documentation), with `app_info` already fetched
/// ([`Session::app_info_text`]). `None` for an app without a cloud quota.
pub async fn fill_shadow_folder(
    session: &Session,
    data_dir: &Path,
    app_id: u32,
    app_info: &str,
) -> Result<Option<(RunCloud, ShadowFill)>, ShadowCloudError> {
    let Some(limit) = quota_from_app_info(app_id, app_info)? else {
        return Ok(None);
    };
    let shadow = RunCloud::shadow(data_dir, session.steam_id(), app_id, limit);
    let files = session.cloud_files(app_id).await.map_err(CoreError::from)?;
    let client = CdnClient::new().map_err(|error| CoreError::Http(error.to_string()))?;
    let folder = shadow.dir();
    let fill = fill_from(&shadow, &files, |relative, file| {
        let (client, folder) = (&client, &folder);
        async move {
            let Some(url) = &file.download_url else {
                return Err("no download link".to_owned());
            };
            let reference = CloudFileRef {
                name: &file.name,
                url: url.expose(),
                size: file.size,
                sha1: &file.sha1,
            };
            download_cloud_file_into(client, folder, &relative, reference)
                .await
                .map_err(|error| error.to_string())
        }
    })
    .await?;
    Ok(Some((shadow, fill)))
}

/// [`fill_shadow_folder`] after the listing: clears the folder, then
/// hands each chosen file to `download` (the real download, or a fake in
/// tests) one after the other.
async fn fill_from<'a, F, Fut>(
    shadow: &RunCloud,
    files: &'a [CloudFile],
    mut download: F,
) -> Result<ShadowFill, RunCloudError>
where
    F: FnMut(PathBuf, &'a CloudFile) -> Fut,
    Fut: Future<Output = Result<CloudOutcome, String>>,
{
    clear(shadow)?;
    let (chosen, skipped) = shadow_files(files, shadow.limit());
    let mut fill = ShadowFill {
        skipped,
        ..ShadowFill::default()
    };
    for (file, relative) in chosen {
        match download(relative, file).await {
            Ok(_) => fill.written += 1,
            Err(_) => fill.failed += 1,
        }
    }
    Ok(fill)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up SteamID: the individual base plus account 123456.
    const STEAM_ID: u64 = 0x0110_0001_0000_0000 + 123_456;
    /// Cave Crawlers: API cloud files without a root token, quota 1 000 000
    /// (`docs/protocol/cloud.md`).
    const APP: u32 = 2_143_030;
    const QUOTA: u64 = 1_000_000;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-core-shadow-cloud-{name}-{}-{}",
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

    /// A listed file. Its download link is left out: a `CloudUrl` comes
    /// only from Valve's answer, and the fake downloads below need none.
    fn listed(name: &str, size: u32) -> CloudFile {
        CloudFile {
            name: name.to_owned(),
            size,
            timestamp: 0,
            sha1: "0".repeat(40),
            platforms: vec!["All".to_owned()],
            download_url: None,
        }
    }

    #[test]
    fn only_files_a_game_reads_through_the_api_are_chosen() {
        let files = vec![
            listed("MainPlayerData.json", 44_875),
            listed("BackUp_MainPlayerData.json", 44_875),
            listed("%WinAppDataLocalLow%Studio/save.json", 10),
            listed("../escape.json", 10),
            listed("C:/absolute.json", 10),
            listed("big.json", 1_000_001),
        ];
        let (chosen, skipped) = shadow_files(&files, QUOTA);
        let names: Vec<&str> = chosen.iter().map(|(file, _)| file.name.as_str()).collect();
        assert_eq!(names, ["MainPlayerData.json", "BackUp_MainPlayerData.json"]);
        assert_eq!(chosen[0].1, PathBuf::from("MainPlayerData.json"));
        assert_eq!(skipped, 4);
    }

    #[test]
    fn at_most_the_bound_of_files_is_chosen() {
        let files: Vec<CloudFile> = (0..MAX_SHADOW_FILES + 3)
            .map(|index| listed(&format!("{index}.sav"), 1))
            .collect();
        let (chosen, skipped) = shadow_files(&files, QUOTA);
        assert_eq!(chosen.len(), MAX_SHADOW_FILES);
        assert_eq!(skipped, 3);
    }

    #[test]
    fn at_most_the_byte_bound_is_chosen() {
        // 16 MiB each (the largest quota): four fit in 64 MiB, the fifth
        // does not, a small one after it still does.
        let big = 16 * 1024 * 1024;
        let mut files: Vec<CloudFile> = (0..5)
            .map(|index| listed(&format!("{index}.sav"), big))
            .collect();
        files.push(listed("small.sav", 1));
        let (chosen, skipped) = shadow_files(&files, u64::from(big));
        assert_eq!(chosen.len(), 4);
        assert_eq!(skipped, 2, "the fifth big one and the small one");
    }

    #[test]
    fn clearing_removes_only_a_shadow_folder() {
        let temp = TempDir::new("clear");
        let shadow = RunCloud::shadow(&temp.0, STEAM_ID, APP, QUOTA);
        assert!(clear(&shadow).is_ok(), "nothing there is fine");
        std::fs::create_dir_all(shadow.dir().join("slots")).unwrap();
        std::fs::write(shadow.dir().join("slots").join("1.sav"), b"old").unwrap();
        assert!(clear(&shadow).is_ok());
        assert!(!shadow.dir().exists());

        // The run's cloud folder holds the user's saves: never removed.
        let run = RunCloud::new(&temp.0, STEAM_ID, APP, QUOTA);
        std::fs::create_dir_all(run.dir()).unwrap();
        std::fs::write(run.dir().join("MainPlayerData.json"), b"save").unwrap();
        assert_eq!(clear(&run), Err(RunCloudError::NotShadow));
        assert!(run.dir().join("MainPlayerData.json").exists());
    }

    /// Makes the folder link `link` point at the folder `target`: a symlink
    /// on Unix, a junction on Windows (`mklink /J`, no extra rights).
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

    #[test]
    fn clearing_follows_no_link_above_or_at_the_shadow_folder() {
        let temp = TempDir::new("clear-link");
        let outside = temp.0.join("outside");
        std::fs::create_dir_all(outside.join(APP.to_string())).unwrap();
        std::fs::write(outside.join(APP.to_string()).join("keep"), b"keep").unwrap();
        std::fs::create_dir_all(temp.0.join("shadow-cloud")).unwrap();
        // The account's folder is a link to a folder outside.
        folder_link(&outside, &temp.0.join("shadow-cloud").join("123456"));
        let shadow = RunCloud::shadow(&temp.0, STEAM_ID, APP, QUOTA);
        assert_eq!(clear(&shadow), Err(RunCloudError::Symlink));
        // The app's folder itself is a link.
        let other = RunCloud::shadow(&temp.0, STEAM_ID + 1, APP, QUOTA);
        std::fs::create_dir_all(temp.0.join("shadow-cloud").join("123457")).unwrap();
        folder_link(&outside.join(APP.to_string()), &other.dir());
        assert_eq!(clear(&other), Err(RunCloudError::Symlink));
        assert!(outside.join(APP.to_string()).join("keep").exists());
    }

    #[test]
    fn filling_clears_then_downloads_each_chosen_file_once() {
        let temp = TempDir::new("fill");
        let shadow = RunCloud::shadow(&temp.0, STEAM_ID, APP, QUOTA);
        std::fs::create_dir_all(shadow.dir()).unwrap();
        std::fs::write(shadow.dir().join("stale.json"), b"from an earlier run").unwrap();
        let files = vec![
            listed("MainPlayerData.json", 3),
            listed("%WinAppDataLocal%x.json", 3),
            listed("BackUp_MainPlayerData.json", 3),
        ];
        let folder = shadow.dir();
        let mut asked = Vec::new();
        let fill = block_on(fill_from(&shadow, &files, |relative, file| {
            asked.push(file.name.clone());
            let folder = folder.clone();
            async move {
                // A fake download: the first file arrives, the second fails.
                if relative == Path::new("MainPlayerData.json") {
                    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
                    std::fs::write(folder.join(&relative), b"new")
                        .map_err(|error| error.to_string())?;
                    Ok(CloudOutcome::Written(folder.join(relative)))
                } else {
                    Err("SHA-1 mismatch".to_owned())
                }
            }
        }));
        assert_eq!(
            fill,
            Ok(ShadowFill {
                written: 1,
                skipped: 1,
                failed: 1
            })
        );
        assert_eq!(asked, ["MainPlayerData.json", "BackUp_MainPlayerData.json"]);
        assert!(!shadow.dir().join("stale.json").exists(), "cleared first");
        assert_eq!(
            shadow.read("MainPlayerData.json", 10),
            Ok(Some(b"new".to_vec()))
        );
    }

    /// Runs a future that never waits on anything (the fakes above) to its
    /// end in one poll; a future that would wait is a test bug.
    fn block_on<F: Future>(future: F) -> F::Output {
        use std::task::{Context, Poll, Waker};
        let mut future = std::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("the fake future waited"),
        }
    }
}
