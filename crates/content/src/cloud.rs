//! Downloading Steam Cloud files into a folder (ADR 0032).
//!
//! File names and download links come from the network. A name never
//! leads outside the folder, existing files are never overwritten, nothing
//! unverified is written, and links never appear in errors or logs.
//!
//! Valve's `ICloudService` documentation and ValvePython/steamctl
//! (`steamctl/commands/cloud/gcmds.py`, MIT) download a cloud file with a
//! plain GET on the `url` of `EnumerateUserFiles`.

use std::io::Write;
use std::path::{Component, Path, PathBuf};

use sha1::{Digest, Sha1};

use crate::cdn::CdnClient;
use crate::path::{PathError, install_path};

/// Suffix of a file while it is being written.
const PART_SUFFIX: &str = ".reborn-part";

/// One cloud file to download, as listed by Steam.
#[derive(Clone, Copy)]
pub struct CloudFileRef<'a> {
    /// Name as stored by Steam (untrusted).
    pub name: &'a str,
    /// Download link. Never log it.
    pub url: &'a str,
    /// Size Steam listed, in bytes.
    pub size: u32,
    /// SHA-1 Steam listed, 40 hex digits.
    pub sha1: &'a str,
}

impl std::fmt::Debug for CloudFileRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudFileRef")
            .field("name", &self.name)
            .field("url", &"<redacted>")
            .field("size", &self.size)
            .field("sha1", &self.sha1)
            .finish()
    }
}

/// What happened to a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloudOutcome {
    /// Downloaded, verified and written to this path.
    Written(PathBuf),
    /// A file with the same content was already at this path.
    AlreadyThere(PathBuf),
}

/// Why a file was not downloaded. Nothing was written in any case.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CloudDownloadError {
    /// The name could lead outside the folder or is unsafe.
    #[error("unsafe file name: {0}")]
    Name(#[from] PathError),
    /// Steam's SHA-1 is not 40 hex digits.
    #[error("Steam's SHA-1 `{0}` is not 40 hex digits")]
    BadSha(String),
    /// Network or TLS error, or timeout. The URL is removed.
    #[error("download failed: {0}")]
    Request(reqwest::Error),
    /// The server answered with an HTTP error status.
    #[error("download answered with HTTP {0}")]
    Status(u16),
    /// The body is larger than the size Steam listed.
    #[error("download is larger than the listed {expected} bytes")]
    TooLarge {
        /// The listed size.
        expected: u32,
    },
    /// The body does not match Steam's SHA-1.
    #[error(
        "downloaded content does not match Steam's SHA-1 ({received} bytes received, {expected} listed)"
    )]
    Mismatch {
        /// Bytes received.
        received: usize,
        /// The listed size.
        expected: u32,
    },
    /// A file with other content is already there.
    #[error("{0} already exists with other content; not overwritten")]
    Exists(PathBuf),
    /// A path component below the folder is a symlink.
    #[error("{0} is a symlink; not followed")]
    Symlink(PathBuf),
    /// Reading or writing a file failed.
    #[error("cannot write {path}: {source}")]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
}

/// The relative path for a cloud file name. A leading root token such as
/// `%WinAppDataLocal%` becomes a folder of that name; everything else must
/// pass [`install_path`].
pub fn cloud_path(name: &str) -> Result<PathBuf, PathError> {
    if let Some(rest) = name.strip_prefix('%')
        && let Some(end) = rest.find('%')
    {
        let root = &name[..end + 2];
        let after = rest[end + 1..]
            .strip_prefix(['/', '\\'])
            .unwrap_or(&rest[end + 1..]);
        let mut path = install_path(root)?;
        path.push(install_path(after)?);
        return Ok(path);
    }
    install_path(name)
}

/// Downloads one file into `folder` (see the module documentation).
pub async fn download_cloud_file(
    client: &CdnClient,
    folder: &Path,
    file: CloudFileRef<'_>,
) -> Result<CloudOutcome, CloudDownloadError> {
    let relative = cloud_path(file.name)?;
    download_cloud_file_into(client, folder, &relative, file).await
}

/// Downloads one file to `base`/`relative` (ADR 0034), with the same
/// rules as [`download_cloud_file`]. `relative` must consist of plain
/// folder and file names only (as from [`install_path`]); anything else is
/// refused.
pub async fn download_cloud_file_into(
    client: &CdnClient,
    folder: &Path,
    relative: &Path,
    file: CloudFileRef<'_>,
) -> Result<CloudOutcome, CloudDownloadError> {
    check_plain(relative)?;
    let relative = relative.to_path_buf();
    let expected = parse_sha1(file.sha1)?;
    let destination = folder.join(&relative);
    check_no_symlinks(folder, &relative)?;
    if let Some(outcome) = existing(&destination, expected)? {
        return Ok(outcome);
    }
    let body = fetch(client, file.url, file.size).await?;
    let folder = folder.to_path_buf();
    let listed = file.size;
    tokio::task::spawn_blocking(move || store(&folder, &relative, &body, expected, listed))
        .await
        .map_err(|error| CloudDownloadError::Io {
            path: destination,
            source: std::io::Error::other(error),
        })?
}

/// Accepts only a non-empty path of plain folder and file names.
fn check_plain(relative: &Path) -> Result<(), CloudDownloadError> {
    let plain = relative.components().next().is_some()
        && relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if plain {
        Ok(())
    } else {
        Err(PathError::Traversal(relative.display().to_string()).into())
    }
}

/// GET with a size limit; errors never carry the URL.
async fn fetch(client: &CdnClient, url: &str, limit: u32) -> Result<Vec<u8>, CloudDownloadError> {
    let request = |error: reqwest::Error| CloudDownloadError::Request(error.without_url());
    let mut response = client.http.get(url).send().await.map_err(request)?;
    let status = response.status();
    if !status.is_success() {
        return Err(CloudDownloadError::Status(status.as_u16()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(request)? {
        if body.len() + chunk.len() > limit as usize {
            return Err(CloudDownloadError::TooLarge { expected: limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Verifies `body` and writes it below `folder`: first as a part file,
/// then renamed. Never replaces an existing file.
fn store(
    folder: &Path,
    relative: &Path,
    body: &[u8],
    expected: [u8; 20],
    listed: u32,
) -> Result<CloudOutcome, CloudDownloadError> {
    if <[u8; 20]>::from(Sha1::digest(body)) != expected {
        return Err(CloudDownloadError::Mismatch {
            received: body.len(),
            expected: listed,
        });
    }
    let destination = folder.join(relative);
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| CloudDownloadError::Io { path, source }
    };
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(io(parent))?;
    }
    // Checked again: folders may have been created in the meantime.
    check_no_symlinks(folder, relative)?;
    if let Some(outcome) = existing(&destination, expected)? {
        return Ok(outcome);
    }
    let mut part_name = destination.as_os_str().to_owned();
    part_name.push(PART_SUFFIX);
    let part = PathBuf::from(part_name);
    let mut file = std::fs::File::create(&part).map_err(io(&part))?;
    file.write_all(body).map_err(io(&part))?;
    file.sync_all().map_err(io(&part))?;
    drop(file);
    if std::fs::symlink_metadata(&destination).is_ok() {
        let _ = std::fs::remove_file(&part);
        return Err(CloudDownloadError::Exists(destination));
    }
    std::fs::rename(&part, &destination).map_err(io(&destination))?;
    Ok(CloudOutcome::Written(destination))
}

/// `Some` if something is already at `path`: the same content is fine,
/// anything else is an error.
fn existing(path: &Path, expected: [u8; 20]) -> Result<Option<CloudOutcome>, CloudDownloadError> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CloudDownloadError::Io {
            path: path.to_path_buf(),
            source,
        }),
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(CloudDownloadError::Symlink(path.to_path_buf()))
        }
        Ok(metadata) if !metadata.is_file() => Err(CloudDownloadError::Exists(path.to_path_buf())),
        Ok(_) => {
            let content = std::fs::read(path).map_err(|source| CloudDownloadError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            if <[u8; 20]>::from(Sha1::digest(&content)) == expected {
                Ok(Some(CloudOutcome::AlreadyThere(path.to_path_buf())))
            } else {
                Err(CloudDownloadError::Exists(path.to_path_buf()))
            }
        }
    }
}

/// Fails if any existing component of `relative` below `folder` is a
/// symlink.
fn check_no_symlinks(folder: &Path, relative: &Path) -> Result<(), CloudDownloadError> {
    let mut path = folder.to_path_buf();
    for component in relative.components() {
        path.push(component);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(CloudDownloadError::Symlink(path));
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    Ok(())
}

fn parse_sha1(hex: &str) -> Result<[u8; 20], CloudDownloadError> {
    let bad = || CloudDownloadError::BadSha(hex.to_owned());
    if hex.len() != 40 || !hex.is_ascii() {
        return Err(bad());
    }
    let mut out = [0u8; 20];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).map_err(|_| bad())?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-cloud-{name}-{}-{}",
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

    fn sha(data: &[u8]) -> [u8; 20] {
        Sha1::digest(data).into()
    }

    #[test]
    fn names_become_safe_relative_paths() {
        assert_eq!(
            cloud_path("MainPlayerData.json").unwrap(),
            PathBuf::from("MainPlayerData.json")
        );
        assert_eq!(
            cloud_path("%WinAppDataLocal%Game/save1.sav").unwrap(),
            Path::new("%WinAppDataLocal%")
                .join("Game")
                .join("save1.sav")
        );
        assert_eq!(
            cloud_path("%WinMyDocuments%/My Games/a.sav").unwrap(),
            Path::new("%WinMyDocuments%").join("My Games").join("a.sav")
        );
        for bad in [
            "",
            "../evil",
            "/etc/passwd",
            "C:\\evil",
            "saves/../../evil",
            "%WinAppDataLocal%../evil",
            "%WinAppDataLocal%",
            "NUL",
        ] {
            assert!(cloud_path(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn parses_sha1() {
        assert_eq!(
            parse_sha1("8fd5b59589120ebdf6928e9b778a40662b1660e8").unwrap()[0],
            0x8f
        );
        assert!(parse_sha1("8fd5").is_err());
        assert!(parse_sha1(&"zz".repeat(20)).is_err());
        assert!(parse_sha1(&"ä".repeat(20)).is_err());
    }

    #[test]
    fn writes_verified_content() {
        let dir = TempDir::new("write");
        let body = b"save data";
        let relative = Path::new("sub").join("save.json");
        let outcome = store(&dir.0, &relative, body, sha(body), 9).unwrap();
        let path = dir.0.join(&relative);
        assert_eq!(outcome, CloudOutcome::Written(path.clone()));
        assert_eq!(std::fs::read(&path).unwrap(), body);
        let mut part = path.as_os_str().to_owned();
        part.push(PART_SUFFIX);
        assert!(!PathBuf::from(part).exists(), "part file renamed");
        // The same content again is left alone.
        assert_eq!(
            store(&dir.0, &relative, body, sha(body), 9).unwrap(),
            CloudOutcome::AlreadyThere(path)
        );
    }

    #[test]
    fn never_writes_unverified_content() {
        let dir = TempDir::new("mismatch");
        let relative = Path::new("save.json");
        let error = store(&dir.0, relative, b"other", sha(b"save data"), 9).unwrap_err();
        assert!(matches!(
            error,
            CloudDownloadError::Mismatch {
                received: 5,
                expected: 9
            }
        ));
        assert!(!dir.0.join(relative).exists());
    }

    #[test]
    fn never_overwrites_other_content() {
        let dir = TempDir::new("exists");
        let relative = Path::new("save.json");
        std::fs::write(dir.0.join(relative), b"my local save").unwrap();
        let body = b"cloud save";
        let error = store(&dir.0, relative, body, sha(body), 10).unwrap_err();
        assert!(matches!(error, CloudDownloadError::Exists(_)));
        assert_eq!(
            std::fs::read(dir.0.join(relative)).unwrap(),
            b"my local save"
        );
    }

    #[cfg(unix)]
    #[test]
    fn never_follows_symlinks() {
        let dir = TempDir::new("symlink");
        let outside = TempDir::new("outside");
        std::os::unix::fs::symlink(&outside.0, dir.0.join("sub")).unwrap();
        let body = b"cloud save";
        let error = store(
            &dir.0,
            &Path::new("sub").join("save.json"),
            body,
            sha(body),
            10,
        )
        .unwrap_err();
        assert!(matches!(error, CloudDownloadError::Symlink(_)));
        assert!(!outside.0.join("save.json").exists());
    }

    #[test]
    fn only_plain_relative_paths_are_accepted() {
        assert!(check_plain(&Path::new("Studio312").join("save.json")).is_ok());
        for bad in [
            PathBuf::new(),
            PathBuf::from(".."),
            Path::new("a").join("..").join("b"),
            std::env::temp_dir(),
        ] {
            assert!(
                matches!(check_plain(&bad), Err(CloudDownloadError::Name(_))),
                "{}",
                bad.display()
            );
        }
    }

    #[test]
    fn debug_hides_the_link() {
        let file = CloudFileRef {
            name: "save.json",
            url: "https://example.invalid/ugc/1/secret/",
            size: 3,
            sha1: "8fd5b59589120ebdf6928e9b778a40662b1660e8",
        };
        let printed = format!("{file:?}");
        assert!(!printed.contains("secret"), "{printed}");
    }
}
