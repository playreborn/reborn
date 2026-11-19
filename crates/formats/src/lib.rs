//! Parsers and writers for Steam's local file formats.
//!
//! - [`vdf`]: text KeyValues (VDF/ACF), parsing and writing.
//! - [`binary`]: binary KeyValues (PICS package info, `appinfo.vdf` entries), reading.
//! - [`AppManifest`]: typed read access to `steamapps/appmanifest_<appid>.acf`.
//! - [`LibraryFolders`]: typed read access to `steamapps/libraryfolders.vdf`.
//!
//! ```
//! use reborn_formats::{AppManifest, vdf};
//!
//! let text = "\"AppState\"\n{\n\t\"appid\"\t\t\"228980\"\n\t\"installdir\"\t\t\"Steamworks Shared\"\n\t\"StateFlags\"\t\t\"4\"\n}\n";
//!
//! let manifest = AppManifest::parse(text)?;
//! assert_eq!(manifest.app_id, 228980);
//! assert!(manifest.is_fully_installed());
//!
//! // Generic access and lossless rewriting:
//! let document = vdf::parse(text)?;
//! assert_eq!(vdf::write(&document), text);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Planned: the `appinfo.vdf` file container and depot manifests.
#![forbid(unsafe_code)]

pub mod app_manifest;
pub mod binary;
mod fields;
pub mod library_folders;
pub mod vdf;

pub use app_manifest::{AppManifest, InstalledDepot, NewAppManifest};
pub use library_folders::{LibraryApp, LibraryFolder, LibraryFolders};

/// Error returned by the typed readers ([`AppManifest`], [`LibraryFolders`]).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The text is not valid KeyValues.
    #[error(transparent)]
    Vdf(#[from] vdf::ParseError),
    /// A required key is missing. Contains the key path, e.g. `AppState/appid`.
    #[error("missing key `{0}`")]
    MissingKey(String),
    /// A key has a string where an object is expected, or the other way round.
    #[error("key `{key}` should be {expected}")]
    WrongType {
        /// Key path, e.g. `libraryfolders/0/apps`.
        key: String,
        /// What was expected, e.g. "an object".
        expected: &'static str,
    },
    /// A value that should be a number is not.
    #[error("key `{key}` has the invalid number `{value}`")]
    InvalidNumber {
        /// Key path, e.g. `AppState/appid`.
        key: String,
        /// The value as found in the file.
        value: String,
    },
}
