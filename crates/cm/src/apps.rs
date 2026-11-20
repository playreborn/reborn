//! App details from PICS: install directory, build and depots.
//!
//! Layout of the app info document (root `appinfo`):
//!
//! ```text
//! common/name, common/type
//! config/installdir
//! config/launch/<n>/{executable, arguments, workingdir, type, description}
//! config/launch/<n>/config/{oslist, osarch, betakey}
//! depots/<depot id>/name
//! depots/<depot id>/config/{oslist, osarch, language, lowviolence}
//! depots/<depot id>/manifests/<branch>/{gid, size, download}   (newer)
//! depots/<depot id>/manifests/<branch> = "<gid>"                (older)
//! depots/<depot id>/{depotfromapp, sharedinstall, dlcappid}
//! depots/branches/<branch>/buildid
//! ```
//!
//! Sources: DepotDownloader `DepotDownloader/ContentDownloader.cs`
//! (depot filtering by oslist/osarch/language/lowviolence, `manifests/<branch>/gid`,
//! `depotfromapp`, `branches`); steamroom `src/types/kv_de.rs` test data
//! (`manifests/public/gid`). The launch section has no public reference
//! implementation; its layout is taken from recorded app info
//! (`crates/cm/tests/data/`, see the fixture notes there).

use reborn_formats::vdf::{Object, Value};

use crate::{Error, Session, pics};

/// The default branch.
pub const PUBLIC_BRANCH: &str = "public";

/// Operating system, architecture and language to pick depots for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Platform {
    /// Steam's OS name: `windows`, `linux` or `macos`.
    pub os: String,
    /// Steam's architecture: `64` or `32`.
    pub arch: String,
    /// Steam language name, e.g. `english`, `german`.
    pub language: String,
}

impl Platform {
    /// The platform Reborn runs on, with English as language.
    pub fn current() -> Self {
        let os = match std::env::consts::OS {
            "windows" => "windows",
            "macos" => "macos",
            _ => "linux",
        };
        let arch = if cfg!(target_pointer_width = "64") {
            "64"
        } else {
            "32"
        };
        Self {
            os: os.to_owned(),
            arch: arch.to_owned(),
            language: "english".to_owned(),
        }
    }
}

/// A depot of an app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DepotInfo {
    /// Depot ID.
    pub depot_id: u32,
    /// Display name, if any.
    pub name: Option<String>,
    /// Operating systems this depot is for. Empty means all.
    pub os_list: Vec<String>,
    /// Architecture (`64`, `32`), if restricted.
    pub os_arch: Option<String>,
    /// Language, if this is a language depot.
    pub language: Option<String>,
    /// Low-violence variant.
    pub low_violence: bool,
    /// Manifest ID of the requested branch, if the depot has its own manifests.
    pub manifest_id: Option<u64>,
    /// Size on disk of that manifest, if known.
    pub size: Option<u64>,
    /// Download size of that manifest, if known.
    pub download_size: Option<u64>,
    /// The app that actually provides this depot (shared depots).
    pub depot_from_app: Option<u32>,
    /// DLC app this depot belongs to.
    pub dlc_app_id: Option<u32>,
    /// Installed once and shared between apps (e.g. redistributables).
    pub shared_install: bool,
}

impl DepotInfo {
    /// DepotDownloader's rules: matching OS (or none given), matching
    /// architecture (or none), matching language (or none), not low violence.
    pub fn applies_to(&self, platform: &Platform) -> bool {
        let os_ok = self.os_list.is_empty() || self.os_list.contains(&platform.os);
        let arch_ok = self
            .os_arch
            .as_deref()
            .is_none_or(|arch| arch == platform.arch);
        let language_ok = self
            .language
            .as_deref()
            .is_none_or(|language| language == platform.language);
        os_ok && arch_ok && language_ok && !self.low_violence
    }
}

/// One entry of the app's launch menu (`config/launch/<n>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaunchOption {
    /// Position in the launch section (the `<n>` key).
    pub index: u32,
    /// Program to start, relative to the install folder. Steam writes
    /// Windows paths with backslashes.
    pub executable: String,
    /// Command line arguments, as one string.
    pub arguments: Option<String>,
    /// Working directory relative to the install folder, if not the
    /// install folder itself.
    pub working_dir: Option<String>,
    /// Steam's launch type (`default`, `option1`, `none`, ...), lowercase.
    pub launch_type: Option<String>,
    /// Text shown in the launch menu.
    pub description: Option<String>,
    /// Operating systems this entry is for. Empty means all.
    pub os_list: Vec<String>,
    /// Architecture (`64`, `32`), if restricted.
    pub os_arch: Option<String>,
    /// Only offered on this beta branch.
    pub beta_key: Option<String>,
}

impl LaunchOption {
    /// Whether the entry fits the platform and branch: matching OS (or none
    /// given), matching architecture (or none), and no beta key unless it
    /// is the branch being played.
    pub fn applies_to(&self, platform: &Platform, branch: &str) -> bool {
        let os_ok = self.os_list.is_empty() || self.os_list.contains(&platform.os);
        let arch_ok = self
            .os_arch
            .as_deref()
            .is_none_or(|arch| arch == platform.arch);
        let branch_ok = self.beta_key.as_deref().is_none_or(|key| key == branch);
        os_ok && arch_ok && branch_ok
    }
}

/// What PICS says about an app.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppDetails {
    /// App ID.
    pub app_id: u32,
    /// Display name.
    pub name: Option<String>,
    /// Folder name below `steamapps/common`.
    pub install_dir: Option<String>,
    /// Build ID of the requested branch.
    pub build_id: Option<u32>,
    /// All depots, in the order Steam lists them.
    pub depots: Vec<DepotInfo>,
    /// Launch menu entries, in key order.
    pub launch: Vec<LaunchOption>,
}

impl AppDetails {
    /// Depots to install on `platform`: matching the platform rules and
    /// providing a manifest for the branch.
    pub fn depots_for<'a>(&'a self, platform: &'a Platform) -> impl Iterator<Item = &'a DepotInfo> {
        self.depots
            .iter()
            .filter(move |depot| depot.applies_to(platform) && depot.manifest_id.is_some())
    }

    /// Launch entries for `platform` on `branch`, in key order.
    pub fn launch_options_for<'a>(
        &'a self,
        platform: &'a Platform,
        branch: &'a str,
    ) -> impl Iterator<Item = &'a LaunchOption> {
        self.launch
            .iter()
            .filter(move |option| option.applies_to(platform, branch))
    }
}

impl Session {
    /// Loads name, install directory, build and depots of an app for a
    /// branch (usually [`PUBLIC_BRANCH`]).
    pub async fn app_details(&self, app_id: u32, branch: &str) -> Result<AppDetails, Error> {
        let document = pics::app_document(&self.connection, app_id).await?;
        Ok(parse_app_details(app_id, &document, branch))
    }

    /// The app info document as Steam sent it, as text VDF. For recording
    /// fixtures (app info is public data, but check it before committing).
    pub async fn app_info_text(&self, app_id: u32) -> Result<String, Error> {
        let document = pics::app_document(&self.connection, app_id).await?;
        Ok(reborn_formats::vdf::write(&document))
    }
}

/// Reads [`AppDetails`] from an app info document.
pub(crate) fn parse_app_details(app_id: u32, document: &Object, branch: &str) -> AppDetails {
    let Some(info) = document.get_object("appinfo") else {
        return AppDetails {
            app_id,
            ..AppDetails::default()
        };
    };
    let depots_section = info.get_object("depots");
    let build_id = depots_section
        .and_then(|depots| depots.get_object("branches"))
        .and_then(|branches| branches.get_object(branch))
        .and_then(|b| number(b.get_str("buildid")));

    let launch = info
        .get_object("config")
        .and_then(|config| config.get_object("launch"))
        .map(parse_launch)
        .unwrap_or_default();

    let depots = depots_section
        .map(|depots| {
            depots
                .iter()
                .filter_map(|(key, value)| {
                    let depot_id = key.parse().ok()?;
                    Some(parse_depot(depot_id, value.as_object()?, branch))
                })
                .collect()
        })
        .unwrap_or_default();

    AppDetails {
        app_id,
        name: info
            .get_object("common")
            .and_then(|c| c.get_str("name"))
            .map(str::to_owned),
        install_dir: info
            .get_object("config")
            .and_then(|c| c.get_str("installdir"))
            .filter(|dir| !dir.is_empty())
            .map(str::to_owned),
        build_id,
        depots,
        launch,
    }
}

fn parse_launch(section: &Object) -> Vec<LaunchOption> {
    let mut options: Vec<LaunchOption> = section
        .iter()
        .filter_map(|(key, value)| {
            let index = key.parse().ok()?;
            let entry = value.as_object()?;
            let text = |object: Option<&Object>, key: &str| {
                object
                    .and_then(|o| o.get_str(key))
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            };
            let config = entry.get_object("config");
            Some(LaunchOption {
                index,
                executable: text(Some(entry), "executable")?,
                arguments: text(Some(entry), "arguments"),
                working_dir: text(Some(entry), "workingdir"),
                launch_type: text(Some(entry), "type").map(|t| t.to_lowercase()),
                description: text(Some(entry), "description"),
                os_list: text(config, "oslist")
                    .map(|list| list.split(',').map(|os| os.trim().to_owned()).collect())
                    .unwrap_or_default(),
                os_arch: text(config, "osarch"),
                beta_key: text(config, "betakey"),
            })
        })
        .collect();
    options.sort_by_key(|option| option.index);
    options
}

fn parse_depot(depot_id: u32, depot: &Object, branch: &str) -> DepotInfo {
    let config = depot.get_object("config");
    let config_str = |key: &str| {
        config
            .and_then(|c| c.get_str(key))
            .filter(|value| !value.trim().is_empty())
    };

    let (manifest_id, size, download_size) = match depot
        .get_object("manifests")
        .and_then(|manifests| manifests.get(branch))
    {
        // Newer layout: manifests/<branch>/{gid, size, download}.
        Some(Value::Object(entry)) => (
            number(entry.get_str("gid")),
            number(entry.get_str("size")),
            number(entry.get_str("download")),
        ),
        // Older layout: manifests/<branch> = "<gid>".
        Some(Value::String(gid)) => (number(Some(gid)), None, None),
        None => (None, None, None),
    };

    DepotInfo {
        depot_id,
        name: depot.get_str("name").map(str::to_owned),
        os_list: config_str("oslist")
            .map(|list| list.split(',').map(|os| os.trim().to_owned()).collect())
            .unwrap_or_default(),
        os_arch: config_str("osarch").map(str::to_owned),
        language: config_str("language").map(str::to_owned),
        low_violence: config_str("lowviolence") == Some("1"),
        manifest_id,
        size,
        download_size,
        depot_from_app: number(depot.get_str("depotfromapp")),
        dlc_app_id: number(depot.get_str("dlcappid")),
        shared_install: number::<u32>(depot.get_str("sharedinstall")).is_some_and(|v| v != 0),
    }
}

fn number<T: std::str::FromStr>(text: Option<&str>) -> Option<T> {
    text?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reborn_formats::vdf;

    const SAMPLE: &str = r#""appinfo"
{
    "appid" "480"
    "common" { "name" "Spacewar" "type" "Game" }
    "config" { "installdir" "Spacewar" }
    "depots"
    {
        "228988" { "config" { "oslist" "windows" } "depotfromapp" "228980" "sharedinstall" "1" }
        "481" { "name" "Spacewar Content" "manifests" { "public" { "gid" "3183503801510301321" "size" "1234" "download" "567" } } }
        "482" { "config" { "oslist" "linux" } "manifests" { "public" "999" } }
        "483" { "config" { "oslist" "windows,macos" "osarch" "32" } "manifests" { "public" { "gid" "1" } } }
        "484" { "config" { "language" "german" } "manifests" { "public" { "gid" "2" } } }
        "485" { "config" { "lowviolence" "1" } "manifests" { "public" { "gid" "3" } } }
        "branches" { "public" { "buildid" "4242" } "beta" { "buildid" "4300" } }
    }
}"#;

    fn details() -> AppDetails {
        parse_app_details(480, &vdf::parse(SAMPLE).unwrap(), PUBLIC_BRANCH)
    }

    fn platform(os: &str, arch: &str, language: &str) -> Platform {
        Platform {
            os: os.to_owned(),
            arch: arch.to_owned(),
            language: language.to_owned(),
        }
    }

    #[test]
    fn reads_app_and_depots() {
        let app = details();
        assert_eq!(app.name.as_deref(), Some("Spacewar"));
        assert_eq!(app.install_dir.as_deref(), Some("Spacewar"));
        assert_eq!(app.build_id, Some(4242));
        let ids: Vec<_> = app.depots.iter().map(|d| d.depot_id).collect();
        assert_eq!(ids, [228988, 481, 482, 483, 484, 485]);

        let main = &app.depots[1];
        assert_eq!(main.name.as_deref(), Some("Spacewar Content"));
        assert_eq!(main.manifest_id, Some(3183503801510301321));
        assert_eq!((main.size, main.download_size), (Some(1234), Some(567)));

        let shared = &app.depots[0];
        assert_eq!(shared.depot_from_app, Some(228980));
        assert!(shared.shared_install);
        assert_eq!(shared.manifest_id, None);

        assert_eq!(
            app.depots[2].manifest_id,
            Some(999),
            "older manifest layout"
        );
    }

    #[test]
    fn selects_depots_like_depotdownloader() {
        let app = details();
        let selected =
            |p: Platform| -> Vec<u32> { app.depots_for(&p).map(|d| d.depot_id).collect() };
        assert_eq!(selected(platform("windows", "64", "english")), [481]);
        assert_eq!(selected(platform("windows", "32", "english")), [481, 483]);
        assert_eq!(selected(platform("linux", "64", "english")), [481, 482]);
        assert_eq!(selected(platform("windows", "64", "german")), [481, 484]);
    }

    #[test]
    fn other_branches_have_their_own_build() {
        let app = parse_app_details(480, &vdf::parse(SAMPLE).unwrap(), "beta");
        assert_eq!(app.build_id, Some(4300));
        assert!(app.depots.iter().all(|d| d.manifest_id.is_none()));
    }

    const LAUNCH: &str = r#""appinfo"
{
    "config"
    {
        "installdir" "Game"
        "launch"
        {
            "1" { "executable" "bin/game" "type" "default" "config" { "oslist" "linux" "osarch" "64" } }
            "0" { "executable" "bin\\Game.exe" "arguments" "-windowed" "workingdir" "bin" "type" "Default" "description" "Play" "config" { "oslist" "windows" } }
            "2" { "executable" "Tools\\Editor.exe" "type" "option1" "description" "Editor" "config" { "oslist" "windows" "betakey" "tools" } }
            "3" { "arguments" "-no-executable" }
        }
    }
}"#;

    #[test]
    fn reads_launch_options_in_key_order() {
        let app = parse_app_details(1, &vdf::parse(LAUNCH).unwrap(), PUBLIC_BRANCH);
        let indices: Vec<u32> = app.launch.iter().map(|option| option.index).collect();
        assert_eq!(indices, [0, 1, 2], "entries without executable are skipped");
        let play = &app.launch[0];
        assert_eq!(play.executable, "bin\\Game.exe");
        assert_eq!(play.arguments.as_deref(), Some("-windowed"));
        assert_eq!(play.working_dir.as_deref(), Some("bin"));
        assert_eq!(play.launch_type.as_deref(), Some("default"));
        assert_eq!(play.description.as_deref(), Some("Play"));
        assert_eq!(play.os_list, ["windows"]);
    }

    #[test]
    fn selects_launch_options_by_platform_and_branch() {
        let app = parse_app_details(1, &vdf::parse(LAUNCH).unwrap(), PUBLIC_BRANCH);
        let pick = |os: &str, branch: &str| -> Vec<u32> {
            app.launch_options_for(&platform(os, "64", "english"), branch)
                .map(|option| option.index)
                .collect()
        };
        assert_eq!(pick("windows", PUBLIC_BRANCH), [0]);
        assert_eq!(pick("windows", "tools"), [0, 2]);
        assert_eq!(pick("linux", PUBLIC_BRANCH), [1]);
    }

    /// Recorded app info, see `tests/data/README.md`.
    const BONGO_CAT: &str = include_str!("../tests/data/appinfo_3419430.vdf");

    #[test]
    fn reads_recorded_app_info() {
        let app = parse_app_details(3419430, &vdf::parse(BONGO_CAT).unwrap(), PUBLIC_BRANCH);
        assert_eq!(app.name.as_deref(), Some("Bongo Cat"));
        assert_eq!(app.install_dir.as_deref(), Some("BongoCat"));
        assert_eq!(app.build_id, Some(26784007));

        let launch: Vec<(u32, &str, Option<&str>, Vec<String>)> = app
            .launch
            .iter()
            .map(|option| {
                (
                    option.index,
                    option.executable.as_str(),
                    option.launch_type.as_deref(),
                    option.os_list.clone(),
                )
            })
            .collect();
        assert_eq!(
            launch,
            [
                (0, "BongoCat.exe", None, vec!["windows".to_owned()]),
                (
                    1,
                    "BongoCat.app/Contents/MacOS/BongoCat",
                    None,
                    vec!["macos".to_owned()]
                ),
                (2, "launch.sh", Some("default"), vec!["linux".to_owned()]),
            ]
        );
        let pick = |os: &str| -> Vec<u32> {
            app.launch_options_for(&platform(os, "64", "english"), PUBLIC_BRANCH)
                .map(|option| option.index)
                .collect()
        };
        assert_eq!(pick("windows"), [0]);
        assert_eq!(pick("linux"), [2]);

        let windows: Vec<u32> = app
            .depots_for(&platform("windows", "64", "english"))
            .map(|depot| depot.depot_id)
            .collect();
        assert_eq!(windows, [3419431]);
    }

    #[test]
    fn current_platform_is_sensible() {
        let current = Platform::current();
        assert!(["windows", "linux", "macos"].contains(&current.os.as_str()));
        assert!(["64", "32"].contains(&current.arch.as_str()));
    }
}
