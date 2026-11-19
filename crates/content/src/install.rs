//! Installing, updating and verifying depots.
//!
//! 1. **Plan:** every manifest entry is turned into a safe relative path
//!    ([`crate::path`]); symlink targets must stay inside the install
//!    directory. Anything unsafe aborts before a single byte is written.
//! 2. **Classify:** files that are already installed and unchanged are left
//!    alone. Without [`InstallOptions::verify_existing`], "unchanged" means the
//!    previous manifest lists the same path with the same SHA-1 and the file
//!    has the right size; with it, the installed file is hashed. Existing user
//!    configuration files are kept.
//! 3. **Build:** every other file is assembled in a **staging directory**.
//!    Each chunk comes from the first source that has it with the right size
//!    and checksum: the staging file itself (an interrupted earlier attempt),
//!    the installed files (found via the chunk ID in the previous manifest or
//!    at the same position), or the CDN (in parallel, with retries).
//! 4. **Verify:** every built file with a SHA-1 is hashed. If a file that
//!    reused local data fails, all its chunks are downloaded again once.
//! 5. **Commit:** built files are moved into the install directory; files
//!    and directories of the previous manifest that the new one no longer
//!    has are removed.
//!
//! A failed or interrupted run never modifies the install directory; the
//! staging directory is kept so the next run can continue where it stopped,
//! and is removed after a successful commit. Later depots win if two depots
//! contain the same path.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use sha1::{Digest, Sha1};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tracing::{debug, warn};

use crate::DepotKey;
use crate::cdn::{CdnClient, CdnHost};
use crate::checksum::steam_adler32;
use crate::chunk::process_chunk;
use crate::manifest::{ChunkId, DepotManifest, ManifestChunk, ManifestFile, file_flags};
use crate::path::{PathError, install_path, link_target};

/// One depot to install: its key, the target manifest and, for updates,
/// the manifest that is currently installed. Names must be decrypted.
#[derive(Debug)]
pub struct DepotDownload {
    /// Depot ID.
    pub depot_id: u32,
    /// The depot's AES key.
    pub key: DepotKey,
    /// The manifest to install.
    pub manifest: DepotManifest,
    /// The manifest that is installed now, if any. Used to find reusable
    /// chunks and to remove files that no longer exist.
    pub previous: Option<DepotManifest>,
}

/// Where to install.
#[derive(Clone, Debug)]
pub struct InstallTarget {
    /// Final directory, e.g. `<library>/steamapps/common/<installdir>`.
    pub install_dir: PathBuf,
    /// Scratch directory on the same drive, e.g.
    /// `<library>/steamapps/downloading/<appid>`.
    pub staging_dir: PathBuf,
}

/// Tuning knobs.
#[derive(Clone, Copy, Debug)]
pub struct InstallOptions {
    /// Chunks downloaded at the same time.
    pub concurrency: usize,
    /// Attempts per chunk (each on the next server).
    pub attempts: usize,
    /// Hash installed files instead of trusting the previous manifest
    /// ("verify integrity"). Broken files are repaired.
    pub verify_existing: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            concurrency: 8,
            attempts: 5,
            verify_existing: false,
        }
    }
}

/// Progress of the build step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// Uncompressed bytes done (downloaded or reused).
    pub bytes_done: u64,
    /// Uncompressed bytes to build in total.
    pub bytes_total: u64,
    /// Chunks done.
    pub chunks_done: usize,
    /// Chunks in total.
    pub chunks_total: usize,
}

/// Summary of a finished run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstallReport {
    /// Files written (new, changed or repaired).
    pub files: usize,
    /// Files that were already correct and left alone.
    pub files_unchanged: usize,
    /// Files of the previous version that were removed.
    pub files_removed: usize,
    /// Directories created.
    pub directories: usize,
    /// Symlinks created.
    pub symlinks: usize,
    /// Symlinks not created because the platform does not allow it (Windows).
    pub symlinks_skipped: usize,
    /// Existing user configuration files that were kept.
    pub user_config_kept: usize,
    /// Chunks downloaded from the CDN.
    pub chunks_downloaded: usize,
    /// Chunks taken from installed files or an earlier, interrupted attempt.
    pub chunks_reused: usize,
    /// Uncompressed bytes written into the install directory.
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
    /// A manifest still has encrypted names.
    #[error("depot {0}: file names are still encrypted")]
    NamesEncrypted(u32),
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
    /// A file's SHA-1 does not match the manifest even after a full download.
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
    /// directory may be partly updated; running again fixes it.
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
        if self.servers.is_empty() {
            return Err("no content servers".to_owned());
        }
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

/// The validated, merged view of all depots.
struct Plan<'a> {
    entries: BTreeMap<PathBuf, Planned<'a>>,
    /// Files of the previous manifests, by path.
    previous_files: HashMap<PathBuf, &'a ManifestFile>,
    /// Where each chunk of the previous manifests lives in the installation.
    previous_chunks: HashMap<ChunkId, (PathBuf, u64)>,
    /// Files and links of the previous manifests that the new ones lack.
    removed_files: Vec<PathBuf>,
    /// Directories of the previous manifests that the new ones lack.
    removed_directories: Vec<PathBuf>,
}

#[derive(Clone, Copy)]
struct ChunkTask<'a> {
    depot: usize,
    path: &'a Path,
    chunk: &'a ManifestChunk,
    force_download: bool,
}

enum Origin {
    /// Already correct in the staging file.
    Staged,
    /// Copied from the installation; must be written to the staging file.
    Reused(Vec<u8>),
    /// Downloaded (bytes on the wire); must be written to the staging file.
    Downloaded(Vec<u8>, usize),
}

/// Installs, updates or verifies the depots in `target`. See the module docs.
pub async fn install(
    depots: &[DepotDownload],
    target: &InstallTarget,
    source: &impl ChunkSource,
    options: InstallOptions,
    mut on_progress: impl FnMut(Progress),
) -> Result<InstallReport, InstallError> {
    let plan = plan(depots)?;
    let mut report = InstallReport::default();
    let install_dir = &target.install_dir;
    let staging = &target.staging_dir;

    // Decide which files have to be built.
    let mut to_build: Vec<(&PathBuf, &PlannedFile<'_>)> = Vec::new();
    for (path, entry) in &plan.entries {
        let Planned::File(planned) = entry else {
            continue;
        };
        let destination = install_dir.join(path);
        let exists = is_file(&destination).await;
        if exists && planned.file.flags & file_flags::USER_CONFIG != 0 {
            report.user_config_kept += 1;
        } else if exists && is_unchanged(&plan, path, planned.file, &destination, options).await? {
            report.files_unchanged += 1;
        } else {
            to_build.push((path, planned));
        }
    }

    // Staging files are created lazily by the first chunk written into them
    // and only grow as data arrives: preallocating every file at full size
    // would reserve the whole game on disk up front (83 GB for a cancelled
    // download in a live test). Existing staging files are kept: their
    // content is reused where it still matches. Chunks are queued front to
    // back within each file, so a file never jumps ahead to a late offset.
    let tasks: Vec<ChunkTask<'_>> = to_build
        .iter()
        .flat_map(|(path, planned)| {
            let mut chunks: Vec<&ManifestChunk> = planned.file.chunks.iter().collect();
            chunks.sort_by_key(|chunk| chunk.offset);
            chunks.into_iter().map(|chunk| ChunkTask {
                depot: planned.depot,
                path: path.as_path(),
                chunk,
                force_download: false,
            })
        })
        .collect();
    let mut progress = Progress {
        bytes_total: tasks.iter().map(|task| u64::from(task.chunk.size)).sum(),
        chunks_total: tasks.len(),
        ..Progress::default()
    };
    on_progress(progress);
    let mut reused_files = BTreeSet::new();
    build(
        depots,
        &plan,
        target,
        source,
        options,
        tasks,
        &mut report,
        &mut progress,
        &mut reused_files,
        &mut on_progress,
    )
    .await?;

    // Give every built file its exact final size: this creates files without
    // chunks (empty or all zeros) and cuts leftovers of an older staging file.
    for (path, planned) in &to_build {
        set_final_size(&staging.join(path), planned.file.size).await?;
    }

    // Verify built files; files that reused local data get one full re-download.
    let mut retry = Vec::new();
    for (path, planned) in &to_build {
        if !hash_matches(&staging.join(path), planned.file).await? {
            if reused_files.contains(path.as_path()) {
                warn!(path = %path.display(), "reused data did not match, downloading the file again");
                retry.push((*path, *planned));
            } else {
                return Err(InstallError::FileHash((*path).clone()));
            }
        }
    }
    if !retry.is_empty() {
        let tasks: Vec<ChunkTask<'_>> = retry
            .iter()
            .flat_map(|(path, planned)| {
                planned.file.chunks.iter().map(|chunk| ChunkTask {
                    depot: planned.depot,
                    path: path.as_path(),
                    chunk,
                    force_download: true,
                })
            })
            .collect();
        progress.bytes_total += tasks
            .iter()
            .map(|task| u64::from(task.chunk.size))
            .sum::<u64>();
        progress.chunks_total += tasks.len();
        build(
            depots,
            &plan,
            target,
            source,
            options,
            tasks,
            &mut report,
            &mut progress,
            &mut BTreeSet::new(),
            &mut on_progress,
        )
        .await?;
        for (path, planned) in &retry {
            if !hash_matches(&staging.join(path), planned.file).await? {
                return Err(InstallError::FileHash((*path).clone()));
            }
        }
    }

    // Commit.
    create_dir(install_dir).await.map_err(as_commit)?;
    for (path, entry) in &plan.entries {
        let destination = install_dir.join(path);
        match entry {
            Planned::Directory => {
                if !destination.is_dir() {
                    create_dir(&destination).await.map_err(as_commit)?;
                    report.directories += 1;
                }
            }
            Planned::File(_) => {}
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
    for (path, planned) in &to_build {
        let destination = install_dir.join(path);
        if let Some(parent) = destination.parent() {
            create_dir(parent).await.map_err(as_commit)?;
        }
        tokio::fs::rename(staging.join(path), &destination)
            .await
            .map_err(commit_error(&destination))?;
        if planned.file.is_executable() {
            make_executable(&destination).await?;
        }
        report.files += 1;
        report.bytes_written += planned.file.size;
    }
    for path in &plan.removed_files {
        let destination = install_dir.join(path);
        match tokio::fs::symlink_metadata(&destination).await {
            Ok(meta) if !meta.is_dir() => {
                tokio::fs::remove_file(&destination)
                    .await
                    .map_err(commit_error(&destination))?;
                report.files_removed += 1;
            }
            _ => {}
        }
    }
    // Deepest first; only empty directories are removed.
    for path in plan.removed_directories.iter().rev() {
        let _ = tokio::fs::remove_dir(install_dir.join(path)).await;
    }
    remove_dir_if_exists(staging).await?;
    debug!(?report, "install finished");
    Ok(report)
}

/// Builds the given chunks into their staging files.
#[allow(clippy::too_many_arguments)]
async fn build<'a>(
    depots: &[DepotDownload],
    plan: &Plan<'a>,
    target: &InstallTarget,
    source: &impl ChunkSource,
    options: InstallOptions,
    tasks: Vec<ChunkTask<'a>>,
    report: &mut InstallReport,
    progress: &mut Progress,
    reused_files: &mut BTreeSet<&'a Path>,
    on_progress: &mut impl FnMut(Progress),
) -> Result<(), InstallError> {
    // An explicit FuturesUnordered of a named async fn instead of
    // `stream::iter(..).map(|t| async move ..).buffer_unordered(..)`: the
    // closure form trips rustc's higher-ranked `Send` inference (rust-lang
    // issue 64552), which makes the whole install future unusable in
    // `tokio::spawn` (the daemon runs installs in a task).
    let concurrency = options.concurrency.max(1);
    let mut pending_tasks = tasks.into_iter();
    let mut in_flight = FuturesUnordered::new();
    loop {
        while in_flight.len() < concurrency {
            let Some(task) = pending_tasks.next() else {
                break;
            };
            in_flight.push(obtain_task(depots, plan, target, source, options, task));
        }
        let Some(result) = in_flight.next().await else {
            break;
        };
        let (task, origin) = result?;
        let staged = target.staging_dir.join(task.path);
        match origin {
            Origin::Staged => {
                report.chunks_reused += 1;
                reused_files.insert(task.path);
            }
            Origin::Reused(data) => {
                write_at(&staged, task.chunk.offset, &data).await?;
                report.chunks_reused += 1;
                reused_files.insert(task.path);
            }
            Origin::Downloaded(data, wire_bytes) => {
                write_at(&staged, task.chunk.offset, &data).await?;
                report.chunks_downloaded += 1;
                report.bytes_downloaded += wire_bytes as u64;
            }
        }
        progress.bytes_done += u64::from(task.chunk.size);
        progress.chunks_done += 1;
        on_progress(*progress);
    }
    Ok(())
}

/// [`obtain_chunk`] paired with its task.
async fn obtain_task<'a>(
    depots: &[DepotDownload],
    plan: &Plan<'_>,
    target: &InstallTarget,
    source: &impl ChunkSource,
    options: InstallOptions,
    task: ChunkTask<'a>,
) -> Result<(ChunkTask<'a>, Origin), InstallError> {
    let origin = obtain_chunk(depots, plan, target, source, options, task).await?;
    Ok((task, origin))
}

/// Finds the chunk locally or downloads it.
async fn obtain_chunk(
    depots: &[DepotDownload],
    plan: &Plan<'_>,
    target: &InstallTarget,
    source: &impl ChunkSource,
    options: InstallOptions,
    task: ChunkTask<'_>,
) -> Result<Origin, InstallError> {
    let chunk = task.chunk;
    if !task.force_download {
        let staged = target.staging_dir.join(task.path);
        if read_matching(&staged, chunk).await.is_some() {
            return Ok(Origin::Staged);
        }
        let mut candidates = Vec::with_capacity(2);
        if let Some((path, offset)) = plan.previous_chunks.get(&chunk.id) {
            candidates.push((target.install_dir.join(path), *offset));
        }
        candidates.push((target.install_dir.join(task.path), chunk.offset));
        for (path, offset) in candidates {
            let at = ManifestChunk { offset, ..*chunk };
            if let Some(data) = read_matching(&path, &at).await {
                return Ok(Origin::Reused(data));
            }
        }
    }
    let depot = &depots[task.depot];
    let (data, wire_bytes) = fetch_verified(source, depot, chunk, options.attempts).await?;
    Ok(Origin::Downloaded(data, wire_bytes))
}

/// Reads `chunk.size` bytes at `chunk.offset` and returns them if size and
/// checksum match the chunk.
async fn read_matching(path: &Path, chunk: &ManifestChunk) -> Option<Vec<u8>> {
    let mut file = tokio::fs::File::open(path).await.ok()?;
    file.seek(SeekFrom::Start(chunk.offset)).await.ok()?;
    let mut data = vec![0; chunk.size as usize];
    file.read_exact(&mut data).await.ok()?;
    (steam_adler32(&data) == chunk.checksum).then_some(data)
}

/// Whether an installed file can be kept as it is.
async fn is_unchanged(
    plan: &Plan<'_>,
    path: &Path,
    file: &ManifestFile,
    destination: &Path,
    options: InstallOptions,
) -> Result<bool, InstallError> {
    let size_matches = tokio::fs::metadata(destination)
        .await
        .is_ok_and(|meta| meta.len() == file.size);
    if !size_matches {
        return Ok(false);
    }
    if options.verify_existing {
        return hash_matches(destination, file).await;
    }
    Ok(match (plan.previous_files.get(path), file.sha_content) {
        (Some(previous), Some(sha)) => previous.sha_content == Some(sha),
        _ => false,
    })
}

/// Whether the file matches its manifest entry: SHA-1 if the manifest has
/// one, otherwise all chunk checksums.
async fn hash_matches(path: &Path, file: &ManifestFile) -> Result<bool, InstallError> {
    match file.sha_content {
        Some(expected) => Ok(sha1_of(path).await? == expected),
        None => {
            for chunk in &file.chunks {
                if read_matching(path, chunk).await.is_none() {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

/// Validates every entry and merges all depots into one plan.
fn plan(depots: &[DepotDownload]) -> Result<Plan<'_>, InstallError> {
    let mut entries = BTreeMap::new();
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
            if entries.insert(path.clone(), entry).is_some() {
                warn!(path = %path.display(), depot = depot.depot_id, "path provided by more than one depot, later depot wins");
            }
        }
    }

    let mut previous_files = HashMap::new();
    let mut previous_chunks = HashMap::new();
    let mut removed_files = Vec::new();
    let mut removed_directories = BTreeSet::new();
    for depot in depots {
        let Some(previous) = &depot.previous else {
            continue;
        };
        if previous.filenames_encrypted {
            return Err(InstallError::NamesEncrypted(depot.depot_id));
        }
        for file in &previous.files {
            // An unsafe name in an old manifest is ignored, never acted on.
            let Ok(path) = install_path(&file.name) else {
                warn!(name = %file.name, "ignoring unsafe name in the previous manifest");
                continue;
            };
            if !entries.contains_key(&path) {
                if file.is_directory() {
                    removed_directories.insert(path.clone());
                } else {
                    removed_files.push(path.clone());
                }
            }
            for chunk in &file.chunks {
                previous_chunks
                    .entry(chunk.id)
                    .or_insert_with(|| (path.clone(), chunk.offset));
            }
            previous_files.insert(path, file);
        }
    }
    Ok(Plan {
        entries,
        previous_files,
        previous_chunks,
        removed_files,
        removed_directories: removed_directories.into_iter().collect(),
    })
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

async fn is_file(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .is_ok_and(|meta| meta.is_file())
}

/// Opens a staging file for writing, creating it (and its directory) if needed.
async fn open_staged(path: &Path) -> Result<tokio::fs::File, InstallError> {
    let mut options = tokio::fs::OpenOptions::new();
    options.create(true).truncate(false).write(true);
    match options.open(path).await {
        Ok(file) => Ok(file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                create_dir(parent).await?;
            }
            options.open(path).await.map_err(staging_error(path))
        }
        Err(error) => Err(staging_error(path)(error)),
    }
}

async fn set_final_size(path: &Path, size: u64) -> Result<(), InstallError> {
    let file = open_staged(path).await?;
    file.set_len(size).await.map_err(staging_error(path))
}

async fn write_at(path: &Path, offset: u64, data: &[u8]) -> Result<(), InstallError> {
    let mut file = open_staged(path).await?;
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
    use crate::chunk::tests::{key, sample, vzstd};
    use crate::crypto;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// In-memory chunk store that counts fetches; `fail_first` makes the
    /// first N fetches fail.
    #[derive(Default)]
    struct MemorySource {
        chunks: HashMap<ChunkId, Vec<u8>>,
        fail_first: Mutex<usize>,
        fetched: AtomicUsize,
    }

    impl MemorySource {
        fn fetched(&self) -> usize {
            self.fetched.swap(0, Ordering::SeqCst)
        }
    }

    impl ChunkSource for MemorySource {
        async fn fetch(
            &self,
            _depot: u32,
            chunk: &ManifestChunk,
            _attempt: usize,
        ) -> Result<Vec<u8>, String> {
            self.fetched.fetch_add(1, Ordering::SeqCst);
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

        fn game(&self) -> PathBuf {
            self.target().install_dir
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Splits `data` into chunks of `chunk_size`, stores the encrypted
    /// chunks in `source` (content-addressed like Steam) and returns the entry.
    fn file(
        name: &str,
        data: &[u8],
        chunk_size: usize,
        flags: u32,
        source: &mut MemorySource,
    ) -> ManifestFile {
        let mut chunks = Vec::new();
        for (index, piece) in data.chunks(chunk_size).enumerate() {
            let id = ChunkId(Sha1::digest(piece).into());
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

    fn manifest(files: Vec<ManifestFile>) -> DepotManifest {
        DepotManifest {
            depot_id: 481,
            manifest_id: 1,
            creation_time: 0,
            filenames_encrypted: false,
            total_size: 0,
            total_compressed_size: 0,
            files,
        }
    }

    fn depot(files: Vec<ManifestFile>) -> DepotDownload {
        DepotDownload {
            depot_id: 481,
            key: key(),
            manifest: manifest(files),
            previous: None,
        }
    }

    fn options() -> InstallOptions {
        InstallOptions {
            concurrency: 4,
            attempts: 3,
            verify_existing: false,
        }
    }

    async fn run(dir: &TempDir, depot: DepotDownload, source: &MemorySource) -> InstallReport {
        install(&[depot], &dir.target(), source, options(), |_| {})
            .await
            .unwrap()
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

        let game = dir.game();
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
        run(&dir, depot(files), &source).await;
        assert!(dir.game().join("a.bin").is_file());
    }

    #[tokio::test]
    async fn corrupt_chunk_leaves_install_dir_untouched() {
        let dir = TempDir::new("corrupt");
        let mut source = MemorySource::default();
        let mut files = vec![file("a.bin", &sample(10_000), 4096, 0, &mut source)];
        files[0].chunks[1].checksum ^= 1;
        let result = install(&[depot(files)], &dir.target(), &source, options(), |_| {}).await;
        assert!(matches!(result, Err(InstallError::Chunk { .. })));
        assert!(!dir.game().exists());
    }

    #[tokio::test]
    async fn wrong_file_hash_is_rejected() {
        let dir = TempDir::new("hash");
        let mut source = MemorySource::default();
        let mut files = vec![file("a.bin", &sample(5_000), 4096, 0, &mut source)];
        files[0].sha_content = Some([0; 20]);
        let result = install(&[depot(files)], &dir.target(), &source, options(), |_| {}).await;
        assert!(matches!(result, Err(InstallError::FileHash(_))));
        assert!(!dir.game().exists());
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
        assert!(!dir.game().exists());
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
        std::fs::create_dir_all(dir.game()).unwrap();
        std::fs::write(dir.game().join("settings.cfg"), b"my settings").unwrap();

        let mut source = MemorySource::default();
        let files = vec![file(
            "settings.cfg",
            b"defaults",
            1024,
            file_flags::USER_CONFIG,
            &mut source,
        )];
        let report = run(&dir, depot(files), &source).await;
        assert_eq!(report.user_config_kept, 1);
        assert_eq!(source.fetched(), 0, "nothing downloaded for a kept file");
        assert_eq!(
            std::fs::read(dir.game().join("settings.cfg")).unwrap(),
            b"my settings"
        );
    }

    #[tokio::test]
    async fn update_downloads_only_changed_chunks_and_removes_old_files() {
        let dir = TempDir::new("update");
        let mut source = MemorySource::default();
        let kept = sample(50_000);
        let changed_v1 = sample(40_000);
        let mut changed_v2 = changed_v1.clone();
        changed_v2[35_000] ^= 0xff; // only the last of four 10 KB chunks differs

        let v1 = vec![
            entry("old", file_flags::DIRECTORY, None),
            file("kept.bin", &kept, 10_000, 0, &mut source),
            file("changed.bin", &changed_v1, 10_000, 0, &mut source),
            file("old\\gone.txt", b"bye", 1024, 0, &mut source),
        ];
        run(&dir, depot(v1.clone()), &source).await;
        assert_eq!(source.fetched(), 5 + 4 + 1);
        let kept_modified = std::fs::metadata(dir.game().join("kept.bin"))
            .unwrap()
            .modified()
            .unwrap();

        let v2 = vec![
            file("kept.bin", &kept, 10_000, 0, &mut source),
            file("changed.bin", &changed_v2, 10_000, 0, &mut source),
            file("new.txt", b"hello", 1024, 0, &mut source),
        ];
        let update = DepotDownload {
            previous: Some(manifest(v1)),
            ..depot(v2)
        };
        let report = run(&dir, update, &source).await;

        assert_eq!(source.fetched(), 2, "one changed chunk and one new file");
        assert_eq!((report.files, report.files_unchanged), (2, 1));
        assert_eq!((report.chunks_downloaded, report.chunks_reused), (2, 3));
        assert_eq!(report.files_removed, 1);
        assert!(
            !dir.game().join("old").exists(),
            "empty old directory removed"
        );
        assert_eq!(
            std::fs::read(dir.game().join("changed.bin")).unwrap(),
            changed_v2
        );
        assert_eq!(
            std::fs::metadata(dir.game().join("kept.bin"))
                .unwrap()
                .modified()
                .unwrap(),
            kept_modified,
            "unchanged file not rewritten"
        );
    }

    #[tokio::test]
    async fn failed_download_does_not_reserve_the_full_size() {
        // Regression: staging files used to be preallocated at full size,
        // so a cancelled 83 GB game kept 83 GB of disk space reserved.
        let dir = TempDir::new("lazy");
        let mut source = MemorySource::default();
        let big = sample(40_000);
        let mut files = vec![
            file("a.bin", &big, 10_000, 0, &mut source),
            file("b.bin", &sample(30_000), 10_000, 0, &mut source),
        ];
        files[0].chunks[2].checksum ^= 1; // the third chunk of a.bin is broken

        let sequential = InstallOptions {
            concurrency: 1,
            attempts: 1,
            verify_existing: false,
        };
        let result = install(&[depot(files)], &dir.target(), &source, sequential, |_| {}).await;
        assert!(matches!(result, Err(InstallError::Chunk { .. })));

        let staging = dir.target().staging_dir;
        let staged = std::fs::metadata(staging.join("a.bin")).unwrap().len();
        assert_eq!(staged, 20_000, "only the two chunks written so far");
        assert!(
            !staging.join("b.bin").exists(),
            "untouched files are not created"
        );
    }

    #[tokio::test]
    async fn chunks_are_written_front_to_back() {
        // A manifest may list a file's chunks in any order; writing the last
        // chunk first would grow the staging file to almost its full size.
        let dir = TempDir::new("order");
        let mut source = MemorySource::default();
        let mut files = vec![file("a.bin", &sample(40_000), 10_000, 0, &mut source)];
        files[0].chunks.reverse();
        files[0].chunks[2].checksum ^= 1; // the chunk at offset 10_000 is broken

        let sequential = InstallOptions {
            concurrency: 1,
            attempts: 1,
            verify_existing: false,
        };
        let result = install(&[depot(files)], &dir.target(), &source, sequential, |_| {}).await;
        assert!(matches!(result, Err(InstallError::Chunk { .. })));

        let staged = std::fs::metadata(dir.target().staging_dir.join("a.bin"))
            .unwrap()
            .len();
        assert_eq!(staged, 10_000, "only the first chunk was written");
    }

    #[tokio::test]
    async fn files_without_chunks_get_their_size() {
        let dir = TempDir::new("zeros");
        let mut zeros = entry("zeros.bin", 0, None);
        zeros.size = 4096;
        zeros.sha_content = Some(Sha1::digest(vec![0u8; 4096]).into());
        run(&dir, depot(vec![zeros]), &MemorySource::default()).await;
        assert_eq!(
            std::fs::read(dir.game().join("zeros.bin")).unwrap(),
            vec![0u8; 4096]
        );
    }

    #[tokio::test]
    async fn resumes_an_interrupted_download() {
        let dir = TempDir::new("resume");
        let mut source = MemorySource::default();
        let data = sample(40_000);
        let files = vec![file("a.bin", &data, 10_000, 0, &mut source)];

        // Simulate an earlier attempt that finished the first two chunks.
        let staged = dir.target().staging_dir.join("a.bin");
        std::fs::create_dir_all(staged.parent().unwrap()).unwrap();
        let mut partial = data[..20_000].to_vec();
        partial.resize(40_000, 0);
        std::fs::write(&staged, &partial).unwrap();

        let report = run(&dir, depot(files), &source).await;
        assert_eq!(source.fetched(), 2);
        assert_eq!(report.chunks_reused, 2);
        assert_eq!(std::fs::read(dir.game().join("a.bin")).unwrap(), data);
    }

    #[tokio::test]
    async fn verify_repairs_only_broken_chunks() {
        let dir = TempDir::new("verify");
        let mut source = MemorySource::default();
        let data = sample(40_000);
        let files = vec![
            file("a.bin", &data, 10_000, 0, &mut source),
            file("b.bin", b"intact", 1024, 0, &mut source),
        ];
        run(&dir, depot(files.clone()), &source).await;
        source.fetched();

        // Damage one chunk of a.bin.
        let mut broken = data.clone();
        broken[25_000] ^= 1;
        std::fs::write(dir.game().join("a.bin"), &broken).unwrap();

        let verify = DepotDownload {
            previous: Some(manifest(files.clone())),
            ..depot(files)
        };
        let options = InstallOptions {
            verify_existing: true,
            ..options()
        };
        let report = install(&[verify], &dir.target(), &source, options, |_| {})
            .await
            .unwrap();
        assert_eq!(source.fetched(), 1, "only the damaged chunk is downloaded");
        assert_eq!((report.files, report.files_unchanged), (1, 1));
        assert_eq!(std::fs::read(dir.game().join("a.bin")).unwrap(), data);
    }

    #[tokio::test]
    async fn without_verify_a_damaged_file_with_same_hash_in_manifest_is_trusted() {
        // Documents the trade-off: a normal update trusts unchanged entries.
        let dir = TempDir::new("trust");
        let mut source = MemorySource::default();
        let files = vec![file("a.bin", b"original", 1024, 0, &mut source)];
        run(&dir, depot(files.clone()), &source).await;
        std::fs::write(dir.game().join("a.bin"), b"ORIGINAL").unwrap();

        let update = DepotDownload {
            previous: Some(manifest(files.clone())),
            ..depot(files)
        };
        let report = run(&dir, update, &source).await;
        assert_eq!(report.files_unchanged, 1);
    }
}
