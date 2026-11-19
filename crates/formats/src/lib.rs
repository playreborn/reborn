//! Parsers and writers for Steam's local file formats.
//!
//! Planned modules:
//! - text KeyValues (VDF/ACF), e.g. `libraryfolders.vdf`, `appmanifest_*.acf`
//! - binary KeyValues, e.g. `appinfo.vdf` (including the newer string-table format)
//! - depot manifests
//!
//! Every parser is tested against real (redacted) sample files in `fixtures/`.
//!
//! **Status:** skeleton, nothing implemented yet (milestone M0/M1, see
//! `docs/plan.md` section 6.4).
#![forbid(unsafe_code)]
