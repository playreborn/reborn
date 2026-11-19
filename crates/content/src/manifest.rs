//! Depot manifests: the list of files and chunks of one depot version.
//!
//! The CDN serves a manifest as a ZIP archive with one entry. Inside is a
//! sequence of sections, each `magic (u32 LE)`, `length (u32 LE)`, protobuf
//! body, until the end marker:
//!
//! | Magic | Section | Protobuf |
//! | --- | --- | --- |
//! | `0x71F617D0` | payload (files and chunks) | `ContentManifestPayload` |
//! | `0x1F4812BE` | metadata | `ContentManifestMetadata` |
//! | `0x1B81B817` | signature (not verified) | `ContentManifestSignature` |
//! | `0x32C415AB` | end of manifest | none |
//!
//! Encrypted file names are Base64 of `ECB(IV) || CBC(name + NUL)` with the
//! depot key; names use `\` as separator.
//!
//! Sources: SteamKit2 `Types/DepotManifest.cs` (magics, section loop,
//! `DecryptFilenames`, sorting); steamroom-client `src/manifest.rs` (ZIP
//! container of CDN manifests); steamroom `src/depot/manifest.rs` (sorting
//! chunks by offset). The legacy binary (Steam3) manifest format is not
//! supported.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use std::io::Read;
use steam_vent_proto_common::protobuf::Message;
use steam_vent_proto_steam::content_manifest::{
    ContentManifestMetadata, ContentManifestPayload, content_manifest_payload::FileMapping,
};

use crate::chunk::ChunkExpectation;
use crate::crypto::{self, CryptoError, DepotKey};

const MAGIC_PAYLOAD: u32 = 0x71F6_17D0;
const MAGIC_METADATA: u32 = 0x1F48_12BE;
const MAGIC_SIGNATURE: u32 = 0x1B81_B817;
const MAGIC_END: u32 = 0x32C4_15AB;

/// `EDepotFileFlag` bits (SteamKit2 `Resources/SteamLanguage/enums.steamd`).
pub mod file_flags {
    /// File is user configuration and must not be overwritten on update.
    pub const USER_CONFIG: u32 = 1;
    /// Versioned user configuration.
    pub const VERSIONED_USER_CONFIG: u32 = 2;
    /// File content is encrypted.
    pub const ENCRYPTED: u32 = 4;
    /// Read-only file.
    pub const READ_ONLY: u32 = 8;
    /// Hidden file.
    pub const HIDDEN: u32 = 16;
    /// Executable (sets the executable bit on Unix).
    pub const EXECUTABLE: u32 = 32;
    /// Entry is a directory, not a file.
    pub const DIRECTORY: u32 = 64;
    /// Custom executable.
    pub const CUSTOM_EXECUTABLE: u32 = 128;
    /// Install script.
    pub const INSTALL_SCRIPT: u32 = 256;
    /// Entry is a symbolic link (see `link_target`).
    pub const SYMLINK: u32 = 512;
}

/// SHA-1 that identifies a chunk on the CDN.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkId(pub [u8; 20]);

impl ChunkId {
    /// Lowercase hex, as used in CDN chunk URLs.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

impl std::fmt::Debug for ChunkId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ChunkId({})", self.to_hex())
    }
}

/// One chunk of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManifestChunk {
    /// CDN identifier.
    pub id: ChunkId,
    /// Position of the chunk inside the file.
    pub offset: u64,
    /// Uncompressed size.
    pub size: u32,
    /// Size on the CDN (encrypted and compressed).
    pub compressed_size: u32,
    /// Zero-seeded Adler-32 of the uncompressed data.
    pub checksum: u32,
}

impl ManifestChunk {
    /// What [`crate::chunk::process_chunk`] must verify.
    pub fn expectation(&self) -> ChunkExpectation {
        ChunkExpectation {
            size: self.size,
            checksum: self.checksum,
        }
    }
}

/// One file, directory or symlink of a depot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManifestFile {
    /// Path as stored by Steam, with `\` separators. Untrusted: convert with
    /// [`crate::path::install_path`] before touching the file system.
    pub name: String,
    /// Total size in bytes.
    pub size: u64,
    /// [`file_flags`] bit set.
    pub flags: u32,
    /// SHA-1 of the whole file content, if present.
    pub sha_content: Option<[u8; 20]>,
    /// Chunks, sorted by offset.
    pub chunks: Vec<ManifestChunk>,
    /// Target of a symlink. Untrusted.
    pub link_target: Option<String>,
}

impl ManifestFile {
    /// Whether this entry is a directory.
    pub fn is_directory(&self) -> bool {
        self.flags & file_flags::DIRECTORY != 0
    }

    /// Whether this entry is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.flags & file_flags::SYMLINK != 0
    }

    /// Whether this file should be executable.
    pub fn is_executable(&self) -> bool {
        self.flags & file_flags::EXECUTABLE != 0
    }
}

/// A parsed depot manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepotManifest {
    /// Depot ID.
    pub depot_id: u32,
    /// Manifest ID ("gid").
    pub manifest_id: u64,
    /// Creation time, Unix seconds.
    pub creation_time: u32,
    /// Whether [`ManifestFile::name`] is still encrypted, see
    /// [`DepotManifest::decrypt_filenames`].
    pub filenames_encrypted: bool,
    /// Total size of all files on disk.
    pub total_size: u64,
    /// Total size of all chunks on the CDN.
    pub total_compressed_size: u64,
    /// Files, directories and symlinks.
    pub files: Vec<ManifestFile>,
}

/// Parsing or decrypting a manifest failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ManifestError {
    /// The ZIP container could not be read.
    #[error("manifest archive: {0}")]
    Archive(String),
    /// The section stream ends early.
    #[error("manifest is truncated")]
    Truncated,
    /// A section with an unknown magic value.
    #[error("unknown manifest section {0:#010x}")]
    UnknownSection(u32),
    /// Payload or metadata section is missing.
    #[error("manifest has no {0} section")]
    MissingSection(&'static str),
    /// A protobuf section could not be decoded.
    #[error("manifest {0} section: {1}")]
    Protobuf(&'static str, String),
    /// A hash has the wrong length.
    #[error("manifest contains a hash with {0} bytes instead of 20")]
    BadHash(usize),
    /// An encrypted file name is not valid Base64.
    #[error("encrypted file name is not valid Base64")]
    FilenameBase64,
    /// An encrypted file name could not be decrypted.
    #[error("file name decryption: {0}")]
    FilenameDecrypt(CryptoError),
    /// A decrypted file name is not UTF-8.
    #[error("decrypted file name is not UTF-8")]
    FilenameUtf8,
}

impl DepotManifest {
    /// Parses a manifest as served by the CDN (ZIP) or already unpacked.
    pub fn parse(data: &[u8]) -> Result<Self, ManifestError> {
        if data.starts_with(b"PK\x03\x04") {
            Self::parse_sections(&unzip(data)?)
        } else {
            Self::parse_sections(data)
        }
    }

    fn parse_sections(data: &[u8]) -> Result<Self, ManifestError> {
        let mut rest = data;
        let mut payload = None;
        let mut metadata = None;
        loop {
            let magic = take_u32(&mut rest)?;
            if magic == MAGIC_END {
                break;
            }
            let length = take_u32(&mut rest)? as usize;
            if rest.len() < length {
                return Err(ManifestError::Truncated);
            }
            let (body, tail) = rest.split_at(length);
            rest = tail;
            match magic {
                MAGIC_PAYLOAD => {
                    payload = Some(
                        ContentManifestPayload::parse_from_bytes(body)
                            .map_err(|e| ManifestError::Protobuf("payload", e.to_string()))?,
                    );
                }
                MAGIC_METADATA => {
                    metadata = Some(
                        ContentManifestMetadata::parse_from_bytes(body)
                            .map_err(|e| ManifestError::Protobuf("metadata", e.to_string()))?,
                    );
                }
                MAGIC_SIGNATURE => {}
                other => return Err(ManifestError::UnknownSection(other)),
            }
        }
        let payload = payload.ok_or(ManifestError::MissingSection("payload"))?;
        let metadata = metadata.ok_or(ManifestError::MissingSection("metadata"))?;

        let files = payload
            .mappings
            .into_iter()
            .map(file_from_proto)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            depot_id: metadata.depot_id.unwrap_or(0),
            manifest_id: metadata.gid_manifest.unwrap_or(0),
            creation_time: metadata.creation_time.unwrap_or(0),
            filenames_encrypted: metadata.filenames_encrypted.unwrap_or(false),
            total_size: metadata.cb_disk_original.unwrap_or(0),
            total_compressed_size: metadata.cb_disk_compressed.unwrap_or(0),
            files,
        })
    }

    /// Decrypts all file names and link targets with the depot key, then
    /// sorts the files by name (case-insensitive), as Steam does.
    /// Does nothing if the names are not encrypted.
    pub fn decrypt_filenames(&mut self, key: &DepotKey) -> Result<(), ManifestError> {
        if !self.filenames_encrypted {
            return Ok(());
        }
        for file in &mut self.files {
            file.name = decrypt_name(&file.name, key)?;
            if let Some(target) = &file.link_target {
                file.link_target = Some(decrypt_name(target, key)?);
            }
        }
        self.files
            .sort_by_cached_key(|file| file.name.to_lowercase());
        self.filenames_encrypted = false;
        Ok(())
    }

    /// Sum of the uncompressed chunk sizes, i.e. what has to be downloaded
    /// and written.
    pub fn content_size(&self) -> u64 {
        self.files
            .iter()
            .flat_map(|file| &file.chunks)
            .map(|chunk| u64::from(chunk.size))
            .sum()
    }
}

fn unzip(data: &[u8]) -> Result<Vec<u8>, ManifestError> {
    let archive_error = |e: zip::result::ZipError| ManifestError::Archive(e.to_string());
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(archive_error)?;
    let mut entry = archive.by_index(0).map_err(archive_error)?;
    let mut out = Vec::new();
    entry
        .read_to_end(&mut out)
        .map_err(|e| ManifestError::Archive(e.to_string()))?;
    Ok(out)
}

fn take_u32(rest: &mut &[u8]) -> Result<u32, ManifestError> {
    let (bytes, tail) = rest
        .split_first_chunk::<4>()
        .ok_or(ManifestError::Truncated)?;
    *rest = tail;
    Ok(u32::from_le_bytes(*bytes))
}

fn hash(bytes: Option<Vec<u8>>) -> Result<Option<[u8; 20]>, ManifestError> {
    match bytes {
        None => Ok(None),
        Some(bytes) if bytes.is_empty() => Ok(None),
        Some(bytes) => <[u8; 20]>::try_from(bytes.as_slice())
            .map(Some)
            .map_err(|_| ManifestError::BadHash(bytes.len())),
    }
}

fn file_from_proto(mapping: FileMapping) -> Result<ManifestFile, ManifestError> {
    let mut chunks = mapping
        .chunks
        .into_iter()
        .map(|chunk| {
            Ok(ManifestChunk {
                id: ChunkId(hash(chunk.sha)?.unwrap_or_default()),
                offset: chunk.offset.unwrap_or(0),
                size: chunk.cb_original.unwrap_or(0),
                compressed_size: chunk.cb_compressed.unwrap_or(0),
                checksum: chunk.crc.unwrap_or(0),
            })
        })
        .collect::<Result<Vec<_>, ManifestError>>()?;
    chunks.sort_by_key(|chunk| chunk.offset);
    Ok(ManifestFile {
        name: mapping.filename.unwrap_or_default(),
        size: mapping.size.unwrap_or(0),
        flags: mapping.flags.unwrap_or(0),
        sha_content: hash(mapping.sha_content)?,
        chunks,
        link_target: mapping.linktarget.filter(|target| !target.is_empty()),
    })
}

fn decrypt_name(encoded: &str, key: &DepotKey) -> Result<String, ManifestError> {
    let compact: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
    let encrypted = BASE64
        .decode(compact)
        .map_err(|_| ManifestError::FilenameBase64)?;
    let mut plain = crypto::decrypt(&encrypted, key).map_err(ManifestError::FilenameDecrypt)?;
    if plain.last() == Some(&0) {
        plain.pop();
    }
    String::from_utf8(plain).map_err(|_| ManifestError::FilenameUtf8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::tests::key;
    use std::io::Write;
    use steam_vent_proto_steam::content_manifest::content_manifest_payload::file_mapping::ChunkData;

    fn section(out: &mut Vec<u8>, magic: u32, body: &[u8]) {
        out.extend_from_slice(&magic.to_le_bytes());
        out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
        out.extend_from_slice(body);
    }

    fn chunk(offset: u64, size: u32, id: u8) -> ChunkData {
        ChunkData {
            sha: Some(vec![id; 20]),
            crc: Some(0xdead_beef),
            offset: Some(offset),
            cb_original: Some(size),
            cb_compressed: Some(size / 2),
            ..ChunkData::default()
        }
    }

    fn manifest_bytes(names: &[&str], encrypted: bool) -> Vec<u8> {
        let payload = ContentManifestPayload {
            mappings: names
                .iter()
                .map(|name| FileMapping {
                    filename: Some((*name).to_owned()),
                    size: Some(300),
                    flags: Some(file_flags::EXECUTABLE),
                    sha_content: Some(vec![9; 20]),
                    // Deliberately out of order: the parser sorts by offset.
                    chunks: vec![chunk(200, 100, 2), chunk(0, 200, 1)],
                    ..FileMapping::default()
                })
                .collect(),
            ..ContentManifestPayload::default()
        };
        let metadata = ContentManifestMetadata {
            depot_id: Some(481),
            gid_manifest: Some(1234567890123),
            creation_time: Some(1_700_000_000),
            filenames_encrypted: Some(encrypted),
            cb_disk_original: Some(600),
            cb_disk_compressed: Some(300),
            ..ContentManifestMetadata::default()
        };
        let mut out = Vec::new();
        section(&mut out, MAGIC_PAYLOAD, &payload.write_to_bytes().unwrap());
        section(
            &mut out,
            MAGIC_METADATA,
            &metadata.write_to_bytes().unwrap(),
        );
        section(&mut out, MAGIC_SIGNATURE, &[]);
        out.extend_from_slice(&MAGIC_END.to_le_bytes());
        out
    }

    fn zipped(data: &[u8]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut cursor);
        writer
            .start_file("z", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data).unwrap();
        writer.finish().unwrap();
        cursor.into_inner()
    }

    fn encrypted_name(name: &str) -> String {
        let mut plain = name.as_bytes().to_vec();
        plain.push(0);
        BASE64.encode(crypto::encrypt(&plain, &key(), [7; 16]))
    }

    #[test]
    fn parses_zipped_and_raw_manifests() {
        let raw = manifest_bytes(&["game.exe", "data\\level1.pak"], false);
        for data in [raw.clone(), zipped(&raw)] {
            let manifest = DepotManifest::parse(&data).unwrap();
            assert_eq!(manifest.depot_id, 481);
            assert_eq!(manifest.manifest_id, 1234567890123);
            assert!(!manifest.filenames_encrypted);
            assert_eq!(manifest.files.len(), 2);
            assert_eq!(manifest.content_size(), 600);

            let file = &manifest.files[1];
            assert_eq!(file.name, "data\\level1.pak");
            assert!(file.is_executable() && !file.is_directory() && !file.is_symlink());
            assert_eq!(file.sha_content, Some([9; 20]));
            let offsets: Vec<_> = file.chunks.iter().map(|c| c.offset).collect();
            assert_eq!(offsets, [0, 200]);
            assert_eq!(file.chunks[0].id.to_hex(), "01".repeat(20));
            assert_eq!(file.chunks[0].expectation().size, 200);
        }
    }

    #[test]
    fn decrypts_and_sorts_file_names() {
        let names = [encrypted_name("Zeta.txt"), encrypted_name("bin\\Alpha.dll")];
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut manifest = DepotManifest::parse(&manifest_bytes(&names, true)).unwrap();
        assert!(manifest.filenames_encrypted);

        manifest.decrypt_filenames(&key()).unwrap();
        assert!(!manifest.filenames_encrypted);
        let decrypted: Vec<_> = manifest.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(decrypted, ["bin\\Alpha.dll", "Zeta.txt"]);

        // A second call is a no-op.
        manifest.decrypt_filenames(&key()).unwrap();
    }

    #[test]
    fn wrong_key_fails_to_decrypt_names() {
        let names = [encrypted_name("a.txt")];
        let mut manifest =
            DepotManifest::parse(&manifest_bytes(&[names[0].as_str()], true)).unwrap();
        let other = DepotKey::from_slice(&[1; 32]).unwrap();
        assert!(manifest.decrypt_filenames(&other).is_err());
    }

    #[test]
    fn rejects_broken_manifests() {
        let raw = manifest_bytes(&["a"], false);
        assert!(matches!(
            DepotManifest::parse(&raw[..raw.len() - 6]),
            Err(ManifestError::Truncated)
        ));

        let mut unknown = Vec::new();
        section(&mut unknown, 0x1234_5678, &[]);
        assert!(matches!(
            DepotManifest::parse(&unknown),
            Err(ManifestError::UnknownSection(0x1234_5678))
        ));

        let mut no_payload = Vec::new();
        section(&mut no_payload, MAGIC_METADATA, &[]);
        no_payload.extend_from_slice(&MAGIC_END.to_le_bytes());
        assert!(matches!(
            DepotManifest::parse(&no_payload),
            Err(ManifestError::MissingSection("payload"))
        ));
    }
}
