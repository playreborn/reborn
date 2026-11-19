//! Depot chunks: decrypt, decompress, verify.
//!
//! A chunk on the CDN is encrypted with the depot key (see [`DepotKey`])
//! and, once decrypted, compressed in one of three containers, recognised by
//! their first bytes. Anything else is rejected.
//!
//! | Start | Container | Layout |
//! | --- | --- | --- |
//! | `VSZa` | Valve Zstd | magic(4) crc32(4) zstd-data footer: crc32(4) size(4) unknown(4) `zsv` |
//! | `VZa` | Valve LZMA | magic(3) crc-or-time(4) lzma-props(5) lzma-data footer: crc32(4) size(4) `zv` |
//! | `PK\x03\x04` | ZIP | one entry with the data |
//!
//! Afterwards the length must equal the manifest's uncompressed size and the
//! zero-seeded Adler-32 must equal the manifest's checksum.
//!
//! Sources: SteamKit2 `Steam/CDN/DepotChunk.cs` (dispatch and checks),
//! `Util/VZstdUtil.cs`, `Util/VZipUtil.cs` (container layouts); steamroom
//! `src/depot/chunk.rs` (raw LZMA stream reconstruction for `lzma-rs`).

use std::io::Read;

use crate::checksum::steam_adler32;
use crate::crypto::{self, CryptoError, DepotKey};

const VZSTD_MAGIC: &[u8] = b"VSZa";
const VZSTD_HEADER: usize = 8;
const VZSTD_FOOTER: usize = 15;
const VZIP_MAGIC: &[u8] = b"VZa";
const VZIP_HEADER: usize = 7;
const VZIP_PROPS: usize = 5;
const VZIP_FOOTER: usize = 10;
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";

/// Everything the manifest says about a chunk that is needed to check it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkExpectation {
    /// Uncompressed size in bytes.
    pub size: u32,
    /// Zero-seeded Adler-32 of the uncompressed data.
    pub checksum: u32,
}

/// Processing a chunk failed. The chunk must be downloaded again.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ChunkError {
    /// Decryption failed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    /// The decrypted data does not start with a known container.
    #[error("unknown chunk compression (first bytes {0:02x?})")]
    UnknownCompression(Vec<u8>),
    /// A container is truncated or its header/footer is wrong.
    #[error("malformed {0} container")]
    Malformed(&'static str),
    /// The decompressor reported an error.
    #[error("decompression failed: {0}")]
    Decompress(String),
    /// Wrong length after decompression.
    #[error("chunk has {actual} bytes, expected {expected}")]
    SizeMismatch {
        /// From the manifest.
        expected: u32,
        /// Actually decompressed.
        actual: usize,
    },
    /// Wrong checksum after decompression.
    #[error("chunk checksum is {actual:#010x}, expected {expected:#010x}")]
    ChecksumMismatch {
        /// From the manifest.
        expected: u32,
        /// Of the decompressed data.
        actual: u32,
    },
}

/// Decrypts, decompresses and verifies one chunk as downloaded from the CDN.
pub fn process_chunk(
    data: &[u8],
    key: &DepotKey,
    expected: ChunkExpectation,
) -> Result<Vec<u8>, ChunkError> {
    let decrypted = crypto::decrypt(data, key)?;
    let output = decompress(&decrypted, expected.size)?;
    if output.len() != expected.size as usize {
        return Err(ChunkError::SizeMismatch {
            expected: expected.size,
            actual: output.len(),
        });
    }
    let actual = steam_adler32(&output);
    if actual != expected.checksum {
        return Err(ChunkError::ChecksumMismatch {
            expected: expected.checksum,
            actual,
        });
    }
    Ok(output)
}

fn decompress(data: &[u8], size: u32) -> Result<Vec<u8>, ChunkError> {
    if data.starts_with(VZSTD_MAGIC) {
        decompress_vzstd(data, size)
    } else if data.starts_with(VZIP_MAGIC) {
        decompress_vzip(data, size)
    } else if data.starts_with(ZIP_MAGIC) {
        decompress_zip(data)
    } else {
        Err(ChunkError::UnknownCompression(
            data.iter().take(4).copied().collect(),
        ))
    }
}

fn decompress_vzstd(data: &[u8], size: u32) -> Result<Vec<u8>, ChunkError> {
    if data.len() < VZSTD_HEADER + VZSTD_FOOTER || !data.ends_with(b"zsv") {
        return Err(ChunkError::Malformed("VZstd"));
    }
    let compressed = &data[VZSTD_HEADER..data.len() - VZSTD_FOOTER];
    zstd::bulk::decompress(compressed, size as usize)
        .map_err(|error| ChunkError::Decompress(error.to_string()))
}

fn decompress_vzip(data: &[u8], size: u32) -> Result<Vec<u8>, ChunkError> {
    if data.len() < VZIP_HEADER + VZIP_PROPS + VZIP_FOOTER || !data.ends_with(b"zv") {
        return Err(ChunkError::Malformed("VZip"));
    }
    let props = &data[VZIP_HEADER..VZIP_HEADER + VZIP_PROPS];
    let compressed = &data[VZIP_HEADER + VZIP_PROPS..data.len() - VZIP_FOOTER];

    // lzma-rs expects a classic .lzma stream: props(5), size(u64 LE), data.
    let mut stream = Vec::with_capacity(13 + compressed.len());
    stream.extend_from_slice(props);
    stream.extend_from_slice(&u64::from(size).to_le_bytes());
    stream.extend_from_slice(compressed);

    let mut output = Vec::with_capacity(size as usize);
    lzma_rs::lzma_decompress(&mut stream.as_slice(), &mut output)
        .map_err(|error| ChunkError::Decompress(error.to_string()))?;
    Ok(output)
}

fn decompress_zip(data: &[u8]) -> Result<Vec<u8>, ChunkError> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(data))
        .map_err(|error| ChunkError::Decompress(error.to_string()))?;
    let mut entry = archive
        .by_index(0)
        .map_err(|error| ChunkError::Decompress(error.to_string()))?;
    let mut output = Vec::new();
    entry
        .read_to_end(&mut output)
        .map_err(|error| ChunkError::Decompress(error.to_string()))?;
    Ok(output)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) fn key() -> DepotKey {
        DepotKey::from_slice(&[0x5a; 32]).unwrap()
    }

    pub(crate) fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| ((i * 7 + i / 13) % 256) as u8).collect()
    }

    fn expectation(data: &[u8]) -> ChunkExpectation {
        ChunkExpectation {
            size: u32::try_from(data.len()).unwrap(),
            checksum: steam_adler32(data),
        }
    }

    pub(crate) fn vzstd(data: &[u8]) -> Vec<u8> {
        let mut out = b"VSZa".to_vec();
        out.extend_from_slice(&[0; 4]);
        out.extend(zstd::bulk::compress(data, 3).unwrap());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(b"zsv");
        out
    }

    fn vzip(data: &[u8]) -> Vec<u8> {
        let mut lzma = Vec::new();
        let options = lzma_rs::compress::Options {
            unpacked_size: lzma_rs::compress::UnpackedSize::WriteToHeader(Some(data.len() as u64)),
        };
        lzma_rs::lzma_compress_with_options(&mut &data[..], &mut lzma, &options).unwrap();
        // .lzma stream: props(5) size(8) data -> VZip keeps props and data.
        let mut out = b"VZa".to_vec();
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&lzma[..5]);
        out.extend_from_slice(&lzma[13..]);
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        out.extend_from_slice(b"zv");
        out
    }

    fn pkzip(data: &[u8]) -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(&mut cursor);
        writer
            .start_file("z", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data).unwrap();
        writer.finish().unwrap();
        cursor.into_inner()
    }

    #[test]
    fn processes_all_three_containers() {
        let data = sample(100_000);
        for (name, container) in [
            ("vzstd", vzstd(&data)),
            ("vzip", vzip(&data)),
            ("zip", pkzip(&data)),
        ] {
            let encrypted = crypto::encrypt(&container, &key(), [1; 16]);
            let output = process_chunk(&encrypted, &key(), expectation(&data))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(output, data, "{name}");
        }
    }

    #[test]
    fn rejects_wrong_checksum_and_size() {
        let data = sample(1_000);
        let encrypted = crypto::encrypt(&vzstd(&data), &key(), [2; 16]);

        let mut wrong = expectation(&data);
        wrong.checksum ^= 1;
        assert!(matches!(
            process_chunk(&encrypted, &key(), wrong),
            Err(ChunkError::ChecksumMismatch { .. })
        ));

        let mut wrong = expectation(&data);
        wrong.size = 999;
        assert!(process_chunk(&encrypted, &key(), wrong).is_err());
    }

    #[test]
    fn rejects_unknown_and_malformed_containers() {
        let plain = crypto::encrypt(b"not compressed at all, just text", &key(), [3; 16]);
        assert!(matches!(
            process_chunk(&plain, &key(), expectation(b"x")),
            Err(ChunkError::UnknownCompression(_))
        ));

        let truncated = crypto::encrypt(b"VSZa\0\0\0\0zsv", &key(), [4; 16]);
        assert!(matches!(
            process_chunk(&truncated, &key(), expectation(b"x")),
            Err(ChunkError::Malformed("VZstd"))
        ));
    }

    #[test]
    fn wrong_key_is_reported() {
        let data = sample(500);
        let encrypted = crypto::encrypt(&vzstd(&data), &key(), [5; 16]);
        let other = DepotKey::from_slice(&[0x11; 32]).unwrap();
        assert!(process_chunk(&encrypted, &other, expectation(&data)).is_err());
    }
}
