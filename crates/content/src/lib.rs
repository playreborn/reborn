//! Steam content engine.
//!
//! Done (milestone M1, stage 1):
//! - [`chunk`]: decrypt, decompress (VZstd, VZip/LZMA, ZIP) and verify depot chunks,
//! - [`checksum`]: Steam's zero-seeded Adler-32,
//! - [`DepotKey`]: the depot's AES-256 key,
//! - [`manifest`]: depot manifests (ZIP container, sections, encrypted names),
//! - [`path`]: safe relative install paths from untrusted manifest names,
//! - [`cdn`]: manifest and chunk downloads from content servers.
//!
//! Planned: parallel downloads, staged installs and manifest-diff updates.
//!
//! File names coming from manifests are untrusted input: they must be checked
//! against path traversal and symlinks before anything is written.
#![forbid(unsafe_code)]

pub mod cdn;
pub mod checksum;
pub mod chunk;
mod crypto;
pub mod manifest;
pub mod path;

pub use crypto::{CryptoError, DepotKey};
