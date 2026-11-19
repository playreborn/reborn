//! Typed read access to `steamapps/libraryfolders.vdf`.
//!
//! The file lists every Steam library folder. Two layouts exist:
//!
//! ```text
//! // Current layout: one object per library, with the apps it contains.
//! "libraryfolders"
//! {
//!     "0"
//!     {
//!         "path"      "C:\\Program Files (x86)\\Steam"
//!         "label"     ""
//!         "apps"
//!         {
//!             "228980"    "123456789"     // app ID -> size in bytes
//!         }
//!     }
//! }
//!
//! // Older layout: numbered keys whose value is the path.
//! "LibraryFolders"
//! {
//!     "TimeNextStatsReport"   "1600000000"
//!     "1"                     "D:\\SteamLibrary"
//! }
//! ```
//!
//! Both are supported. Non-numeric keys (metadata) are ignored.

use crate::Error;
use crate::fields::{key_path, optional_str, parse_number, required_str, section};
use crate::vdf::{self, Object, Value};

const ROOT: &str = "libraryfolders";

/// All library folders listed in `libraryfolders.vdf`, in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibraryFolders {
    /// The library folders.
    pub folders: Vec<LibraryFolder>,
}

/// One Steam library folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryFolder {
    /// The numeric key of the entry (`"0"`, `"1"`, ...).
    pub index: u32,
    /// Absolute path of the library, as written by Steam (OS-specific separators).
    pub path: String,
    /// User-defined label; `None` if absent or empty.
    pub label: Option<String>,
    /// Apps installed in this library. Empty in the older layout.
    pub apps: Vec<LibraryApp>,
}

/// An app listed in a library folder's `apps` section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryApp {
    /// The app ID.
    pub app_id: u32,
    /// Size in bytes as recorded by Steam, if present.
    pub size_on_disk: Option<u64>,
}

impl LibraryFolders {
    /// Parses the text of a `libraryfolders.vdf` file.
    pub fn parse(input: &str) -> Result<Self, Error> {
        Self::from_vdf(&vdf::parse(input)?)
    }

    /// Reads the library folders from an already parsed document.
    pub fn from_vdf(document: &Object) -> Result<Self, Error> {
        let root = section(document, "", ROOT)?;
        let mut folders = Vec::new();
        for (key, value) in root.iter() {
            let Ok(index) = key.parse::<u32>() else {
                continue;
            };
            let context = key_path(ROOT, key);
            let folder = match value {
                Value::String(path) => LibraryFolder {
                    index,
                    path: path.clone(),
                    label: None,
                    apps: Vec::new(),
                },
                Value::Object(entry) => LibraryFolder {
                    index,
                    path: required_str(entry, &context, "path")?.to_owned(),
                    label: optional_str(entry, &context, "label")?
                        .filter(|label| !label.is_empty())
                        .map(str::to_owned),
                    apps: read_apps(entry, &context)?,
                },
            };
            folders.push(folder);
        }
        Ok(Self { folders })
    }
}

fn read_apps(entry: &Object, context: &str) -> Result<Vec<LibraryApp>, Error> {
    let Some(value) = entry.get("apps") else {
        return Ok(Vec::new());
    };
    let context = key_path(context, "apps");
    let Value::Object(apps) = value else {
        return Err(Error::WrongType {
            key: context,
            expected: "an object",
        });
    };
    apps.iter()
        .map(|(app_id, size)| {
            let path = key_path(&context, app_id);
            let app_id = parse_number(app_id, &path)?.ok_or_else(|| Error::InvalidNumber {
                key: path.clone(),
                value: app_id.to_owned(),
            })?;
            let size_on_disk = match size {
                Value::String(text) => parse_number(text, &path)?,
                Value::Object(_) => {
                    return Err(Error::WrongType {
                        key: path,
                        expected: "a string",
                    });
                }
            };
            Ok(LibraryApp {
                app_id,
                size_on_disk,
            })
        })
        .collect()
}
