//! Proton on Linux: finding installed builds and building the command line
//! that runs a Windows program through one of them.
//!
//! Nothing here starts a process or writes a file. [`discover`] only reads
//! directories; [`proton_command`] is a pure function that returns the
//! program, arguments, environment and working directory for the caller to
//! spawn. All search roots are parameters, so tests run against temporary
//! directories. The module compiles on every OS; Proton itself only runs on
//! Linux.
//!
//! # Sources
//!
//! Every behaviour cites one of these, by file and line (see also
//! `docs/platform/proton.md`):
//!
//! - **Valve's Proton**, <https://github.com/ValveSoftware/Proton>, branch
//!   `proton_10.0` at commit `e91ca2be`, BSD-3-Clause (`LICENSE.proton`):
//!   the `proton` launcher script, `README.md`,
//!   `compatibilitytool.vdf.template` and `toolmanifest_runtime.vdf`.
//!   Only read; no code is copied.
//! - **umu-launcher**, <https://github.com/Open-Wine-Components/umu-launcher>,
//!   commit `e2b203a1`, GPL-3.0: `umu/umu_run.py`, `umu/umu_consts.py` and
//!   `umu/umu_runtime.py`. Read-only reference.
//!
//! # Not covered (not verified)
//!
//! - Modern Proton builds declare in `toolmanifest.vdf` that they must run
//!   inside the Steam Linux Runtime container (`require_tool_appid`,
//!   reported as [`ProtonInstall::required_tool_app_id`]). Wrapping the
//!   command in that runtime (pressure-vessel) is not done here; whether a
//!   given build works without it is unverified.
//! - Proton starts the game through its own `steam.exe` helper (`proton`
//!   lines 1965-1973). How that helper reaches a Steam client (plan section
//!   8 assumes Proton's `lsteamclient` and the native `steamclient.so`) is
//!   not verified here, so Steamworks games may still need the official
//!   client until the shim exists (M4).
//! - `STEAM_COMPAT_APP_ID`, `STEAM_COMPAT_INSTALL_PATH`,
//!   `STEAM_COMPAT_LIBRARY_PATHS`, `STEAM_COMPAT_CONFIG` and the shader
//!   cache variables are not set: their exact values as set by the official
//!   client could not be verified from the sources above.
//! - Tool manifests placed directly in `compatibilitytools.d` (instead of in
//!   a subdirectory, see `compatibilitytool.vdf.template` lines 7-13) are not
//!   read.

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use reborn_formats::LibraryFolders;
use reborn_formats::vdf::{self, Object};

/// Name of the launcher script inside a Proton directory.
///
/// Source: Proton `README.md` lines 208-216 (layout of an installed tool)
/// and `toolmanifest_runtime.vdf` (`"commandline" "/proton %verb%"`).
const SCRIPT: &str = "proton";

/// Prefix of Proton directories in `steamapps/common`.
///
/// Source: Proton `README.md` line 293 ("`~/.steam/steam/steamapps/common/Proton #.#`").
/// Other Valve builds share the prefix; every match must also contain the
/// [`SCRIPT`], so unrelated folders are not picked up.
const LIBRARY_PREFIX: &str = "Proton";

/// Where to look for Proton builds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchRoots {
    /// Steam installations. Their `steamapps/libraryfolders.vdf` adds more
    /// libraries; the root itself is a library too.
    pub steam_roots: Vec<PathBuf>,
    /// Library folders (each containing `steamapps/common`), searched in
    /// addition to those of the Steam roots, for example Reborn's own library.
    pub libraries: Vec<PathBuf>,
    /// `compatibilitytools.d` directories with custom builds such as GE-Proton.
    pub compat_tool_dirs: Vec<PathBuf>,
}

impl SearchRoots {
    /// The usual locations on Linux, for the given home directory and
    /// `XDG_DATA_HOME` value. Pure: reads no environment and no files.
    ///
    /// - Steam roots `~/.steam/root` (Proton `README.md` line 141) and
    ///   `~/.steam/steam` (`README.md` line 293). Both are often symlinks to
    ///   the same place; [`discover`] removes duplicates.
    /// - `~/.steam/root/compatibilitytools.d` (`README.md` lines 141 and 198).
    /// - `$XDG_DATA_HOME/Steam/compatibilitytools.d`, with `XDG_DATA_HOME`
    ///   defaulting to `~/.local/share` (umu-launcher `umu_consts.py` lines
    ///   82-86 and 104). A relative or empty `XDG_DATA_HOME` is ignored, as
    ///   the XDG Base Directory Specification requires.
    pub fn linux_defaults(home: &Path, xdg_data_home: Option<&Path>) -> Self {
        let data_home = xdg_data_home
            .filter(|path| path.is_absolute())
            .map_or_else(|| home.join(".local").join("share"), Path::to_path_buf);
        let steam_root = home.join(".steam").join("root");
        Self {
            steam_roots: vec![steam_root.clone(), home.join(".steam").join("steam")],
            libraries: Vec::new(),
            compat_tool_dirs: vec![
                steam_root.join("compatibilitytools.d"),
                data_home.join("Steam").join("compatibilitytools.d"),
            ],
        }
    }

    /// [`linux_defaults`](Self::linux_defaults) for the current user (`HOME`
    /// and `XDG_DATA_HOME`), plus Reborn's own default library. Empty
    /// if `HOME` is not set, which is normal on Windows.
    pub fn from_environment() -> Self {
        let Some(home) = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
        else {
            return Self::default();
        };
        let xdg_data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let mut roots = Self::linux_defaults(&home, xdg_data_home.as_deref());
        if let Ok(library) = crate::settings::default_library() {
            roots.libraries.push(library);
        }
        roots
    }
}

/// Where a Proton build was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtonSource {
    /// `steamapps/common/Proton*` in this library folder (a Valve build,
    /// installed as a Steam app).
    SteamLibrary(PathBuf),
    /// A subdirectory of a `compatibilitytools.d` directory (a custom build).
    CompatibilityTool,
}

/// An installed Proton build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtonInstall {
    /// Name to show: `display_name` from `compatibilitytool.vdf` if there is
    /// one, otherwise the directory name.
    pub name: String,
    /// The Proton directory, containing the `proton` script.
    pub dir: PathBuf,
    /// Where it was found.
    pub source: ProtonSource,
    /// First line of the `version` file, if present. Opaque text; Proton
    /// itself only logs it (`proton` lines 525 and 1847).
    pub version: Option<String>,
    /// App ID of the runtime this build requires (`require_tool_appid` in
    /// `toolmanifest.vdf`), if declared. Valve's Proton 10.0 declares 1628350
    /// (`toolmanifest_runtime.vdf`), the Steam Linux Runtime 3.0 "sniper"
    /// (umu-launcher `umu_runtime.py` line 524).
    pub required_tool_app_id: Option<u32>,
}

impl ProtonInstall {
    /// Path of the `proton` launcher script.
    pub fn script(&self) -> PathBuf {
        self.dir.join(SCRIPT)
    }
}

/// Result of [`discover`].
#[derive(Debug, Default)]
pub struct Discovery {
    /// Proton builds found, without duplicates, in search order: Steam
    /// libraries first, then `compatibilitytools.d`; by directory name
    /// within one directory.
    pub installs: Vec<ProtonInstall>,
    /// Problems that did not stop the search, for example an unreadable
    /// library or a broken `compatibilitytool.vdf`. Missing directories are
    /// not problems.
    pub problems: Vec<ProtonError>,
}

/// Finds installed Proton builds. Only reads; missing roots are skipped.
pub fn discover(roots: &SearchRoots) -> Discovery {
    let mut discovery = Discovery::default();
    let mut libraries = Vec::new();
    for root in &roots.steam_roots {
        match library_folders(root) {
            Ok(found) => libraries.extend(found),
            Err(problem) => discovery.problems.push(problem),
        }
    }
    libraries.extend(roots.libraries.iter().cloned());

    let mut seen = HashSet::new();
    let mut add = |discovery: &mut Discovery, install: ProtonInstall| {
        if seen.insert(identity(&install.dir)) {
            discovery.installs.push(install);
        }
    };

    let mut seen_libraries = HashSet::new();
    for library in libraries {
        if !seen_libraries.insert(identity(&library)) {
            continue;
        }
        let common = library.join("steamapps").join("common");
        for dir in subdirectories(&common, &mut discovery.problems) {
            let is_proton = dir
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(LIBRARY_PREFIX));
            if !is_proton || !dir.join(SCRIPT).is_file() {
                continue;
            }
            let install = read_install(
                dir,
                ProtonSource::SteamLibrary(library.clone()),
                None,
                &mut discovery.problems,
            );
            add(&mut discovery, install);
        }
    }

    for tools in &roots.compat_tool_dirs {
        for dir in subdirectories(tools, &mut discovery.problems) {
            if !dir.join(SCRIPT).is_file() {
                continue;
            }
            let tool = match read_compatibility_tool(&dir) {
                Ok(tool) => tool,
                Err(problem) => {
                    discovery.problems.push(problem);
                    None
                }
            };
            if tool.as_ref().is_some_and(|tool| !tool.from_windows) {
                continue;
            }
            let name = tool.and_then(|tool| tool.display_name);
            let install = read_install(
                dir,
                ProtonSource::CompatibilityTool,
                name,
                &mut discovery.problems,
            );
            add(&mut discovery, install);
        }
    }
    discovery
}

/// The library folders of a Steam installation: the root itself, then every
/// folder listed in `<root>/steamapps/libraryfolders.vdf`, without
/// duplicates. Empty if the root does not exist.
pub fn library_folders(steam_root: &Path) -> Result<Vec<PathBuf>, ProtonError> {
    if !steam_root.is_dir() {
        return Ok(Vec::new());
    }
    let path = steam_root.join("steamapps").join("libraryfolders.vdf");
    let mut folders = vec![steam_root.to_path_buf()];
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(folders),
        Err(source) => return Err(ProtonError::Io { path, source }),
    };
    let parsed = LibraryFolders::parse(&text).map_err(|source| ProtonError::LibraryFolders {
        path: path.clone(),
        source,
    })?;
    let mut seen: HashSet<PathBuf> = HashSet::from([identity(steam_root)]);
    for folder in parsed.folders {
        let folder = PathBuf::from(folder.path);
        if folder.is_absolute() && seen.insert(identity(&folder)) {
            folders.push(folder);
        }
    }
    Ok(folders)
}

/// A path's identity for removing duplicates: the canonical path if it
/// exists (resolves `~/.steam/root` and `~/.steam/steam` symlinks), else the
/// path itself.
fn identity(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Subdirectories of `dir`, sorted by name. A missing `dir` yields nothing;
/// other errors are recorded.
fn subdirectories(dir: &Path, problems: &mut Vec<ProtonError>) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(source) => {
            problems.push(ProtonError::Io {
                path: dir.to_path_buf(),
                source,
            });
            return Vec::new();
        }
    };
    let mut dirs = Vec::new();
    for entry in entries {
        match entry {
            // `is_dir` follows symlinks, like Steam's own symlinked folders.
            Ok(entry) if entry.path().is_dir() => dirs.push(entry.path()),
            Ok(_) => {}
            Err(source) => problems.push(ProtonError::Io {
                path: dir.to_path_buf(),
                source,
            }),
        }
    }
    dirs.sort();
    dirs
}

fn read_install(
    dir: PathBuf,
    source: ProtonSource,
    display_name: Option<String>,
    problems: &mut Vec<ProtonError>,
) -> ProtonInstall {
    let name = display_name.unwrap_or_else(|| {
        // Fallback when there is no display name, as umu-launcher does
        // (`umu_runtime.py` line 565).
        dir.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    });
    let version = match read_optional(&dir.join("version")) {
        Ok(text) => text
            .and_then(|text| text.lines().next().map(str::trim).map(str::to_owned))
            .filter(|line| !line.is_empty()),
        Err(problem) => {
            problems.push(problem);
            None
        }
    };
    let required_tool_app_id = match read_tool_manifest(&dir) {
        Ok(app_id) => app_id,
        Err(problem) => {
            problems.push(problem);
            None
        }
    };
    ProtonInstall {
        name,
        dir,
        source,
        version,
        required_tool_app_id,
    }
}

/// Reads a text file; `Ok(None)` if it does not exist.
fn read_optional(path: &Path) -> Result<Option<String>, ProtonError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ProtonError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn read_vdf(path: &Path) -> Result<Option<Object>, ProtonError> {
    let Some(text) = read_optional(path)? else {
        return Ok(None);
    };
    vdf::parse(&text)
        .map(Some)
        .map_err(|source| ProtonError::Manifest {
            path: path.to_path_buf(),
            problem: source.to_string(),
        })
}

/// `require_tool_appid` from `toolmanifest.vdf`.
///
/// Source: Proton `toolmanifest_runtime.vdf` (`"manifest"` with
/// `"require_tool_appid" "1628350"`); umu-launcher reads the same file and
/// root key (`umu_runtime.py` lines 553-554).
fn read_tool_manifest(dir: &Path) -> Result<Option<u32>, ProtonError> {
    let path = dir.join("toolmanifest.vdf");
    let Some(document) = read_vdf(&path)? else {
        return Ok(None);
    };
    let invalid = |problem: &str| ProtonError::Manifest {
        path: path.clone(),
        problem: problem.to_owned(),
    };
    let manifest = document
        .get_object("manifest")
        .ok_or_else(|| invalid("no \"manifest\" section"))?;
    match manifest.get_str("require_tool_appid") {
        None => Ok(None),
        Some(text) => text
            .parse()
            .map(Some)
            .map_err(|_| invalid("require_tool_appid is not a number")),
    }
}

/// What `compatibilitytool.vdf` says about a tool.
struct CompatibilityTool {
    display_name: Option<String>,
    /// Whether the tool runs Windows programs (`from_oslist` contains
    /// `windows`, or is absent).
    from_windows: bool,
}

/// Reads `<dir>/compatibilitytool.vdf`; `Ok(None)` if there is none.
///
/// Layout from Proton's `compatibilitytool.vdf.template`:
/// `"compatibilitytools" { "compat_tools" { "<internal name>" {
/// "install_path" "." "display_name" "..." "from_oslist" "windows"
/// "to_oslist" "linux" } } }`. A file may define several tools; like
/// umu-launcher (`umu_runtime.py` lines 556-563) the first one is used.
fn read_compatibility_tool(dir: &Path) -> Result<Option<CompatibilityTool>, ProtonError> {
    let path = dir.join("compatibilitytool.vdf");
    let Some(document) = read_vdf(&path)? else {
        return Ok(None);
    };
    let tool = document
        .get_object("compatibilitytools")
        .and_then(|root| root.get_object("compat_tools"))
        .and_then(|tools| tools.iter().find_map(|(_, value)| value.as_object()))
        .ok_or_else(|| ProtonError::Manifest {
            path: path.clone(),
            problem: "no tool in \"compatibilitytools\" / \"compat_tools\"".to_owned(),
        })?;
    let display_name = tool
        .get_str("display_name")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    // The template writes a single OS name; splitting on commas also accepts
    // a list without guessing at more.
    let from_windows = tool.get_str("from_oslist").is_none_or(|list| {
        list.split(',')
            .any(|os| os.trim().eq_ignore_ascii_case("windows"))
    });
    Ok(Some(CompatibilityTool {
        display_name,
        from_windows,
    }))
}

/// The Proton verb, the first argument of the `proton` script.
///
/// Source: `proton` lines 2009-2020 (the verbs Proton accepts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProtonVerb {
    /// `run`: start the program (`proton` lines 2009-2013).
    #[default]
    Run,
    /// `waitforexitandrun`: wait until the prefix's wineserver has exited,
    /// then start the program (`proton` lines 2014-2018). umu-launcher's
    /// default verb (`umu_run.py` lines 233-236).
    WaitForExitAndRun,
}

impl ProtonVerb {
    /// The verb as passed to the script.
    pub fn as_str(self) -> &'static str {
        match self {
            ProtonVerb::Run => "run",
            ProtonVerb::WaitForExitAndRun => "waitforexitandrun",
        }
    }
}

/// What to run through Proton. Every path must be absolute.
#[derive(Clone, Copy, Debug)]
pub struct ProtonLaunch<'a> {
    /// The Proton directory (see [`ProtonInstall::dir`]).
    pub proton_dir: &'a Path,
    /// The verb; [`ProtonVerb::Run`] by default.
    pub verb: ProtonVerb,
    /// The Windows executable, as a Linux path.
    pub executable: &'a Path,
    /// Arguments for the executable.
    pub args: &'a [String],
    /// The app's own compatibility data directory, created by the caller,
    /// one per app. Proton keeps the Wine prefix in its `pfx` subdirectory.
    pub compat_data_dir: &'a Path,
    /// An official Steam client installation, if there is one. Proton copies
    /// a few client files from it into the prefix when they exist.
    pub steam_client_dir: Option<&'a Path>,
    /// The app ID.
    pub app_id: u32,
    /// Working directory of the game.
    pub working_dir: &'a Path,
}

/// A fully built command: what to spawn, with which environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtonCommand {
    /// The program to execute: the `proton` script.
    pub program: PathBuf,
    /// Its arguments: the verb, the executable, then the game's arguments.
    pub args: Vec<OsString>,
    /// Variables to set on top of the inherited environment.
    pub env: BTreeMap<String, OsString>,
    /// The working directory.
    pub current_dir: PathBuf,
}

/// Builds the command that runs `launch.executable` through Proton:
/// `<proton dir>/proton <verb> <executable> <args...>`.
///
/// Pure: checks only that the paths are absolute, touches no file and
/// starts nothing.
///
/// Environment, each with its source:
///
/// - `STEAM_COMPAT_DATA_PATH` = `compat_data_dir`. Proton exits without it
///   (`proton` lines 1987-1989) and keeps the prefix in `<it>/pfx/`
///   (`proton` lines 585-588).
/// - `STEAM_COMPAT_CLIENT_INSTALL_PATH` = `steam_client_dir`, or empty.
///   Proton reads it unconditionally while setting up a prefix (`proton`
///   line 949) and copies `legacycompat/*` files from it only if they exist
///   (lines 1036-1047); umu-launcher sets it to an empty string
///   (`umu_run.py` line 843, exported at lines 1039-1041).
/// - `SteamAppId` and `SteamGameId` = the app ID. Proton uses both for
///   per-game workarounds (`proton` lines 871 and 1195); umu-launcher sets
///   both to the same ID (`umu_run.py` lines 281-282).
///
/// The program path must be absolute because Proton finds its own files
/// relative to its script (`proton` line 1991) and the working directory is
/// changed. The executable must be absolute because Proton fails otherwise
/// (`umu_run.py` line 254). Proton does not change the working directory
/// itself; the game's working directory is simply where it is started.
pub fn proton_command(launch: &ProtonLaunch<'_>) -> Result<ProtonCommand, ProtonError> {
    let absolute = |what: &'static str, path: &Path| {
        if path.is_absolute() {
            Ok(())
        } else {
            Err(ProtonError::RelativePath {
                what,
                path: path.to_path_buf(),
            })
        }
    };
    absolute("Proton directory", launch.proton_dir)?;
    absolute("executable", launch.executable)?;
    absolute("compat data directory", launch.compat_data_dir)?;
    absolute("working directory", launch.working_dir)?;
    if let Some(client) = launch.steam_client_dir {
        absolute("Steam client directory", client)?;
    }

    let mut args = vec![
        OsString::from(launch.verb.as_str()),
        launch.executable.as_os_str().to_owned(),
    ];
    args.extend(launch.args.iter().map(OsString::from));

    let app_id = OsString::from(launch.app_id.to_string());
    let env = BTreeMap::from([
        (
            "STEAM_COMPAT_DATA_PATH".to_owned(),
            launch.compat_data_dir.as_os_str().to_owned(),
        ),
        (
            "STEAM_COMPAT_CLIENT_INSTALL_PATH".to_owned(),
            launch
                .steam_client_dir
                .map(|dir| dir.as_os_str().to_owned())
                .unwrap_or_default(),
        ),
        ("SteamAppId".to_owned(), app_id.clone()),
        ("SteamGameId".to_owned(), app_id),
    ]);

    Ok(ProtonCommand {
        program: launch.proton_dir.join(SCRIPT),
        args,
        env,
        current_dir: launch.working_dir.to_path_buf(),
    })
}

/// Errors while finding Proton builds or building a command.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ProtonError {
    /// A file or directory could not be read.
    #[error("{}: {source}", path.display())]
    Io {
        /// The file or directory.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// `libraryfolders.vdf` is invalid.
    #[error("{}: {source}", path.display())]
    LibraryFolders {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        source: reborn_formats::Error,
    },
    /// `compatibilitytool.vdf` or `toolmanifest.vdf` is invalid.
    #[error("{}: {problem}", path.display())]
    Manifest {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        problem: String,
    },
    /// A path given to [`proton_command`] is relative.
    #[error("the {what} must be an absolute path, got {}", path.display())]
    RelativePath {
        /// Which path.
        what: &'static str,
        /// The path.
        path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique scratch directory, removed on drop.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-proton-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let dir = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self, relative: &str) -> PathBuf {
            self.0.join(relative)
        }

        fn write(&self, relative: &str, text: &str) -> PathBuf {
            let path = self.path(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }

        /// A fake Proton directory: just the files discovery looks at.
        fn proton(&self, relative: &str) -> PathBuf {
            self.write(&format!("{relative}/proton"), "#!/usr/bin/env python3\n");
            self.path(relative)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn library_folders_vdf(paths: &[&Path]) -> String {
        let mut folders = Object::new();
        for (index, path) in paths.iter().enumerate() {
            let mut entry = Object::new();
            entry.push("path", path.to_string_lossy().into_owned());
            entry.push("label", "");
            folders.push(index.to_string(), entry);
        }
        let mut document = Object::new();
        document.push("libraryfolders", folders);
        vdf::write(&document)
    }

    const GE_TOOL: &str = r#""compatibilitytools"
{
  "compat_tools"
  {
    "GE-Proton-Test"
    {
      "install_path" "."
      "display_name" "GE-Proton Test Build"
      "from_oslist"  "windows"
      "to_oslist"    "linux"
    }
  }
}
"#;

    const TOOL_MANIFEST: &str = r#""manifest"
{
  "version" "2"
  "commandline" "/proton %verb%"
  "require_tool_appid" "1628350"
  "use_sessions" "1"
}
"#;

    #[test]
    fn linux_defaults() {
        let home = Path::new("/home/user");
        let roots = SearchRoots::linux_defaults(home, None);
        assert_eq!(
            roots.steam_roots,
            vec![home.join(".steam/root"), home.join(".steam/steam")]
        );
        assert!(roots.libraries.is_empty());
        assert_eq!(
            roots.compat_tool_dirs,
            vec![
                home.join(".steam/root/compatibilitytools.d"),
                home.join(".local/share/Steam/compatibilitytools.d"),
            ]
        );
    }

    #[test]
    fn linux_defaults_respect_absolute_xdg_data_home_only() {
        let home = std::env::temp_dir().join("home");
        let data = std::env::temp_dir().join("data");
        let roots = SearchRoots::linux_defaults(&home, Some(&data));
        assert_eq!(
            roots.compat_tool_dirs[1],
            data.join("Steam").join("compatibilitytools.d")
        );
        for ignored in [Path::new("relative/data"), Path::new("")] {
            let roots = SearchRoots::linux_defaults(&home, Some(ignored));
            assert_eq!(
                roots.compat_tool_dirs[1],
                home.join(".local/share/Steam/compatibilitytools.d")
            );
        }
    }

    #[test]
    fn finds_proton_in_steam_libraries() {
        let temp = TempDir::new("libraries");
        let root = temp.path("steam");
        let second = temp.path("games");
        temp.write(
            "steam/steamapps/libraryfolders.vdf",
            &library_folders_vdf(&[&root, &second]),
        );
        let proton9 = temp.proton("steam/steamapps/common/Proton 9.0");
        temp.write(
            "steam/steamapps/common/Proton 9.0/version",
            "1700000000 proton-9.0-test\nmore\n",
        );
        temp.write(
            "steam/steamapps/common/Proton 9.0/toolmanifest.vdf",
            TOOL_MANIFEST,
        );
        let experimental = temp.proton("games/steamapps/common/Proton - Experimental");
        // Not Proton: wrong prefix, or no script.
        temp.proton("steam/steamapps/common/Some Game");
        temp.write("steam/steamapps/common/Proton 8.0/version", "x");

        let roots = SearchRoots {
            steam_roots: vec![root.clone()],
            ..SearchRoots::default()
        };
        let discovery = discover(&roots);
        assert!(discovery.problems.is_empty(), "{:?}", discovery.problems);
        assert_eq!(
            discovery.installs,
            vec![
                ProtonInstall {
                    name: "Proton 9.0".to_owned(),
                    dir: proton9,
                    source: ProtonSource::SteamLibrary(root),
                    version: Some("1700000000 proton-9.0-test".to_owned()),
                    required_tool_app_id: Some(1628350),
                },
                ProtonInstall {
                    name: "Proton - Experimental".to_owned(),
                    dir: experimental,
                    source: ProtonSource::SteamLibrary(second),
                    version: None,
                    required_tool_app_id: None,
                },
            ]
        );
    }

    #[test]
    fn finds_custom_builds_in_compatibilitytools_d() {
        let temp = TempDir::new("tools");
        let ge = temp.proton("tools/GE-Proton-Test");
        temp.write("tools/GE-Proton-Test/compatibilitytool.vdf", GE_TOOL);
        // No compatibilitytool.vdf: falls back to the directory name.
        let plain = temp.proton("tools/my_proton");
        // Not a Windows compatibility tool.
        temp.proton("tools/linux-tool");
        temp.write(
            "tools/linux-tool/compatibilitytool.vdf",
            &GE_TOOL.replace("\"windows\"", "\"linux\""),
        );
        // No proton script.
        temp.write("tools/runtime/compatibilitytool.vdf", GE_TOOL);
        // A plain file in compatibilitytools.d.
        temp.write("tools/stray.vdf", GE_TOOL);

        let roots = SearchRoots {
            compat_tool_dirs: vec![temp.path("tools"), temp.path("missing")],
            ..SearchRoots::default()
        };
        let discovery = discover(&roots);
        assert!(discovery.problems.is_empty(), "{:?}", discovery.problems);
        let found: Vec<_> = discovery
            .installs
            .iter()
            .map(|install| (install.name.as_str(), install.dir.clone(), &install.source))
            .collect();
        assert_eq!(
            found,
            vec![
                ("GE-Proton Test Build", ge, &ProtonSource::CompatibilityTool),
                ("my_proton", plain, &ProtonSource::CompatibilityTool),
            ]
        );
    }

    #[test]
    fn broken_manifests_are_problems_not_failures() {
        let temp = TempDir::new("broken");
        let dir = temp.proton("tools/Broken");
        temp.write(
            "tools/Broken/compatibilitytool.vdf",
            "\"compatibilitytools\" {",
        );
        temp.write(
            "tools/Broken/toolmanifest.vdf",
            "\"manifest\" { \"require_tool_appid\" \"abc\" }",
        );
        let roots = SearchRoots {
            compat_tool_dirs: vec![temp.path("tools")],
            ..SearchRoots::default()
        };
        let discovery = discover(&roots);
        assert_eq!(discovery.installs.len(), 1);
        assert_eq!(discovery.installs[0].name, "Broken");
        assert_eq!(discovery.installs[0].dir, dir);
        assert_eq!(discovery.installs[0].required_tool_app_id, None);
        assert_eq!(discovery.problems.len(), 2, "{:?}", discovery.problems);
        assert!(
            discovery
                .problems
                .iter()
                .all(|problem| matches!(problem, ProtonError::Manifest { .. }))
        );
    }

    #[test]
    fn duplicates_are_removed() {
        let temp = TempDir::new("dupes");
        let root = temp.path("steam");
        temp.write(
            "steam/steamapps/libraryfolders.vdf",
            &library_folders_vdf(&[&root, &root]),
        );
        temp.proton("steam/steamapps/common/Proton 9.0");
        let roots = SearchRoots {
            steam_roots: vec![root.clone(), root.clone()],
            libraries: vec![root.clone()],
            ..SearchRoots::default()
        };
        let discovery = discover(&roots);
        assert!(discovery.problems.is_empty(), "{:?}", discovery.problems);
        assert_eq!(discovery.installs.len(), 1);
    }

    #[test]
    fn library_folders_of_a_steam_root() {
        let temp = TempDir::new("folders");
        let root = temp.path("steam");
        assert_eq!(library_folders(&root).unwrap(), Vec::<PathBuf>::new());

        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(library_folders(&root).unwrap(), vec![root.clone()]);

        let other = temp.path("other");
        temp.write(
            "steam/steamapps/libraryfolders.vdf",
            &library_folders_vdf(&[&root, &other, Path::new("relative")]),
        );
        assert_eq!(library_folders(&root).unwrap(), vec![root.clone(), other]);

        temp.write("steam/steamapps/libraryfolders.vdf", "\"libraryfolders\" {");
        assert!(matches!(
            library_folders(&root),
            Err(ProtonError::LibraryFolders { .. })
        ));
        // A broken libraryfolders.vdf is reported, other roots still searched.
        let tools = temp.proton("tools/my_proton");
        let discovery = discover(&SearchRoots {
            steam_roots: vec![root],
            compat_tool_dirs: vec![temp.path("tools")],
            ..SearchRoots::default()
        });
        assert_eq!(discovery.problems.len(), 1);
        assert_eq!(discovery.installs.len(), 1);
        assert_eq!(discovery.installs[0].dir, tools);
    }

    fn launch_paths(temp: &TempDir) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        (
            temp.path("Proton 9.0"),
            temp.path("library/steamapps/common/Game/Game.exe"),
            temp.path("compatdata/12345"),
            temp.path("library/steamapps/common/Game"),
        )
    }

    #[test]
    fn builds_the_proton_command() {
        let temp = TempDir::new("command");
        let (proton, exe, compat, cwd) = launch_paths(&temp);
        let args = vec!["-windowed".to_owned(), "two words".to_owned()];
        let command = proton_command(&ProtonLaunch {
            proton_dir: &proton,
            verb: ProtonVerb::Run,
            executable: &exe,
            args: &args,
            compat_data_dir: &compat,
            steam_client_dir: None,
            app_id: 12345,
            working_dir: &cwd,
        })
        .unwrap();
        assert_eq!(command.program, proton.join("proton"));
        assert_eq!(
            command.args,
            vec![
                OsString::from("run"),
                exe.clone().into_os_string(),
                OsString::from("-windowed"),
                OsString::from("two words"),
            ]
        );
        assert_eq!(command.current_dir, cwd);
        assert_eq!(
            command.env,
            BTreeMap::from([
                ("STEAM_COMPAT_DATA_PATH".to_owned(), compat.into_os_string()),
                (
                    "STEAM_COMPAT_CLIENT_INSTALL_PATH".to_owned(),
                    OsString::new()
                ),
                ("SteamAppId".to_owned(), OsString::from("12345")),
                ("SteamGameId".to_owned(), OsString::from("12345")),
            ])
        );
        // Pure: nothing was created.
        assert!(!temp.path("compatdata").exists());
    }

    #[test]
    fn command_with_client_dir_and_other_verb() {
        let temp = TempDir::new("client");
        let (proton, exe, compat, cwd) = launch_paths(&temp);
        let client = temp.path("Steam");
        let command = proton_command(&ProtonLaunch {
            proton_dir: &proton,
            verb: ProtonVerb::WaitForExitAndRun,
            executable: &exe,
            args: &[],
            compat_data_dir: &compat,
            steam_client_dir: Some(&client),
            app_id: 7,
            working_dir: &cwd,
        })
        .unwrap();
        assert_eq!(
            command.args,
            vec![OsString::from("waitforexitandrun"), exe.into_os_string()]
        );
        assert_eq!(
            command.env["STEAM_COMPAT_CLIENT_INSTALL_PATH"],
            client.into_os_string()
        );
        assert_eq!(ProtonVerb::default(), ProtonVerb::Run);
    }

    #[test]
    fn command_rejects_relative_paths() {
        let temp = TempDir::new("relative");
        let (proton, exe, compat, cwd) = launch_paths(&temp);
        let relative = Path::new("relative/path");
        let base = ProtonLaunch {
            proton_dir: &proton,
            verb: ProtonVerb::Run,
            executable: &exe,
            args: &[],
            compat_data_dir: &compat,
            steam_client_dir: None,
            app_id: 1,
            working_dir: &cwd,
        };
        let cases = [
            (
                "Proton directory",
                ProtonLaunch {
                    proton_dir: relative,
                    ..base
                },
            ),
            (
                "executable",
                ProtonLaunch {
                    executable: relative,
                    ..base
                },
            ),
            (
                "compat data directory",
                ProtonLaunch {
                    compat_data_dir: relative,
                    ..base
                },
            ),
            (
                "working directory",
                ProtonLaunch {
                    working_dir: relative,
                    ..base
                },
            ),
            (
                "Steam client directory",
                ProtonLaunch {
                    steam_client_dir: Some(relative),
                    ..base
                },
            ),
        ];
        for (expected, launch) in cases {
            match proton_command(&launch) {
                Err(ProtonError::RelativePath { what, path }) => {
                    assert_eq!(what, expected);
                    assert_eq!(path, relative);
                }
                other => panic!("{expected}: {other:?}"),
            }
        }
    }
}
