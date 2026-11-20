//! Installing, updating and verifying an app in a library folder.
//!
//! The flow: app details from PICS → depots for this platform → existing
//! appmanifest (installed manifests) → depot keys and manifests (new and
//! installed) → `reborn_content::install` → appmanifest written or updated.

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reborn_cm::{AppDetails, Platform, Session};
use reborn_content::DepotKey;
use reborn_content::cdn::{CdnClient, CdnHost};
use reborn_content::install::{
    CdnSource, DepotDownload, InstallOptions, InstallReport, InstallTarget, Progress,
};
use reborn_content::manifest::DepotManifest;
use reborn_content::path::install_path;
use reborn_formats::{InstalledDepot, NewAppManifest, vdf};
use reborn_platform::settings;
use tracing::{info, warn};

use crate::CoreError;
use crate::error::io_error;
use crate::library::{app_manifest_path, is_steam_library, read_app_manifest};

/// What to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallRequest {
    /// The app.
    pub app_id: u32,
    /// Library folder (contains `steamapps`); `None` for Reborn's own library.
    pub library: Option<PathBuf>,
    /// Branch, usually [`reborn_cm::PUBLIC_BRANCH`].
    pub branch: String,
    /// Hash every installed file and repair broken ones.
    pub verify: bool,
}

/// Which kind of run this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallAction {
    /// Nothing installed yet.
    Install,
    /// An older version is installed.
    Update,
    /// Checking (and repairing) the installed version.
    Verify,
}

/// Progress notifications for frontends.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum InstallEvent {
    /// The library is a Steam library; the official client must be closed.
    SteamLibrary(PathBuf),
    /// The app has no Linux version; its Windows depots are installed to
    /// run it through Proton (ADR 0017).
    WindowsDepotsForProton,
    /// Work starts.
    Started {
        /// Install, update or verify.
        action: InstallAction,
        /// App name.
        name: String,
        /// Target build.
        build_id: Option<u32>,
        /// Game folder.
        install_dir: PathBuf,
    },
    /// A depot's manifest is ready.
    Depot {
        /// Depot.
        depot_id: u32,
        /// Entries in its manifest.
        files: usize,
        /// Size on disk.
        size: u64,
        /// Installed manifest this updates from, if it differs.
        update_from: Option<u64>,
    },
    /// The installed manifest could not be downloaded; files of the old
    /// version will not be removed.
    PreviousManifestUnavailable {
        /// Depot.
        depot_id: u32,
        /// Installed manifest.
        manifest_id: u64,
        /// Why.
        error: String,
    },
    /// Download/build progress.
    Progress(Progress),
}

/// Result of a run.
#[derive(Clone, Debug, PartialEq)]
pub enum InstallOutcome {
    /// Everything is already installed in the newest version.
    UpToDate {
        /// App name.
        name: String,
        /// Installed build.
        build_id: Option<u32>,
    },
    /// The run finished.
    Done(InstallSummary),
}

/// Summary of a finished run.
#[derive(Clone, Debug, PartialEq)]
pub struct InstallSummary {
    /// Install, update or verify.
    pub action: InstallAction,
    /// App name.
    pub name: String,
    /// What was done.
    pub report: InstallReport,
    /// Game folder.
    pub install_dir: PathBuf,
    /// The appmanifest that was written.
    pub app_manifest: PathBuf,
    /// Duration of the run.
    pub elapsed: Duration,
}

/// The platform whose depots to install. On Linux this is Windows when the
/// app has no Linux version (no Linux launch entry but a Windows one, or no
/// Linux depots but Windows ones), so it can run through Proton; the launch
/// uses the same rule (ADR 0017). Fetching another OS's depots is what
/// DepotDownloader's `-os` option does (`ContentDownloader.cs`, depot
/// filtering by `oslist`).
pub fn install_platform(app: &AppDetails, current: Platform, branch: &str) -> Platform {
    if current.os != "linux" {
        return current;
    }
    let windows = Platform {
        os: "windows".to_owned(),
        ..current.clone()
    };
    let no_linux_launch = app.launch_options_for(&current, branch).next().is_none()
        && app.launch_options_for(&windows, branch).next().is_some();
    let no_linux_depots =
        app.depots_for(&current).next().is_none() && app.depots_for(&windows).next().is_some();
    if no_linux_launch || no_linux_depots {
        windows
    } else {
        current
    }
}

/// Installs, updates or verifies an app. Dropping the future cancels the
/// run; the staging directory is kept, so the next run resumes.
pub async fn install_app(
    session: &Session,
    request: &InstallRequest,
    mut on_event: impl FnMut(InstallEvent),
) -> Result<InstallOutcome, CoreError> {
    let app_id = request.app_id;
    let branch = request.branch.as_str();
    let app = session.app_details(app_id, branch).await?;
    let name = app.name.clone().unwrap_or_else(|| format!("App {app_id}"));
    let install_dir_name = app
        .install_dir
        .clone()
        .ok_or(CoreError::NoInstallDir(app_id))?;
    // The folder name comes from Steam: treat it like any other untrusted name.
    let install_dir_relative = install_path(&install_dir_name)
        .ok()
        .filter(|path| path.components().count() == 1)
        .ok_or_else(|| CoreError::UnsafeInstallDir(install_dir_name.clone()))?;

    let platform = install_platform(&app, Platform::current(), branch);
    if platform.os == "windows" && !cfg!(windows) {
        on_event(InstallEvent::WindowsDepotsForProton);
    }
    let depots: Vec<_> = app.depots_for(&platform).collect();
    if depots.is_empty() {
        return Err(CoreError::NoDepots {
            name,
            os: platform.os,
            arch: platform.arch,
            branch: branch.to_owned(),
        });
    }

    let library = match &request.library {
        Some(path) => path.clone(),
        None => settings::default_library()?,
    };
    if is_steam_library(&library) {
        on_event(InstallEvent::SteamLibrary(library.clone()));
    }
    let steamapps = library.join("steamapps");
    let target = InstallTarget {
        install_dir: steamapps.join("common").join(&install_dir_relative),
        staging_dir: steamapps.join("downloading").join(app_id.to_string()),
    };

    // An existing installation tells us which manifests are installed now.
    let app_manifest = app_manifest_path(&library, app_id);
    let existing = read_app_manifest(&app_manifest)?;
    let installed_manifest = |depot_id: u32| {
        existing.as_ref().and_then(|(_, manifest)| {
            manifest
                .installed_depots
                .iter()
                .find(|depot| depot.depot_id == depot_id)
                .map(|depot| depot.manifest_id)
        })
    };
    let up_to_date = target.install_dir.is_dir()
        && depots.iter().all(|depot| {
            depot.manifest_id.is_some() && installed_manifest(depot.depot_id) == depot.manifest_id
        });
    if up_to_date && !request.verify {
        return Ok(InstallOutcome::UpToDate {
            name,
            build_id: app.build_id,
        });
    }

    let action = match (request.verify, existing.is_some()) {
        (true, _) => InstallAction::Verify,
        (false, true) => InstallAction::Update,
        (false, false) => InstallAction::Install,
    };
    on_event(InstallEvent::Started {
        action,
        name: name.clone(),
        build_id: app.build_id,
        install_dir: target.install_dir.clone(),
    });
    let servers = cdn_hosts(session, app_id).await?;
    let cdn = CdnClient::new().map_err(|error| CoreError::Http(error.to_string()))?;

    let mut downloads = Vec::new();
    let mut installed_depots = Vec::new();
    for depot in depots {
        let Some(manifest_id) = depot.manifest_id else {
            continue;
        };
        let fetched = fetch_depot(
            session,
            &cdn,
            &servers,
            app_id,
            depot.depot_id,
            manifest_id,
            branch,
        )
        .await?;

        // The installed manifest, to reuse its chunks and remove its leftovers.
        let installed = installed_manifest(depot.depot_id);
        let previous = match installed {
            Some(previous_id) if previous_id == manifest_id => Some(fetched.manifest.clone()),
            Some(previous_id) => {
                match fetch_depot(
                    session,
                    &cdn,
                    &servers,
                    app_id,
                    depot.depot_id,
                    previous_id,
                    branch,
                )
                .await
                {
                    Ok(previous) => Some(previous.manifest),
                    Err(error) => {
                        on_event(InstallEvent::PreviousManifestUnavailable {
                            depot_id: depot.depot_id,
                            manifest_id: previous_id,
                            error: error.to_string(),
                        });
                        None
                    }
                }
            }
            None => None,
        };
        on_event(InstallEvent::Depot {
            depot_id: depot.depot_id,
            files: fetched.manifest.files.len(),
            size: fetched.manifest.content_size(),
            update_from: installed.filter(|&previous_id| previous_id != manifest_id),
        });
        installed_depots.push(InstalledDepot {
            depot_id: depot.depot_id,
            manifest_id,
            size: fetched.manifest.content_size(),
        });
        downloads.push(DepotDownload {
            depot_id: depot.depot_id,
            key: fetched.key,
            manifest: fetched.manifest,
            previous,
        });
    }

    let source = CdnSource {
        cdn: &cdn,
        servers: &servers,
    };
    let started = Instant::now();
    let options = InstallOptions {
        verify_existing: request.verify,
        ..InstallOptions::default()
    };
    let report =
        reborn_content::install::install(&downloads, &target, &source, options, |progress| {
            on_event(InstallEvent::Progress(progress));
        })
        .await?;

    let manifest = NewAppManifest {
        app_id,
        name: name.clone(),
        install_dir: install_dir_name,
        build_id: app.build_id.map(u64::from).unwrap_or(0),
        last_owner: session.steam_id(),
        size_on_disk: installed_depots.iter().map(|depot| depot.size).sum(),
        download_size: report.bytes_downloaded,
        last_updated: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|time| time.as_secs())
            .unwrap_or(0),
        installed_depots,
    };
    // Update an existing appmanifest in place (keeps Steam's own fields),
    // or write a new one; via a temporary file so it is never half-written.
    let text = match existing {
        Some((mut document, _)) => {
            manifest.apply_to(&mut document);
            vdf::write(&document)
        }
        None => manifest.to_text(),
    };
    let temporary = steamapps.join(format!("appmanifest_{app_id}.acf.tmp"));
    std::fs::write(&temporary, text).map_err(io_error(&temporary))?;
    std::fs::rename(&temporary, &app_manifest).map_err(io_error(&app_manifest))?;

    Ok(InstallOutcome::Done(InstallSummary {
        action,
        name,
        report,
        install_dir: target.install_dir,
        app_manifest,
        elapsed: started.elapsed(),
    }))
}

/// Content servers that may serve the app, best first.
pub async fn cdn_hosts(session: &Session, app_id: u32) -> Result<Vec<CdnHost>, CoreError> {
    let servers: Vec<CdnHost> = session
        .content_servers()
        .await?
        .into_iter()
        .filter(|server| server.serves_app(app_id))
        .map(|server| CdnHost {
            host: server.host,
            https: server.https,
        })
        .collect();
    if servers.is_empty() {
        return Err(CoreError::NoServers);
    }
    Ok(servers)
}

/// A depot's key and its manifest with decrypted names.
#[derive(Debug)]
pub struct FetchedDepot {
    /// The depot key.
    pub key: DepotKey,
    /// The manifest, names decrypted.
    pub manifest: DepotManifest,
    /// The server that delivered the manifest.
    pub server: CdnHost,
    /// Whether the names were encrypted on the CDN.
    pub was_encrypted: bool,
}

/// Fetches the depot key and a manifest (trying up to five servers, each
/// with a fresh request code) and decrypts the file names.
pub async fn fetch_depot(
    session: &Session,
    cdn: &CdnClient,
    servers: &[CdnHost],
    app_id: u32,
    depot_id: u32,
    manifest_id: u64,
    branch: &str,
) -> Result<FetchedDepot, CoreError> {
    let key = session.depot_key(app_id, depot_id).await?;
    let key = DepotKey::from_slice(key.as_bytes()).ok_or(CoreError::BadDepotKey)?;

    let mut downloaded = None;
    for server in servers.iter().take(5) {
        // Request codes are short-lived, so fetch a fresh one per attempt.
        let code = session
            .manifest_request_code(app_id, depot_id, manifest_id, branch)
            .await?;
        match cdn.manifest(server, depot_id, manifest_id, code).await {
            Ok(bytes) => {
                downloaded = Some((server.clone(), bytes));
                break;
            }
            Err(error) => warn!(%error, "manifest download failed, trying the next server"),
        }
    }
    let (server, raw) = downloaded.ok_or(CoreError::ManifestUnavailable {
        depot_id,
        manifest_id,
    })?;
    info!(host = %server.host, https = server.https, bytes = raw.len(), "manifest downloaded");

    let manifest_error = |source| CoreError::Manifest { depot_id, source };
    let mut manifest = DepotManifest::parse(&raw).map_err(manifest_error)?;
    let was_encrypted = manifest.filenames_encrypted;
    manifest.decrypt_filenames(&key).map_err(manifest_error)?;
    Ok(FetchedDepot {
        key,
        manifest,
        server,
        was_encrypted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reborn_cm::{DepotInfo, LaunchOption, PUBLIC_BRANCH};

    fn depot(depot_id: u32, os: &str) -> DepotInfo {
        DepotInfo {
            depot_id,
            os_list: vec![os.to_owned()],
            manifest_id: Some(1),
            ..DepotInfo::default()
        }
    }

    fn launch(os: &str) -> LaunchOption {
        LaunchOption {
            executable: "game".to_owned(),
            os_list: vec![os.to_owned()],
            ..LaunchOption::default()
        }
    }

    fn linux() -> Platform {
        Platform {
            os: "linux".to_owned(),
            arch: "64".to_owned(),
            language: "english".to_owned(),
        }
    }

    #[test]
    fn windows_only_apps_get_windows_depots_on_linux() {
        let windows_only = AppDetails {
            depots: vec![depot(1, "windows")],
            launch: vec![launch("windows")],
            ..AppDetails::default()
        };
        assert_eq!(
            install_platform(&windows_only, linux(), PUBLIC_BRANCH).os,
            "windows"
        );

        // Shared content for all systems, but only a Windows launch entry.
        let mut shared = windows_only.clone();
        shared.depots.push(DepotInfo {
            depot_id: 2,
            manifest_id: Some(1),
            ..DepotInfo::default()
        });
        assert_eq!(
            install_platform(&shared, linux(), PUBLIC_BRANCH).os,
            "windows"
        );

        let native = AppDetails {
            depots: vec![depot(1, "windows"), depot(2, "linux")],
            launch: vec![launch("windows"), launch("linux")],
            ..AppDetails::default()
        };
        assert_eq!(
            install_platform(&native, linux(), PUBLIC_BRANCH).os,
            "linux"
        );

        let windows = Platform {
            os: "windows".to_owned(),
            ..linux()
        };
        assert_eq!(
            install_platform(&windows_only, windows, PUBLIC_BRANCH).os,
            "windows"
        );
    }
}
