//! Typed read access to `steamapps/appmanifest_<appid>.acf`.
//!
//! Steam writes one such file per installed (or installing) app into each
//! library folder. The root key is `"AppState"`.

use crate::Error;
use crate::fields::{optional_number, optional_str, required_number, required_str, section};
use crate::vdf::{self, Object};

const ROOT: &str = "AppState";

/// The fields of an app manifest that Reborn needs to show and launch a game.
///
/// This is a read-only view: unknown fields are ignored. Code that rewrites
/// a manifest works on the [`vdf::Object`] directly, so no field gets lost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppManifest {
    /// `appid`.
    pub app_id: u32,
    /// `name`, the display name.
    pub name: Option<String>,
    /// `installdir`, the folder name below `steamapps/common/`.
    pub install_dir: String,
    /// `StateFlags`, a bit set describing the install state. `0` if absent.
    pub state_flags: u32,
    /// `SizeOnDisk` in bytes.
    pub size_on_disk: Option<u64>,
    /// `buildid` of the installed build.
    pub build_id: Option<u64>,
    /// `LastUpdated`, Unix timestamp in seconds.
    pub last_updated: Option<u64>,
    /// `LastPlayed`, Unix timestamp in seconds.
    pub last_played: Option<u64>,
}

impl AppManifest {
    /// `StateFlags` bit that marks an app as fully installed.
    ///
    /// Observed in real manifests (a fully installed app has `"StateFlags" "4"`)
    /// and used the same way by other tools that read Steam libraries, such
    /// as the `steamlocate` crate. Other bits are not interpreted yet.
    pub const STATE_FULLY_INSTALLED: u32 = 4;

    /// Parses the text of an `appmanifest_*.acf` file.
    pub fn parse(input: &str) -> Result<Self, Error> {
        Self::from_vdf(&vdf::parse(input)?)
    }

    /// Reads the manifest from an already parsed document.
    pub fn from_vdf(document: &Object) -> Result<Self, Error> {
        let state = section(document, "", ROOT)?;
        Ok(Self {
            app_id: required_number(state, ROOT, "appid")?,
            name: optional_str(state, ROOT, "name")?.map(str::to_owned),
            install_dir: required_str(state, ROOT, "installdir")?.to_owned(),
            state_flags: optional_number(state, ROOT, "StateFlags")?.unwrap_or(0),
            size_on_disk: optional_number(state, ROOT, "SizeOnDisk")?,
            build_id: optional_number(state, ROOT, "buildid")?,
            last_updated: optional_number(state, ROOT, "LastUpdated")?,
            last_played: optional_number(state, ROOT, "LastPlayed")?,
        })
    }

    /// Whether the [`STATE_FULLY_INSTALLED`](Self::STATE_FULLY_INSTALLED) bit is set.
    pub fn is_fully_installed(&self) -> bool {
        self.state_flags & Self::STATE_FULLY_INSTALLED != 0
    }
}
