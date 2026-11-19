//! Steam content engine.
//!
//! Planned responsibilities: depot decryption keys, manifest request codes,
//! CDN server selection, manifest download and parsing, chunk download,
//! AES decryption, decompression (VZip/LZMA, Zstd, Zip), checksum
//! verification, staged installs and manifest-diff updates.
//!
//! File names coming from manifests are untrusted input: they must be checked
//! against path traversal and symlinks before anything is written.
//!
//! **Status:** skeleton, nothing implemented yet (milestone M1, see
//! `docs/plan.md` section 7.4).
#![forbid(unsafe_code)]
