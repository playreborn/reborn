//! Typed read access to `steamapps/appmanifest_<appid>.acf`.
//!
//! Steam writes one such file per installed (or installing) app into each
//! library folder. The root key is `"AppState"`.

use crate::Error;
use crate::fields::{
    key_path, optional_number, optional_str, parse_number, required_number, required_str, section,
};
use crate::vdf::{self, Object, Value};

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
    /// `InstalledDepots`: which manifest of each depot is installed.
    pub installed_depots: Vec<InstalledDepot>,
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
            installed_depots: read_installed_depots(state)?,
        })
    }

    /// Whether the [`STATE_FULLY_INSTALLED`](Self::STATE_FULLY_INSTALLED) bit is set.
    pub fn is_fully_installed(&self) -> bool {
        self.state_flags & Self::STATE_FULLY_INSTALLED != 0
    }
}

fn read_installed_depots(state: &Object) -> Result<Vec<InstalledDepot>, Error> {
    let context = format!("{ROOT}/InstalledDepots");
    let Some(depots) = state.get("InstalledDepots") else {
        return Ok(Vec::new());
    };
    let Value::Object(depots) = depots else {
        return Err(Error::WrongType {
            key: context,
            expected: "an object",
        });
    };
    depots
        .iter()
        .map(|(depot_id, entry)| {
            let path = key_path(&context, depot_id);
            let depot_id = parse_number(depot_id, &path)?.ok_or_else(|| Error::InvalidNumber {
                key: path.clone(),
                value: depot_id.to_owned(),
            })?;
            let Value::Object(entry) = entry else {
                return Err(Error::WrongType {
                    key: path,
                    expected: "an object",
                });
            };
            Ok(InstalledDepot {
                depot_id,
                manifest_id: required_number(entry, &path, "manifest")?,
                size: optional_number(entry, &path, "size")?.unwrap_or(0),
            })
        })
        .collect()
}

/// One installed depot, as listed under `InstalledDepots`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledDepot {
    /// Depot ID.
    pub depot_id: u32,
    /// Installed manifest ID.
    pub manifest_id: u64,
    /// Size on disk of this depot's files.
    pub size: u64,
}

/// Everything needed to write the `appmanifest_<appid>.acf` of a freshly
/// installed app.
///
/// Field order and values follow the files the official client writes
/// (checked against 18 real manifests, 2026-11-19): `Universe` 1,
/// `StateFlags` 4, `DownloadType` 1, `UpdateResult` 0, byte counters at
/// their final values. `LauncherPath` (path of `steam.exe`) and
/// `InstallScripts` are left out; Steam fills in what it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewAppManifest {
    /// App ID.
    pub app_id: u32,
    /// Display name.
    pub name: String,
    /// Folder name below `steamapps/common`.
    pub install_dir: String,
    /// Installed build ID.
    pub build_id: u64,
    /// 64-bit SteamID of the owner.
    pub last_owner: u64,
    /// Total size on disk.
    pub size_on_disk: u64,
    /// Bytes downloaded from the CDN.
    pub download_size: u64,
    /// Install time, Unix seconds.
    pub last_updated: u64,
    /// Installed depots.
    pub installed_depots: Vec<InstalledDepot>,
}

impl NewAppManifest {
    /// Builds the KeyValues document (root `AppState`).
    pub fn to_vdf(&self) -> Object {
        let mut state = Object::new();
        let number = |value: u64| value.to_string();
        state.push("appid", self.app_id.to_string());
        state.push("Universe", "1");
        state.push("name", self.name.as_str());
        state.push("StateFlags", AppManifest::STATE_FULLY_INSTALLED.to_string());
        state.push("installdir", self.install_dir.as_str());
        state.push("LastUpdated", number(self.last_updated));
        state.push("LastPlayed", "0");
        state.push("SizeOnDisk", number(self.size_on_disk));
        state.push("StagingSize", "0");
        state.push("buildid", number(self.build_id));
        state.push("LastOwner", number(self.last_owner));
        state.push("DownloadType", "1");
        state.push("UpdateResult", "0");
        state.push("BytesToDownload", number(self.download_size));
        state.push("BytesDownloaded", number(self.download_size));
        state.push("BytesToStage", number(self.size_on_disk));
        state.push("BytesStaged", number(self.size_on_disk));
        state.push("TargetBuildID", number(self.build_id));
        state.push("AutoUpdateBehavior", "0");
        state.push("AllowOtherDownloadsWhileRunning", "0");
        state.push("ScheduledAutoUpdate", "0");

        let mut depots = Object::new();
        for depot in &self.installed_depots {
            let mut entry = Object::new();
            entry.push("manifest", number(depot.manifest_id));
            entry.push("size", number(depot.size));
            depots.push(depot.depot_id.to_string(), entry);
        }
        state.push("InstalledDepots", depots);
        state.push("UserConfig", Object::new());
        state.push("MountedConfig", Object::new());

        let mut document = Object::new();
        document.push(ROOT, state);
        document
    }

    /// The file content in Steam's layout.
    pub fn to_text(&self) -> String {
        vdf::write(&self.to_vdf())
    }

    /// Updates an existing appmanifest document in place after an install
    /// or update. Only the fields an install sets are changed; everything
    /// else (`LauncherPath`, `InstallScripts`, `UserConfig`, keys Reborn does
    /// not know, entries of other depots) is kept in its position. If the
    /// document has no `AppState`, it is replaced by [`Self::to_vdf`].
    pub fn apply_to(&self, document: &mut Object) {
        let Some(state) = document.get_object_mut(ROOT) else {
            *document = self.to_vdf();
            return;
        };
        let number = |value: u64| value.to_string();
        state.insert("name", self.name.as_str());
        state.insert("StateFlags", AppManifest::STATE_FULLY_INSTALLED.to_string());
        state.insert("installdir", self.install_dir.as_str());
        state.insert("LastUpdated", number(self.last_updated));
        state.insert("SizeOnDisk", number(self.size_on_disk));
        state.insert("StagingSize", "0");
        state.insert("buildid", number(self.build_id));
        state.insert("LastOwner", number(self.last_owner));
        state.insert("UpdateResult", "0");
        state.insert("BytesToDownload", number(self.download_size));
        state.insert("BytesDownloaded", number(self.download_size));
        state.insert("BytesToStage", number(self.size_on_disk));
        state.insert("BytesStaged", number(self.size_on_disk));
        state.insert("TargetBuildID", number(self.build_id));

        if state.get_object("InstalledDepots").is_none() {
            state.insert("InstalledDepots", Object::new());
        }
        if let Some(depots) = state.get_object_mut("InstalledDepots") {
            for depot in &self.installed_depots {
                let mut entry = Object::new();
                entry.push("manifest", number(depot.manifest_id));
                entry.push("size", number(depot.size));
                depots.insert(depot.depot_id.to_string(), entry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> NewAppManifest {
        NewAppManifest {
            app_id: 480,
            name: "Spacewar".to_owned(),
            install_dir: "Spacewar".to_owned(),
            build_id: 3_538_192,
            last_owner: 76_561_190_000_000_000,
            size_on_disk: 1_900_000,
            download_size: 900_000,
            last_updated: 1_700_000_000,
            installed_depots: vec![InstalledDepot {
                depot_id: 481,
                manifest_id: 3_183_503_801_510_301_321,
                size: 1_900_000,
            }],
        }
    }

    #[test]
    fn written_manifest_reads_back() {
        let manifest = AppManifest::parse(&sample().to_text()).unwrap();
        assert_eq!(manifest.app_id, 480);
        assert_eq!(manifest.install_dir, "Spacewar");
        assert!(manifest.is_fully_installed());
        assert_eq!(manifest.build_id, Some(3_538_192));
        assert_eq!(manifest.size_on_disk, Some(1_900_000));
    }

    #[test]
    fn uses_the_official_key_order() {
        let document = sample().to_vdf();
        let state = document.get_object("AppState").unwrap();
        let keys: Vec<_> = state.iter().map(|(key, _)| key).collect();
        assert_eq!(
            keys,
            [
                "appid",
                "Universe",
                "name",
                "StateFlags",
                "installdir",
                "LastUpdated",
                "LastPlayed",
                "SizeOnDisk",
                "StagingSize",
                "buildid",
                "LastOwner",
                "DownloadType",
                "UpdateResult",
                "BytesToDownload",
                "BytesDownloaded",
                "BytesToStage",
                "BytesStaged",
                "TargetBuildID",
                "AutoUpdateBehavior",
                "AllowOtherDownloadsWhileRunning",
                "ScheduledAutoUpdate",
                "InstalledDepots",
                "UserConfig",
                "MountedConfig",
            ]
        );
        let depot = state
            .get_object("InstalledDepots")
            .and_then(|depots| depots.get_object("481"))
            .unwrap();
        assert_eq!(depot.get_str("manifest"), Some("3183503801510301321"));
    }
}
