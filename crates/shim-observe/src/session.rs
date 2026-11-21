//! Arming and disarming an observation session (ADR 0039).
//!
//! The game loads `steamclient64.dll` from the path in
//! `HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll64`
//! (`docs/shim/loading.md`). Arming saves that value, copies the proxy to
//! Reborn's folder and points the value at it; disarming puts the saved
//! value back, but only if the proxy's path is still there.

use std::io;
use std::path::{Path, PathBuf};

/// Name of the file that holds the value from before the session.
pub(crate) const SAVED: &str = "original-steamclient64.txt";
/// Name of the proxy's copy in Reborn's folder.
const PROXY: &str = "steamclient64.dll";
/// Name of the file next to the proxy that turns its stand-in objects off
/// (ADR 0040, step 3b); the proxy then only reads, as in step 3a.
pub const READ_ONLY_MARKER: &str = "read-only";

/// The one registry value a session changes.
pub trait ValueStore {
    /// The current value, `None` if it does not exist.
    fn get(&self) -> io::Result<Option<String>>;
    /// Writes the value.
    fn set(&self, value: &str) -> io::Result<()>;
}

/// Why a command refused or failed. Nothing was changed when it refused.
#[derive(Debug, thiserror::Error)]
pub enum ObserveError {
    /// A previous session was not disarmed.
    #[error("a session is still armed; run `reborn-observe disarm` first")]
    AlreadyArmed,
    /// `SteamClientDll64` does not exist.
    #[error("SteamClientDll64 is not set; start the official client first")]
    NoValue,
    /// `SteamClientDll64` already points into Reborn's folder.
    #[error("SteamClientDll64 already points to Reborn ({0}); run `reborn-observe disarm`")]
    PointsToReborn(String),
    /// The proxy file to copy does not exist.
    #[error("the proxy {} does not exist", .0.display())]
    NoProxy(PathBuf),
    /// Reading or writing failed.
    #[error("{what}: {source}")]
    Io {
        /// What was being done.
        what: String,
        /// The error.
        source: io::Error,
    },
}

/// What `status` reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// The value now.
    pub current: Option<String>,
    /// The value saved by `arm`, if a session is armed.
    pub saved: Option<String>,
    /// Whether the value points to the proxy.
    pub points_to_proxy: bool,
}

/// What `disarm` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Disarmed {
    /// No session was armed.
    NothingArmed,
    /// The saved value was written back.
    Restored(String),
    /// The value no longer pointed to the proxy, so it was left as it was.
    LeftAlone {
        /// The value now.
        current: Option<String>,
        /// The value saved by `arm`.
        saved: String,
    },
}

/// A session's files, in one folder of Reborn's.
#[derive(Clone, Debug)]
pub struct Session {
    dir: PathBuf,
}

impl Session {
    /// A session keeping its files in `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Where the proxy is copied to, and what the value points to while
    /// armed.
    pub fn proxy_path(&self) -> PathBuf {
        self.dir.join(PROXY)
    }

    fn saved_path(&self) -> PathBuf {
        self.dir.join(SAVED)
    }

    /// The current state.
    pub fn status(&self, store: &dyn ValueStore) -> Result<Status, ObserveError> {
        let current = store.get().map_err(io_error("reading SteamClientDll64"))?;
        let points_to_proxy = current
            .as_deref()
            .is_some_and(|value| same_path(value, &self.proxy_path()));
        Ok(Status {
            current,
            saved: self.saved()?,
            points_to_proxy,
        })
    }

    /// Saves the current value, copies `proxy` to Reborn's folder and
    /// points the value at the copy. With `read_only`, the proxy hands out
    /// no stand-in objects. Refuses without changing anything if a session
    /// is armed, the value is missing, or it already points into Reborn's
    /// folder.
    pub fn arm(
        &self,
        store: &dyn ValueStore,
        proxy: &Path,
        read_only: bool,
    ) -> Result<PathBuf, ObserveError> {
        if self.saved_path().exists() {
            return Err(ObserveError::AlreadyArmed);
        }
        let current = store
            .get()
            .map_err(io_error("reading SteamClientDll64"))?
            .filter(|value| !value.is_empty())
            .ok_or(ObserveError::NoValue)?;
        if is_inside(&current, &self.dir) {
            return Err(ObserveError::PointsToReborn(current));
        }
        if !proxy.is_file() {
            return Err(ObserveError::NoProxy(proxy.to_path_buf()));
        }
        std::fs::create_dir_all(&self.dir).map_err(io_error("creating Reborn's folder"))?;
        let target = self.proxy_path();
        std::fs::copy(proxy, &target).map_err(io_error("copying the proxy"))?;
        let marker = self.dir.join(READ_ONLY_MARKER);
        if read_only {
            std::fs::write(&marker, b"").map_err(io_error("writing the read-only marker"))?;
        } else if marker.exists() {
            std::fs::remove_file(&marker).map_err(io_error("removing the read-only marker"))?;
        }
        // Saved before the value changes, so `disarm` can always restore.
        let temporary = self.dir.join(format!("{SAVED}.part"));
        std::fs::write(&temporary, &current).map_err(io_error("saving the old value"))?;
        std::fs::rename(&temporary, self.saved_path()).map_err(io_error("saving the old value"))?;
        let target_text = target.to_string_lossy().into_owned();
        if let Err(error) = store.set(&target_text) {
            // Nothing changed in the registry: forget the saved value again.
            let _ = std::fs::remove_file(self.saved_path());
            return Err(io_error("writing SteamClientDll64")(error));
        }
        Ok(target)
    }

    /// Writes the saved value back if the value still points to the proxy,
    /// then forgets the session. Safe to run any number of times.
    pub fn disarm(&self, store: &dyn ValueStore) -> Result<Disarmed, ObserveError> {
        let Some(saved) = self.saved()? else {
            return Ok(Disarmed::NothingArmed);
        };
        let current = store.get().map_err(io_error("reading SteamClientDll64"))?;
        let outcome = if current
            .as_deref()
            .is_some_and(|value| same_path(value, &self.proxy_path()))
        {
            store
                .set(&saved)
                .map_err(io_error("writing SteamClientDll64"))?;
            Disarmed::Restored(saved)
        } else {
            Disarmed::LeftAlone { current, saved }
        };
        std::fs::remove_file(self.saved_path()).map_err(io_error("removing the saved value"))?;
        // The copy may still be loaded by a running game; it is harmless.
        let _ = std::fs::remove_file(self.proxy_path());
        let _ = std::fs::remove_file(self.dir.join(READ_ONLY_MARKER));
        Ok(outcome)
    }

    fn saved(&self) -> Result<Option<String>, ObserveError> {
        match std::fs::read_to_string(self.saved_path()) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error("reading the saved value")(error)),
        }
    }
}

fn io_error(what: &str) -> impl FnOnce(io::Error) -> ObserveError + '_ {
    move |source| ObserveError::Io {
        what: what.to_owned(),
        source,
    }
}

/// Windows paths compare without regard to case or slash direction.
fn normalize(path: &str) -> String {
    path.replace('/', "\\").to_lowercase()
}

fn same_path(value: &str, path: &Path) -> bool {
    normalize(value) == normalize(&path.to_string_lossy())
}

fn is_inside(value: &str, dir: &Path) -> bool {
    let dir = normalize(&dir.to_string_lossy());
    let dir = dir.trim_end_matches('\\');
    normalize(value).starts_with(&format!("{dir}\\"))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    const OFFICIAL: &str = r"C:\Program Files (x86)\Steam\steamclient64.dll";

    #[derive(Default)]
    struct FakeStore {
        value: RefCell<Option<String>>,
        fail_set: bool,
    }

    impl FakeStore {
        fn with(value: &str) -> Self {
            Self {
                value: RefCell::new(Some(value.to_owned())),
                fail_set: false,
            }
        }
    }

    impl ValueStore for FakeStore {
        fn get(&self) -> io::Result<Option<String>> {
            Ok(self.value.borrow().clone())
        }

        fn set(&self, value: &str) -> io::Result<()> {
            if self.fail_set {
                return Err(io::Error::other("denied"));
            }
            *self.value.borrow_mut() = Some(value.to_owned());
            Ok(())
        }
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "reborn-observe-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|time| time.as_nanos())
                    .unwrap_or(0)
            );
            let dir = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&dir).ok();
            Self(dir)
        }

        fn proxy(&self) -> PathBuf {
            let path = self.0.join("built-proxy.dll");
            std::fs::write(&path, b"proxy").ok();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn arm_then_disarm_restores_the_value() -> Result<(), ObserveError> {
        let temp = TempDir::new("cycle");
        let session = Session::new(temp.0.join("shim-observe"));
        let store = FakeStore::with(OFFICIAL);
        let target = session.arm(&store, &temp.proxy(), false)?;
        assert_eq!(target, session.proxy_path());
        assert!(target.is_file(), "proxy copied");
        let status = session.status(&store)?;
        assert!(status.points_to_proxy);
        assert_eq!(status.saved.as_deref(), Some(OFFICIAL));

        assert_eq!(
            session.disarm(&store)?,
            Disarmed::Restored(OFFICIAL.to_owned())
        );
        assert_eq!(store.get().ok().flatten().as_deref(), Some(OFFICIAL));
        assert_eq!(session.status(&store)?.saved, None);
        // Running it again does nothing.
        assert_eq!(session.disarm(&store)?, Disarmed::NothingArmed);
        Ok(())
    }

    #[test]
    fn arm_refuses_twice_and_changes_nothing() -> Result<(), ObserveError> {
        let temp = TempDir::new("twice");
        let session = Session::new(temp.0.join("shim-observe"));
        let store = FakeStore::with(OFFICIAL);
        session.arm(&store, &temp.proxy(), false)?;
        let before = store.get().ok().flatten();
        assert!(matches!(
            session.arm(&store, &temp.proxy(), false),
            Err(ObserveError::AlreadyArmed)
        ));
        assert_eq!(store.get().ok().flatten(), before);
        Ok(())
    }

    #[test]
    fn arm_refuses_without_a_value_or_proxy() {
        let temp = TempDir::new("refuse");
        let session = Session::new(temp.0.join("shim-observe"));
        let empty = FakeStore::default();
        assert!(matches!(
            session.arm(&empty, &temp.proxy(), false),
            Err(ObserveError::NoValue)
        ));
        let store = FakeStore::with(OFFICIAL);
        assert!(matches!(
            session.arm(&store, &temp.0.join("missing.dll"), false),
            Err(ObserveError::NoProxy(_))
        ));
        assert_eq!(store.get().ok().flatten().as_deref(), Some(OFFICIAL));
        assert!(!session.saved_path().exists());
    }

    #[test]
    fn arm_refuses_when_the_value_already_points_to_reborn() {
        let temp = TempDir::new("reborn");
        let session = Session::new(temp.0.join("shim-observe"));
        let inside = session.proxy_path().to_string_lossy().to_uppercase();
        let store = FakeStore::with(&inside);
        assert!(matches!(
            session.arm(&store, &temp.proxy(), false),
            Err(ObserveError::PointsToReborn(_))
        ));
    }

    #[test]
    fn a_failed_write_leaves_no_session() {
        let temp = TempDir::new("fail");
        let session = Session::new(temp.0.join("shim-observe"));
        let store = FakeStore {
            value: RefCell::new(Some(OFFICIAL.to_owned())),
            fail_set: true,
        };
        assert!(matches!(
            session.arm(&store, &temp.proxy(), false),
            Err(ObserveError::Io { .. })
        ));
        assert!(!session.saved_path().exists());
        assert_eq!(store.get().ok().flatten().as_deref(), Some(OFFICIAL));
    }

    #[test]
    fn disarm_leaves_a_changed_value_alone() -> Result<(), ObserveError> {
        let temp = TempDir::new("changed");
        let session = Session::new(temp.0.join("shim-observe"));
        let store = FakeStore::with(OFFICIAL);
        session.arm(&store, &temp.proxy(), false)?;
        // The official client wrote its value again in the meantime.
        let other = r"D:\Steam\steamclient64.dll";
        store.set(other).ok();
        assert_eq!(
            session.disarm(&store)?,
            Disarmed::LeftAlone {
                current: Some(other.to_owned()),
                saved: OFFICIAL.to_owned(),
            }
        );
        assert_eq!(store.get().ok().flatten().as_deref(), Some(other));
        assert_eq!(session.status(&store)?.saved, None);
        Ok(())
    }

    #[test]
    fn read_only_writes_a_marker_that_disarm_removes() -> Result<(), ObserveError> {
        let temp = TempDir::new("read-only");
        let dir = temp.0.join("shim-observe");
        let session = Session::new(&dir);
        let store = FakeStore::with(OFFICIAL);
        session.arm(&store, &temp.proxy(), true)?;
        assert!(dir.join(READ_ONLY_MARKER).exists());
        session.disarm(&store)?;
        assert!(!dir.join(READ_ONLY_MARKER).exists());
        // Arming without it removes a marker left behind.
        std::fs::write(dir.join(READ_ONLY_MARKER), b"").ok();
        session.arm(&store, &temp.proxy(), false)?;
        assert!(!dir.join(READ_ONLY_MARKER).exists());
        session.disarm(&store)?;
        Ok(())
    }

    #[test]
    fn paths_compare_like_windows() {
        let dir = Path::new(r"C:\Users\x\AppData\Local\Reborn\data\shim-observe");
        assert!(is_inside(
            r"c:/users/X/appdata/local/reborn/data/shim-observe/steamclient64.dll",
            dir
        ));
        assert!(!is_inside(
            r"C:\Users\x\AppData\Local\Reborn\data\shim-observe-other\a.dll",
            dir
        ));
        assert!(!is_inside(OFFICIAL, dir));
    }
}
