use std::path::PathBuf;

use reborn_content::install::InstallError;
use reborn_content::manifest::ManifestError;
use reborn_platform::secrets::SecretStoreError;
use reborn_platform::settings::SettingsError;

/// Errors of Reborn's domain logic.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CoreError {
    /// No account was given and none is set as default.
    #[error("no account given and no default account; sign in first")]
    NoAccount,
    /// The account has no stored login on this computer.
    #[error("no stored login for `{0}`; sign in first")]
    NotSignedIn(String),
    /// The stored refresh token was rejected (expired or revoked).
    #[error("signing in with the stored login failed, sign in again: {0}")]
    TokenRejected(#[source] reborn_cm::Error),
    /// Talking to Steam failed.
    #[error(transparent)]
    Steam(#[from] reborn_cm::Error),
    /// Signing in to Steam took too long (network trouble).
    #[error("signing in to Steam timed out; check the network connection")]
    SignInTimeout,
    /// The OS credential store failed.
    #[error(transparent)]
    Secrets(#[from] SecretStoreError),
    /// Reading or writing settings failed.
    #[error(transparent)]
    Settings(#[from] SettingsError),
    /// The app info has no install folder.
    #[error("app {0} has no install folder in its app info")]
    NoInstallDir(u32),
    /// The install folder from the app info is not a single safe name.
    #[error("unsafe install folder `{0}`")]
    UnsafeInstallDir(String),
    /// No depot fits this computer.
    #[error("{name} has no depots for {os} {arch}-bit on branch `{branch}`")]
    NoDepots {
        /// App name.
        name: String,
        /// Operating system.
        os: String,
        /// Architecture.
        arch: String,
        /// Branch.
        branch: String,
    },
    /// Steam returned no content servers that may serve the app.
    #[error("Steam returned no usable content servers")]
    NoServers,
    /// Steam sent a depot key that is not 32 bytes.
    #[error("Steam sent a depot key of unexpected length")]
    BadDepotKey,
    /// No content server delivered a manifest.
    #[error("no content server delivered manifest {manifest_id} of depot {depot_id}")]
    ManifestUnavailable {
        /// Depot.
        depot_id: u32,
        /// Manifest.
        manifest_id: u64,
    },
    /// A manifest could not be read.
    #[error("depot {depot_id}: {source}")]
    Manifest {
        /// Depot.
        depot_id: u32,
        /// The error.
        source: ManifestError,
    },
    /// Downloading or writing files failed.
    #[error(transparent)]
    Install(#[from] InstallError),
    /// An appmanifest could not be read.
    #[error("{path}: {source}")]
    AppManifest {
        /// The file.
        path: PathBuf,
        /// The error.
        source: reborn_formats::Error,
    },
    /// File system error.
    #[error("{path}: {source}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// The HTTP client could not be created.
    #[error("HTTP client: {0}")]
    Http(String),
    /// The app is not (fully) installed in the library.
    #[error("app {0} is not installed")]
    NotInstalled(u32),
    /// No launch entry fits this computer.
    #[error("{name} has no launch entry for {os}")]
    NoLaunchOption {
        /// App name.
        name: String,
        /// Operating system.
        os: String,
    },
    /// The game only has Windows launch entries (and this is not Linux, or
    /// the caller did not try Proton).
    #[error("{0} only runs on Windows")]
    NeedsProton(String),
    /// The game needs Proton, but no Proton build is installed.
    #[error(
        "{0} only runs on Windows and needs Proton; install a Proton build, e.g. in the Steam client under Library > Tools"
    )]
    NoProton(String),
    /// Building the Proton command failed.
    #[error(transparent)]
    Proton(#[from] reborn_platform::proton::ProtonError),
    /// The requested launch entry does not exist for this computer.
    #[error("launch entry {0} does not exist for this computer")]
    UnknownLaunchOption(u32),
    /// A path in the launch entry could escape the install folder.
    #[error("unsafe path `{0}` in the launch entry")]
    UnsafeLaunchPath(String),
    /// The program of the launch entry is not there.
    #[error("{} is missing; verify the game", .0.display())]
    MissingExecutable(PathBuf),
    /// Starting the program failed.
    #[error("cannot start {}: {source}", program.display())]
    Launch {
        /// The program.
        program: PathBuf,
        /// The error.
        source: std::io::Error,
    },
}

pub(crate) fn io_error(path: &std::path::Path) -> impl FnOnce(std::io::Error) -> CoreError + '_ {
    move |source| CoreError::Io {
        path: path.to_path_buf(),
        source,
    }
}
