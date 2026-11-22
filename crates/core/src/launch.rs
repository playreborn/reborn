//! Starting installed games (M2 stage 4, compatibility tier 1 in the plan:
//! games that need the official client still need it running).
//!
//! The launch entries come from the app info (`config/launch`, see
//! [`reborn_cm::LaunchOption`]). Paths in them are untrusted server data and
//! go through the same checks as file names from depot manifests
//! ([`reborn_content::path::install_path`]).
//!
//! The game gets `SteamAppId` and `SteamGameId` in its environment, as the
//! Steam client provides them: Valve's Proton launcher reads both
//! (ValveSoftware/Proton, `proton` script, branch `proton_9.0`).
//!
//! On Linux, a game with only Windows launch entries runs through an
//! installed Proton build ([`reborn_platform::proton`], ADR 0017). The Wine
//! prefix lives in Reborn's data folder, `compatdata/<app id>`, one per app.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use reborn_cm::{AppDetails, LaunchOption, PUBLIC_BRANCH, Platform, Session};
use reborn_content::path::install_path;
use reborn_platform::proton::{self, ProtonInstall, ProtonLaunch, ProtonSource, SearchRoots};
use reborn_platform::settings;
use tracing::warn;

use crate::CoreError;
use crate::library::{app_manifest_path, read_app_manifest};

/// What to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchRequest {
    /// The app.
    pub app_id: u32,
    /// Library folder (`None`: Reborn's own library).
    pub library: Option<PathBuf>,
    /// Index of a launch entry (`None`: the default entry for this platform).
    pub option: Option<u32>,
}

/// A resolved launch: everything needed to start the process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchPlan {
    /// The app.
    pub app_id: u32,
    /// App name, if known.
    pub name: Option<String>,
    /// Program to run: the game itself, or Proton's launcher script.
    pub program: PathBuf,
    /// Its arguments.
    pub args: Vec<OsString>,
    /// Working directory.
    pub working_dir: PathBuf,
    /// Extra environment variables.
    pub env: Vec<(String, OsString)>,
    /// The launch entry that was chosen.
    pub option: LaunchOption,
    /// The game's own executable (the same as `program` without Proton).
    pub executable: PathBuf,
    /// The Proton build, if the game runs through one.
    pub proton: Option<String>,
    /// Directory to create before starting (Proton's per-app data).
    pub create_dir: Option<PathBuf>,
    /// The DLC app IDs the app info names for its depots
    /// (`depots/<id>/dlcappid`, [`reborn_cm::DepotInfo::dlc_app_id`]),
    /// sorted, without duplicates. The shim may ask about the ownership of
    /// these and of the app itself, nothing else (ADR 0055). DLC without a
    /// depot is not in this list: no sourced field names them yet.
    pub dlc_app_ids: Vec<u32>,
}

/// Resolves a launch for an installed app: reads its appmanifest, loads the
/// launch entries from Steam and picks one for this computer.
pub async fn plan_launch(
    session: &Session,
    request: &LaunchRequest,
) -> Result<LaunchPlan, CoreError> {
    let library = match &request.library {
        Some(library) => library.clone(),
        None => settings::default_library()?,
    };
    let game_dir = installed_game_dir(&library, request.app_id)?;
    let details = session.app_details(request.app_id, PUBLIC_BRANCH).await?;
    let platform = Platform::current();
    match plan_from(
        &details,
        &game_dir,
        &platform,
        PUBLIC_BRANCH,
        request.option,
    ) {
        Err(CoreError::NeedsProton(name)) if platform.os == "linux" => {
            let windows = Platform {
                os: "windows".to_owned(),
                ..platform
            };
            let plan = plan_from(&details, &game_dir, &windows, PUBLIC_BRANCH, request.option)?;
            let roots = SearchRoots::from_environment();
            let discovery = proton::discover(&roots);
            for problem in &discovery.problems {
                warn!(%problem, "while looking for Proton");
            }
            let wanted = settings::proton_setting().ok().flatten();
            let install = pick_proton(&discovery.installs, wanted.as_deref())
                .ok_or(CoreError::NoProton(name))?;
            let compat_data = settings::data_dir()?
                .join("compatdata")
                .join(request.app_id.to_string());
            let steam_client = roots.steam_roots.iter().find(|root| root.is_dir());
            with_proton(
                plan,
                install,
                &compat_data,
                steam_client.map(PathBuf::as_path),
            )
        }
        other => other,
    }
}

/// The Proton build chosen in the settings if it is installed, else the
/// automatic choice ([`choose_proton`]).
pub fn pick_proton<'a>(
    installs: &'a [ProtonInstall],
    wanted: Option<&str>,
) -> Option<&'a ProtonInstall> {
    if let Some(wanted) = wanted {
        match installs.iter().find(|install| install.name == wanted) {
            Some(install) => return Some(install),
            None => warn!(
                wanted,
                "the chosen Proton build is not installed; choosing automatically"
            ),
        }
    }
    choose_proton(installs)
}

/// Names of the installed Proton builds, best automatic choice first.
/// Empty on Windows, where games run natively.
pub fn installed_protons() -> Vec<String> {
    if cfg!(windows) {
        return Vec::new();
    }
    let discovery = proton::discover(&SearchRoots::from_environment());
    let mut installs = discovery.installs;
    let best = choose_proton(&installs).map(|install| install.name.clone());
    installs.sort_by(|a, b| a.name.cmp(&b.name));
    let mut names: Vec<String> = installs.into_iter().map(|install| install.name).collect();
    if let Some(best) = best {
        names.retain(|name| *name != best);
        names.insert(0, best);
    }
    names.dedup();
    names
}

/// The Proton build to use: the newest numbered official build (`Proton
/// 9.0`, `Proton 10.0`, ...), else another official build such as `Proton
/// Experimental`, else a custom build from `compatibilitytools.d`. This is
/// Reborn's own default until a setting lets the user choose.
pub fn choose_proton(installs: &[ProtonInstall]) -> Option<&ProtonInstall> {
    installs.iter().max_by_key(|install| {
        let official = matches!(install.source, ProtonSource::SteamLibrary(_));
        let version: Option<Vec<u32>> = install.name.strip_prefix("Proton ").and_then(|rest| {
            rest.split('.')
                .map(|part| part.trim().parse().ok())
                .collect()
        });
        match (official, version) {
            (true, Some(version)) => (2, version),
            (true, None) => (1, Vec::new()),
            (false, _) => (0, Vec::new()),
        }
    })
}

/// Turns a plan for the Windows executable into one that runs it through
/// Proton. Pure apart from what [`proton::proton_command`] checks.
pub fn with_proton(
    plan: LaunchPlan,
    install: &ProtonInstall,
    compat_data: &Path,
    steam_client: Option<&Path>,
) -> Result<LaunchPlan, CoreError> {
    let args: Vec<String> = plan
        .args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let command = proton::proton_command(&ProtonLaunch {
        proton_dir: &install.dir,
        verb: proton::ProtonVerb::default(),
        executable: &plan.executable,
        args: &args,
        compat_data_dir: compat_data,
        steam_client_dir: steam_client,
        app_id: plan.app_id,
        working_dir: &plan.working_dir,
    })?;
    Ok(LaunchPlan {
        program: command.program,
        args: command.args,
        working_dir: command.current_dir,
        env: command.env.into_iter().collect(),
        proton: Some(install.name.clone()),
        create_dir: Some(compat_data.to_path_buf()),
        ..plan
    })
}

/// The install folder of a fully installed app.
pub fn installed_game_dir(library: &Path, app_id: u32) -> Result<PathBuf, CoreError> {
    let manifest = read_app_manifest(&app_manifest_path(library, app_id))?
        .map(|(_, manifest)| manifest)
        .filter(|manifest| manifest.is_fully_installed())
        .ok_or(CoreError::NotInstalled(app_id))?;
    let folder = install_path(&manifest.install_dir)
        .ok()
        .filter(|path| path.components().count() == 1)
        .ok_or_else(|| CoreError::UnsafeInstallDir(manifest.install_dir.clone()))?;
    Ok(library.join("steamapps").join("common").join(folder))
}

/// Picks the launch entry and builds the plan. Pure: touches no files.
///
/// Without a requested index, the first entry for the platform whose type
/// is `default` (or not given) wins, otherwise the first entry for the
/// platform.
pub fn plan_from(
    details: &AppDetails,
    game_dir: &Path,
    platform: &Platform,
    branch: &str,
    requested: Option<u32>,
) -> Result<LaunchPlan, CoreError> {
    let candidates: Vec<&LaunchOption> = details.launch_options_for(platform, branch).collect();
    let name = || {
        details
            .name
            .clone()
            .unwrap_or_else(|| format!("app {}", details.app_id))
    };
    let option = match requested {
        Some(index) => candidates
            .iter()
            .find(|option| option.index == index)
            .copied()
            .ok_or(CoreError::UnknownLaunchOption(index))?,
        None => candidates
            .iter()
            .find(|option| {
                option
                    .launch_type
                    .as_deref()
                    .is_none_or(|kind| kind == "default")
            })
            .or_else(|| candidates.first())
            .copied()
            .ok_or_else(|| {
                let windows_only = platform.os != "windows"
                    && details
                        .launch
                        .iter()
                        .any(|option| option.os_list.iter().any(|os| os == "windows"));
                if windows_only {
                    CoreError::NeedsProton(name())
                } else {
                    CoreError::NoLaunchOption {
                        name: name(),
                        os: platform.os.clone(),
                    }
                }
            })?,
    };

    let program = game_dir.join(relative(&option.executable)?);
    let working_dir = match option.working_dir.as_deref() {
        Some(dir) if !is_current_dir(dir) => game_dir.join(relative(dir)?),
        _ => game_dir.to_path_buf(),
    };
    let app_id = OsString::from(details.app_id.to_string());
    Ok(LaunchPlan {
        app_id: details.app_id,
        name: details.name.clone(),
        executable: program.clone(),
        program,
        args: option
            .arguments
            .as_deref()
            .map(split_arguments)
            .unwrap_or_default()
            .into_iter()
            .map(OsString::from)
            .collect(),
        working_dir,
        env: vec![
            ("SteamAppId".to_owned(), app_id.clone()),
            ("SteamGameId".to_owned(), app_id),
        ],
        option: option.clone(),
        proton: None,
        create_dir: None,
        dlc_app_ids: dlc_app_ids(details),
    })
}

/// The DLC app IDs named by the app's depots, on any platform, sorted and
/// without duplicates; never the app itself.
fn dlc_app_ids(details: &AppDetails) -> Vec<u32> {
    let mut ids: Vec<u32> = details
        .depots
        .iter()
        .filter_map(|depot| depot.dlc_app_id)
        .filter(|&id| id != details.app_id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Starts the planned process with no stdin/stdout/stderr attached.
pub fn spawn(plan: &LaunchPlan) -> Result<Child, CoreError> {
    if !plan.executable.is_file() {
        return Err(CoreError::MissingExecutable(plan.executable.clone()));
    }
    if let Some(dir) = &plan.create_dir {
        std::fs::create_dir_all(dir).map_err(crate::error::io_error(dir))?;
    }
    Command::new(&plan.program)
        .args(&plan.args)
        .current_dir(&plan.working_dir)
        .envs(plan.env.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|source| CoreError::Launch {
            program: plan.program.clone(),
            source,
        })
}

/// A path from the launch entry, relative to the install folder. A leading
/// `./` or `.\` (common for Linux scripts) is dropped; everything else must
/// pass [`install_path`].
fn relative(path: &str) -> Result<PathBuf, CoreError> {
    let trimmed = path
        .trim()
        .trim_start_matches("./")
        .trim_start_matches(".\\");
    install_path(trimmed).map_err(|_| CoreError::UnsafeLaunchPath(path.to_owned()))
}

fn is_current_dir(dir: &str) -> bool {
    matches!(dir.trim(), "" | "." | "./" | ".\\")
}

/// Splits Steam's argument string on whitespace; double quotes group words
/// (and are removed). Steam does not document its quoting, so nothing more
/// (no escapes) is interpreted.
fn split_arguments(arguments: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut has_token = false;
    for c in arguments.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                has_token = true;
            }
            c if c.is_whitespace() && !quoted => {
                if has_token {
                    args.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(index: u32, executable: &str, os: &str) -> LaunchOption {
        LaunchOption {
            index,
            executable: executable.to_owned(),
            launch_type: Some("default".to_owned()),
            os_list: if os.is_empty() {
                Vec::new()
            } else {
                vec![os.to_owned()]
            },
            ..LaunchOption::default()
        }
    }

    fn app(launch: Vec<LaunchOption>) -> AppDetails {
        AppDetails {
            app_id: 480,
            name: Some("Spacewar".to_owned()),
            launch,
            ..AppDetails::default()
        }
    }

    fn platform(os: &str) -> Platform {
        Platform {
            os: os.to_owned(),
            arch: "64".to_owned(),
            language: "english".to_owned(),
        }
    }

    #[test]
    fn plans_the_default_entry_for_the_platform() {
        let mut windows = option(0, "bin\\Game.exe", "windows");
        windows.arguments = Some("-windowed \"+name Some Player\"".to_owned());
        windows.working_dir = Some("bin".to_owned());
        let linux = option(1, "./game.sh", "linux");
        let details = app(vec![windows, linux]);
        let dir = Path::new("lib")
            .join("steamapps")
            .join("common")
            .join("Game");

        let plan = plan_from(&details, &dir, &platform("windows"), PUBLIC_BRANCH, None).unwrap();
        assert_eq!(plan.program, dir.join("bin").join("Game.exe"));
        assert_eq!(plan.working_dir, dir.join("bin"));
        assert_eq!(plan.args, ["-windowed", "+name Some Player"]);
        assert!(plan.env.contains(&("SteamAppId".to_owned(), "480".into())));
        assert!(plan.env.contains(&("SteamGameId".to_owned(), "480".into())));

        let plan = plan_from(&details, &dir, &platform("linux"), PUBLIC_BRANCH, None).unwrap();
        assert_eq!(plan.program, dir.join("game.sh"), "leading ./ dropped");
        assert_eq!(plan.working_dir, dir);
    }

    #[test]
    fn prefers_default_type_and_honours_a_requested_entry() {
        let mut tool = option(0, "Tool.exe", "");
        tool.launch_type = Some("option1".to_owned());
        let details = app(vec![tool, option(1, "Game.exe", "")]);
        let dir = Path::new("game");
        let pick = |requested| {
            plan_from(
                &details,
                dir,
                &platform("windows"),
                PUBLIC_BRANCH,
                requested,
            )
            .map(|plan| plan.option.index)
        };
        assert_eq!(pick(None).unwrap(), 1);
        assert_eq!(pick(Some(0)).unwrap(), 0);
        assert!(matches!(
            pick(Some(7)),
            Err(CoreError::UnknownLaunchOption(7))
        ));
    }

    #[test]
    fn unsafe_paths_from_the_server_are_refused() {
        for bad in [
            "..\\..\\Windows\\evil.exe",
            "/usr/bin/evil",
            "C:\\evil.exe",
            "bin/../../x",
        ] {
            let details = app(vec![option(0, bad, "")]);
            let result = plan_from(
                &details,
                Path::new("game"),
                &platform("windows"),
                PUBLIC_BRANCH,
                None,
            );
            assert!(
                matches!(result, Err(CoreError::UnsafeLaunchPath(_))),
                "{bad}"
            );
        }
        let mut escaping_dir = option(0, "Game.exe", "");
        escaping_dir.working_dir = Some("..".to_owned());
        let result = plan_from(
            &app(vec![escaping_dir]),
            Path::new("game"),
            &platform("windows"),
            PUBLIC_BRANCH,
            None,
        );
        assert!(matches!(result, Err(CoreError::UnsafeLaunchPath(_))));
    }

    #[test]
    fn windows_only_games_on_linux_need_proton() {
        let details = app(vec![option(0, "Game.exe", "windows")]);
        let result = plan_from(
            &details,
            Path::new("game"),
            &platform("linux"),
            PUBLIC_BRANCH,
            None,
        );
        assert!(matches!(result, Err(CoreError::NeedsProton(_))));
        let result = plan_from(
            &app(Vec::new()),
            Path::new("game"),
            &platform("windows"),
            PUBLIC_BRANCH,
            None,
        );
        assert!(matches!(result, Err(CoreError::NoLaunchOption { .. })));
    }

    #[test]
    fn splits_arguments_like_a_simple_shell() {
        assert_eq!(split_arguments("  -a   -b "), ["-a", "-b"]);
        assert_eq!(
            split_arguments("-x \"two words\" \"\""),
            ["-x", "two words", ""]
        );
        assert!(split_arguments("").is_empty());
    }

    #[test]
    fn missing_executable_is_reported_before_spawning() {
        let plan = LaunchPlan {
            app_id: 1,
            name: None,
            program: PathBuf::from("definitely-missing-reborn-test.exe"),
            args: Vec::new(),
            working_dir: PathBuf::from("."),
            env: Vec::new(),
            option: LaunchOption::default(),
            executable: PathBuf::from("definitely-missing-reborn-test.exe"),
            proton: None,
            create_dir: None,
            dlc_app_ids: Vec::new(),
        };
        assert!(matches!(spawn(&plan), Err(CoreError::MissingExecutable(_))));
    }

    #[test]
    fn the_plan_names_the_dlc_of_the_app_depots() {
        let depot = |depot_id, dlc_app_id| reborn_cm::DepotInfo {
            depot_id,
            dlc_app_id,
            ..reborn_cm::DepotInfo::default()
        };
        let mut details = app(vec![option(0, "Game.exe", "")]);
        details.depots = vec![
            depot(481, None),
            depot(483, Some(700)),
            depot(482, Some(600)),
            depot(484, Some(700)),
            depot(485, Some(480)),
        ];
        let plan = plan_from(
            &details,
            Path::new("game"),
            &platform("windows"),
            PUBLIC_BRANCH,
            None,
        )
        .unwrap();
        assert_eq!(plan.dlc_app_ids, [600, 700]);
    }

    fn proton(name: &str, official: bool) -> ProtonInstall {
        ProtonInstall {
            name: name.to_owned(),
            dir: absolute(&["tools", name]),
            source: if official {
                ProtonSource::SteamLibrary(absolute(&["lib"]))
            } else {
                ProtonSource::CompatibilityTool
            },
            version: None,
            required_tool_app_id: None,
        }
    }

    /// An absolute path on every OS (`C:\a\b` or `/a/b`).
    fn absolute(parts: &[&str]) -> PathBuf {
        let mut path = if cfg!(windows) {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        };
        path.extend(parts);
        path
    }

    #[test]
    fn chooses_the_newest_official_proton() {
        let installs = [
            proton("GE-Proton9-20", false),
            proton("Proton Experimental", true),
            proton("Proton 9.0", true),
            proton("Proton 10.0", true),
        ];
        assert_eq!(choose_proton(&installs).unwrap().name, "Proton 10.0");
        assert_eq!(
            choose_proton(&installs[..2]).unwrap().name,
            "Proton Experimental"
        );
        assert_eq!(choose_proton(&installs[..1]).unwrap().name, "GE-Proton9-20");
        assert!(choose_proton(&[]).is_none());
    }

    #[test]
    fn the_chosen_proton_wins_if_installed() {
        let installs = [proton("GE-Proton9-20", false), proton("Proton 9.0", true)];
        assert_eq!(
            pick_proton(&installs, Some("GE-Proton9-20")).unwrap().name,
            "GE-Proton9-20"
        );
        assert_eq!(
            pick_proton(&installs, Some("Proton 1.0")).unwrap().name,
            "Proton 9.0",
            "falls back to the automatic choice"
        );
        assert_eq!(pick_proton(&installs, None).unwrap().name, "Proton 9.0");
    }

    #[test]
    fn runs_windows_games_through_proton() {
        let mut windows = option(0, "Game.exe", "windows");
        windows.arguments = Some("-windowed".to_owned());
        let details = app(vec![windows]);
        let game_dir = absolute(&["lib", "steamapps", "common", "Game"]);
        let plan = plan_from(
            &details,
            &game_dir,
            &platform("windows"),
            PUBLIC_BRANCH,
            None,
        )
        .unwrap();
        let install = proton("Proton 9.0", true);
        let compat = absolute(&["data", "compatdata", "480"]);
        let plan = with_proton(plan, &install, &compat, None).unwrap();

        assert_eq!(plan.program, install.dir.join("proton"));
        let game = game_dir.join("Game.exe");
        assert_eq!(
            plan.args,
            [
                OsString::from("run"),
                game.clone().into_os_string(),
                OsString::from("-windowed")
            ]
        );
        assert_eq!(plan.executable, game, "the game file is still checked");
        assert_eq!(plan.working_dir, game_dir);
        assert_eq!(plan.proton.as_deref(), Some("Proton 9.0"));
        assert_eq!(plan.create_dir.as_deref(), Some(compat.as_path()));
        let env = |key: &str| {
            plan.env
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(env("STEAM_COMPAT_DATA_PATH"), Some(compat.into_os_string()));
        assert_eq!(env("SteamAppId"), Some("480".into()));
    }

    #[test]
    fn only_fully_installed_apps_can_be_launched() {
        let library = std::env::temp_dir().join(format!("reborn-launch-{}", std::process::id()));
        std::fs::create_dir_all(library.join("steamapps")).unwrap();
        assert!(matches!(
            installed_game_dir(&library, 480),
            Err(CoreError::NotInstalled(480))
        ));
        let manifest = |flags: u32, dir: &str| {
            format!(
                "\"AppState\"\n{{\n\t\"appid\"\t\t\"480\"\n\t\"name\"\t\t\"Spacewar\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"{dir}\"\n}}\n"
            )
        };
        let path = app_manifest_path(&library, 480);
        std::fs::write(&path, manifest(4, "Spacewar")).unwrap();
        assert_eq!(
            installed_game_dir(&library, 480).unwrap(),
            library.join("steamapps").join("common").join("Spacewar")
        );
        std::fs::write(&path, manifest(1026, "Spacewar")).unwrap();
        assert!(matches!(
            installed_game_dir(&library, 480),
            Err(CoreError::NotInstalled(480))
        ));
        std::fs::write(&path, manifest(4, "..")).unwrap();
        assert!(matches!(
            installed_game_dir(&library, 480),
            Err(CoreError::UnsafeInstallDir(_))
        ));
        std::fs::remove_dir_all(&library).unwrap();
    }
}
