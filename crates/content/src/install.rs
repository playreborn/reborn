//! Installing depots.
//!
//! 1. **Plan:** every manifest entry is turned into a safe relative path
//!    ([`crate::path`]); symlink targets must stay inside the install
//!    directory. Anything unsafe aborts before a single byte is written.
//! 2. **Download:** all chunks are fetched in parallel, decrypted,
//!    decompressed and verified ([`crate::chunk`]), with retries on other
//!    servers, and written into a **staging directory**.
//! 3. **Verify:** every staged file with a SHA-1 in the manifest is hashed.
//! 4. **Commit:** only now files are moved into the install directory.
//!    Files flagged as user configuration are not overwritten if present.
//!
//! A failed or interrupted install never modifies the install directory;
//! the staging directory is removed on success and reset on the next try.
//! Later depots win if two depots contain the same path.

use std::collections::BTreeMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::{StreamExt, stream};
use sha1::{Digest, Sha1};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tracing::{debug, warn};

use crate::DepotKey;
use crate::cdn::{CdnClient, CdnHost};
use crate::chunk::process_chunk;
use crate::manifest::{DepotManifest, ManifestChunk, ManifestFile, file_flags};
use crate::path::{PathError, install_path, link_target};

/// One depot to install: its key and its manifest with decrypted names.
#[derive(Debug)]
pub struct DepotDownload {
    /// Depot ID.
    pub depot_id: u32,
    /// The depot's AES key.
    pub key: DepotKey,
    /// The manifest; [`DepotManifest::decrypt_filenames`] must have been called.
    pub manifest: DepotManifest,
}

/// Where to install.
#[derive(Clone, Debug)]
pub struct InstallTarget {
    /// Final directory, e.g. `<library>/steamapps/common/<installdir>`.
    pub install_dir: PathBuf,
    /// Scratch directory on the same drive, e.g.
    /// `<library>/steamapps/downloading/<appid>`. Deleted and recreated.
    pub staging_dir: PathBuf,
}

/// Tuning knobs.
#[derive(Clone, Copy, Debug)]
pub struct InstallOptions {
    /// Chunks downloaded at the same time.
    pub concurrency: usize,
    /// Attempts per chunk (each on the next server).
    pub attempts: usize,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            concurrency: 8,
            attempts: 5,
        }
    }
}

/// Download progress.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Uncompressed bytes written so far.
    pub bytes_done: u64,
    /// Uncompressed bytes to write in total.
    pub bytes_total: u64,
    /// Chunks done.
    pub chunks_done: usize,
    /// Chunks in total.
    pub chunks_total: usize,
}

/// Summary of a finished install.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstallReport {
    /// Files written.
    pub files: usize,
    /// Directories created.
    pub directories: usize,
    /// Symlinks created.
    pub symlinks: usize,
    /// Symlinks not created because the platform does not allow it (Windows).
    pub symlinks_skipped: usize,
    /// Existing user configuration files that were kept.
    pub user_config_kept: usize,
    /// Uncompressed bytes written.
    pub bytes_written: u64,
    /// Bytes downloaded from the CDN.
    pub bytes_downloaded: u64,
}

/// Installing failed. The install directory has not been modified, except
/// for [`InstallError::Commit`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InstallError {
    /// The manifest contains an unsafe name or symlink target.
    #[error("unsafe entry in depot {depot_id}: {source}")]
    UnsafePath {
        /// Depot.
        depot_id: u32,
        /// Why it was rejected.
        source: PathError,
    },
    /// The manifest still has encrypted names.
    #[error("depot {0}: file names are still encrypted")]
    NamesEncrypted(u32),
    /// No content servers given.
    #[error("no content servers to download from")]
    NoServers,
    /// A chunk could not be downloaded or verified on any attempt.
    #[error("chunk {chunk} of depot {depot_id} failed after all attempts: {last_error}")]
    Chunk {
        /// Depot.
        depot_id: u32,
        /// Chunk ID (hex).
        chunk: String,
        /// The last error.
        last_error: String,
    },
    /// A file's SHA-1 does not match the manifest.
    #[error("{0} does not match its checksum after download")]
    FileHash(PathBuf),
    /// File system error while staging (install directory untouched).
    #[error("{path}: {source}")]
    Staging {
        /// The path.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// File system error while moving files into place. The install
    /// directory may be partly updated; a new install fixes it.
    #[error("{path}: {source}")]
    Commit {
        /// The path.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
}

/// Where chunks come from. [`CdnSource`] in production, an in-memory map in tests.
pub trait ChunkSource: Sync {
    /// Fetches the raw (encrypted) chunk; `attempt` starts at 0 and can be
    /// used to switch servers.
    fn fetch(
        &self,
        depot_id: u32,
        chunk: &ManifestChunk,
        attempt: usize,
    ) -> impl Future<Output = Result<Vec<u8>, String>> + Send;
}

/// Fetches chunks from content servers, moving to the next server on every attempt.
#[derive(Debug)]
pub struct CdnSource<'a> {
    /// HTTP client.
    pub cdn: &'a CdnClient,
    /// Servers, best first.
    pub servers: &'a [CdnHost],
}

impl ChunkSource for CdnSource<'_> {
    async fn fetch(
        &self,
        depot_id: u32,
        chunk: &ManifestChunk,
        attempt: usize,
    ) -> Result<Vec<u8>, String> {
        // Spread load: start at a server derived from the chunk, then rotate.
        let start = usize::from(chunk.id.0[0]);
        let server = &self.servers[(start + attempt) % self.servers.len()];
        self.cdn
            .chunk(server, depot_id, &chunk.id)
            .await
            .map_err(|error| error.to_string())
    }
}

/// A file to write.
struct PlannedFile<'a> {
    depot: usize,
    file: &'a ManifestFile,
}

/// What ends up in the install directory, keyed by relative path.
enum Planned<'a> {
    Directory,
    File(PlannedFile<'a>),
    Symlink(PathBuf),
}

struct ChunkTask<'a> {
    depot: usize,
    path: &'a Path,
    chunk: &'a ManifestChunk,
}

/// Installs the depots into `target`. See the module docs for the steps.
pub async fn install(
    depots: &[DepotDownload],
    target: &InstallTarget,
    source: &impl ChunkSource,
    options: InstallOptions,
    mut on_progress: impl FnMut(Progress),
) -> Result<InstallReport, InstallError> {
    let plan = plan(depots)?;
    let mut report = InstallReport::default();

    // Fresh staging directory with all files pre-allocated.
    let staging = &target.staging_dir;
    remove_dir_if_exists(staging).await?;
    create_dir(staging).await?;
    for (path, entry) in &plan {
        if let Planned::File(planned) = entry {
            let staged = staging.join(path);
            if let Some(parent) = staged.parent() {
                create_dir(parent).await?;
            }
            let file = tokio::fs::File::create(&staged)
                .await
                .map_err(staging_error(&staged))?;
            file.set_len(planned.file.size)
                .await
                .map_err(staging_error(&staged))?;
        }
    }

    // Download all chunks in parallel and write them at their offsets.
    let tasks: Vec<ChunkTask<'_>> = plan
        .iter()
        .filter_map(|(path, entry)| match entry {
            Planned::File(planned) => Some((path, planned)),
            _ => None,
        })
        .flat_map(|(path, planned)| {
            planned.file.chunks.iter().map(move |chunk| ChunkTask {
                depot: planned.depot,
                path,
                chunk,
            })
        })
        .collect();
    let mut progress = Progress {
        bytes_total: tasks.iter().map(|task| u64::from(task.chunk.size)).sum(),
        chunks_total: tasks.len(),
        ..Progress::default()
    };
    on_progress(progress);

    let mut downloads = stream::iter(tasks)
        .map(|task| async move {
            let depot = &depots[task.depot];
            let (data, downloaded) =
                fetch_verified(source, depot, task.chunk, options.attempts).await?;
            Ok::<_, InstallError>((task, data, downloaded))
        })
        .buffer_unordered(options.concurrency.max(1));
    while let Some(result) = downloads.next().await {
        let (task, data, downloaded) = result?;
        write_at(&staging.join(task.path), task.chunk.offset, &data).await?;
        report.bytes_downloaded += downloaded as u64;
        progress.bytes_done += data.len() as u64;
        progress.chunks_done += 1;
        on_progress(progress);
    }

    // Verify whole files.
    for (path, entry) in &plan {
        if let Planned::File(planned) = entry
            && let Some(expected) = planned.file.sha_content
        {
            let staged = staging.join(path);
            if sha1_of(&staged).await? != expected {
                return Err(InstallError::FileHash(path.clone()));
            }
        }
    }

    // Commit: move everything into place.
    let install_dir = &target.install_dir;
    create_dir(install_dir).await.map_err(as_commit)?;
    for (path, entry) in &plan {
        let destination = install_dir.join(path);
        match entry {
            Planned::Directory => {
                create_dir(&destination).await.map_err(as_commit)?;
                report.directories += 1;
            }
            Planned::File(planned) => {
                if let Some(parent) = destination.parent() {
                    create_dir(parent).await.map_err(as_commit)?;
                }
                let is_user_config = planned.file.flags & file_flags::USER_CONFIG != 0;
                if is_user_config && tokio::fs::try_exists(&destination).await.unwrap_or(false) {
                    report.user_config_kept += 1;
                    continue;
                }
                let staged = staging.join(path);
                tokio::fs::rename(&staged, &destination)
                    .await
                    .map_err(commit_error(&destination))?;
                if planned.file.is_executable() {
                    make_executable(&destination).await?;
                }
                report.files += 1;
                report.bytes_written += planned.file.size;
            }
            Planned::Symlink(link) => {
                if let Some(parent) = destination.parent() {
                    create_dir(parent).await.map_err(as_commit)?;
                }
                if create_symlink(link, &destination).await? {
                    report.symlinks += 1;
                } else {
                    report.symlinks_skipped += 1;
                }
            }
        }
    }
    remove_dir_if_exists(staging).await?;
    debug!(?report, "install finished");
    Ok(report)
}

/// Validates every entry and merges all depots into one plan.
fn plan(depots: &[DepotDownload]) -> Result<BTreeMap<PathBuf, Planned<'_>>, InstallError> {
    let mut plan = BTreeMap::new();
    for (index, depot) in depots.iter().enumerate() {
        if depot.manifest.filenames_encrypted {
            return Err(InstallError::NamesEncrypted(depot.depot_id));
        }
        let unsafe_path = |source| InstallError::UnsafePath {
            depot_id: depot.depot_id,
            source,
        };
        for file in &depot.manifest.files {
            let path = install_path(&file.name).map_err(unsafe_path)?;
            let entry = if file.is_directory() {
                Planned::Directory
            } else if file.is_symlink() {
                let target = file.link_target.as_deref().unwrap_or_default();
                Planned::Symlink(link_target(&path, target).map_err(unsafe_path)?)
            } else {
                Planned::File(PlannedFile { depot: index, file })
            };
            if plan.insert(path.clone(), entry).is_some() {
                warn!(path = %path.display(), depot = depot.depot_id, "path provided by more than one depot, later depot wins");
            }
        }
    }
    Ok(plan)
}

async fn fetch_verified(
    source: &impl ChunkSource,
    depot: &DepotDownload,
    chunk: &ManifestChunk,
    attempts: usize,
) -> Result<(Vec<u8>, usize), InstallError> {
    let mut last_error = String::from("no attempt made");
    for attempt in 0..attempts.max(1) {
        if attempt > 0 {
            let delay = Duration::from_millis(250 << attempt.min(5));
            tokio::time::sleep(delay).await;
        }
        match source.fetch(depot.depot_id, chunk, attempt).await {
            Ok(raw) => match process_chunk(&raw, &depot.key, chunk.expectation()) {
                Ok(data) => return Ok((data, raw.len())),
                Err(error) => last_error = error.to_string(),
            },
            Err(error) => last_error = error,
        }
        debug!(chunk = %chunk.id.to_hex(), attempt, %last_error, "chunk attempt failed");
    }
    Err(InstallError::Chunk {
        depot_id: depot.depot_id,
        chunk: chunk.id.to_hex(),
        last_error,
    })
}

async fn write_at(path: &Path, offset: u64, data: &[u8]) -> Result<(), InstallError> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .await
        .map_err(staging_error(path))?;
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(staging_error(path))?;
    file.write_all(data).await.map_err(staging_error(path))?;
    file.flush().await.map_err(staging_error(path))
}

async fn sha1_of(path: &Path) -> Result<[u8; 20], InstallError> {
    let owned = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut file = std::fs::File::open(&owned)?;
        let mut hasher = Sha1::new();
        let mut buffer = vec![0; 1 << 16];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        Ok::<_, std::io::Error>(hasher.finalize().into())
    })
    .await
    .map_err(|join| std::io::Error::other(join.to_string()))
    .and_then(|result| result)
    .map_err(staging_error(path))
}

async fn create_dir(path: &Path) -> Result<(), InstallError> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(staging_error(path))
}

async fn remove_dir_if_exists(path: &Path) -> Result<(), InstallError> {
    match tokio::fs::remove_dir_all(path).await {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(staging_error(path)(error))
        }
        _ => Ok(()),
    }
}

#[cfg(unix)]
async fn make_executable(path: &Path) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .await
        .map_err(commit_error(path))
}

#[cfg(not(unix))]
async fn make_executable(_path: &Path) -> Result<(), InstallError> {
    Ok(())
}

/// Returns `false` if symlinks are not supported here (Windows without
/// developer mode); the link is then skipped.
#[cfg(unix)]
async fn create_symlink(target: &Path, link: &Path) -> Result<bool, InstallError> {
    let _ = tokio::fs::remove_file(link).await;
    tokio::fs::symlink(target, link)
        .await
        .map(|()| true)
        .map_err(commit_error(link))
}

#[cfg(not(unix))]
async fn create_symlink(target: &Path, link: &Path) -> Result<bool, InstallError> {
    warn!(link = %link.display(), target = %target.display(), "symlinks are not created on this platform");
    Ok(false)
}

fn staging_error(path: &Path) -> impl FnOnce(std::io::Error) -> InstallError + '_ {
    move |source| InstallError::Staging {
        path: path.to_path_buf(),
        source,
    }
}

fn commit_error(path: &Path) -> impl FnOnce(std::io::Error) -> InstallError + '_ {
    move |source| InstallError::Commit {
        path: path.to_path_buf(),
        source,
    }
}

fn as_commit(error: InstallError) -> InstallError {
    match error {
        InstallError::Staging { path, source } => InstallError::Commit { path, source },
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checksum::steam_adler32;
    use crate::chunk::tests::{key, sample, vzstd};
    use crate::crypto;
    use crate::manifest::ChunkId;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory chunk store; `fail_first` makes the first N fetches fail.
    #[derive(Default)]
    struct MemorySource {
        chunks: HashMap<ChunkId, Vec<u8>>,
        fail_first: Mutex<usize>,
    }

    impl ChunkSource for MemorySource {
        async fn fetch(
            &self,
            _depot: u32,
            chunk: &ManifestChunk,
            _attempt: usize,
        ) -> Result<Vec<u8>, String> {
            let fail = {
                let mut remaining = self.fail_first.lock().unwrap();
                let fail = *remaining > 0;
                *remaining = remaining.saturating_sub(1);
                fail
            };
            if fail {
                return Err("simulated outage".to_owned());
            }
            self.chunks
                .get(&chunk.id)
                .cloned()
                .ok_or_else(|| "404".to_owned())
        }
    }

    /// Unique scratch directory, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-install-{name}-{}-{}",
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

        fn target(&self) -> InstallTarget {
            InstallTarget {
                install_dir: self.0.join("common").join("Game"),
                staging_dir: self.0.join("downloading").join("1"),
            }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Builds a file of `data`, split into chunks of `chunk_size`, and
    /// stores the encrypted chunks in `source`.
    fn file(
        name: &str,
        data: &[u8],
        chunk_size: usize,
        flags: u32,
        source: &mut MemorySource,
    ) -> ManifestFile {
        let mut chunks = Vec::new();
        for (index, piece) in data.chunks(chunk_size).enumerate() {
            let mut id = [0u8; 20];
            id[..8].copy_from_slice(&(source.chunks.len() as u64 + 1).to_le_bytes());
            let id = ChunkId(id);
            source.chunks.insert(
                id,
                crypto::encrypt(&vzstd(piece), &key(), [index as u8; 16]),
            );
            chunks.push(ManifestChunk {
                id,
                offset: (index * chunk_size) as u64,
                size: piece.len() as u32,
                compressed_size: 0,
                checksum: steam_adler32(piece),
            });
        }
        ManifestFile {
            name: name.to_owned(),
            size: data.len() as u64,
            flags,
            sha_content: Some(Sha1::digest(data).into()),
            chunks,
            link_target: None,
        }
    }

    fn entry(name: &str, flags: u32, link_target: Option<&str>) -> ManifestFile {
        ManifestFile {
            name: name.to_owned(),
            size: 0,
            flags,
            sha_content: None,
            chunks: Vec::new(),
            link_target: link_target.map(str::to_owned),
        }
    }

    fn depot(files: Vec<ManifestFile>) -> DepotDownload {
        DepotDownload {
            depot_id: 481,
            key: key(),
            manifest: DepotManifest {
                depot_id: 481,
                manifest_id: 1,
                creation_time: 0,
                filenames_encrypted: false,
                total_size: 0,
                total_compressed_size: 0,
                files,
            },
        }
    }

    fn options() -> InstallOptions {
        InstallOptions {
            concurrency: 4,
            attempts: 3,
        }
    }

    #[tokio::test]
    async fn installs_files_and_directories() {
        let dir = TempDir::new("ok");
        let mut source = MemorySource::default();
        let big = sample(300_000);
        let files = vec![
            entry("data", file_flags::DIRECTORY, None),
            file("data\\big.pak", &big, 64 * 1024, 0, &mut source),
            file(
                "game.exe",
                b"MZ executable",
                1024,
                file_flags::EXECUTABLE,
                &mut source,
            ),
            file("empty.txt", b"", 1024, 0, &mut source),
            entry("data\\saves", file_flags::DIRECTORY, None),
        ];
        let mut last = Progress::default();
        let report = install(&[depot(files)], &dir.target(), &source, options(), |p| {
            last = p
        })
        .await
        .unwrap();

        let game = dir.target().install_dir;
        assert_eq!(
            std::fs::read(game.join("data").join("big.pak")).unwrap(),
            big
        );
        assert_eq!(
            std::fs::read(game.join("game.exe")).unwrap(),
            b"MZ executable"
        );
        assert_eq!(std::fs::read(game.join("empty.txt")).unwrap(), b"");
        assert!(game.join("data").join("saves").is_dir());
        assert!(!dir.target().staging_dir.exists(), "staging removed");
        assert_eq!((report.files, report.directories), (3, 2));
        assert_eq!(last.bytes_done, last.bytes_total);
        assert_eq!(last.chunks_done, last.chunks_total);
        assert_eq!(report.bytes_written, big.len() as u64 + 13);
    }

    #[tokio::test]
    async fn retries_failed_chunks() {
        let dir = TempDir::new("retry");
        let mut source = MemorySource::default();
        let files = vec![file("a.bin", &sample(10_000), 4096, 0, &mut source)];
        source.fail_first = Mutex::new(2);
        install(&[depot(files)], &dir.target(), &source, options(), |_| {})
            .await
            .unwrap();
        assert!(dir.target().install_dir.join("a.bin").is_file());
    }

    #[tokio::test]
    async fn corrupt_chunk_leaves_install_dir_untouched() {
        let dir = TempDir::new("corrupt");
        let mut source = MemorySource::default();
        let mut files = vec![file("a.bin", &sample(10_000), 4096, 0, &mut source)];
        files[0].chunks[1].checksum ^= 1;
        let result = install(&[depot(files)], &dir.target(), &source, options(), |_| {}).await;
        assert!(matches!(result, Err(InstallError::Chunk { .. })));
        assert!(!dir.target().install_dir.exists());
    }

    #[tokio::test]
    async fn wrong_file_hash_is_rejected() {
        let dir = TempDir::new("hash");
        let mut source = MemorySource::default();
        let mut files = vec![file("a.bin", &sample(5_000), 4096, 0, &mut source)];
        files[0].sha_content = Some([0; 20]);
        let result = install(&[depot(files)], &dir.target(), &source, options(), |_| {}).await;
        assert!(matches!(result, Err(InstallError::FileHash(_))));
        assert!(!dir.target().install_dir.exists());
    }

    #[tokio::test]
    async fn unsafe_names_abort_before_writing() {
        let dir = TempDir::new("unsafe");
        let mut source = MemorySource::default();
        let files = vec![
            file("ok.txt", b"fine", 1024, 0, &mut source),
            file("..\\..\\evil.dll", b"bad", 1024, 0, &mut source),
        ];
        let result = install(&[depot(files)], &dir.target(), &source, options(), |_| {}).await;
        assert!(matches!(result, Err(InstallError::UnsafePath { .. })));
        assert!(!dir.target().install_dir.exists());
        assert!(!dir.target().staging_dir.exists());
    }

    #[tokio::test]
    async fn escaping_symlinks_are_rejected() {
        let dir = TempDir::new("link");
        let files = vec![entry(
            "bin\\run",
            file_flags::SYMLINK,
            Some("../../../etc/passwd"),
        )];
        let result = install(
            &[depot(files)],
            &dir.target(),
            &MemorySource::default(),
            options(),
            |_| {},
        )
        .await;
        assert!(matches!(result, Err(InstallError::UnsafePath { .. })));
    }

    #[tokio::test]
    async fn keeps_existing_user_config() {
        let dir = TempDir::new("config");
        let game = dir.target().install_dir;
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("settings.cfg"), b"my settings").unwrap();

        let mut source = MemorySource::default();
        let files = vec![file(
            "settings.cfg",
            b"defaults",
            1024,
            file_flags::USER_CONFIG,
            &mut source,
        )];
        let report = install(&[depot(files)], &dir.target(), &source, options(), |_| {})
            .await
            .unwrap();
        assert_eq!(report.user_config_kept, 1);
        assert_eq!(
            std::fs::read(game.join("settings.cfg")).unwrap(),
            b"my settings"
        );
    }
}
