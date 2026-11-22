//! Shim mode for one game launch (ADR 0048).
//!
//! A game loads `steamclient64.dll` from the path in
//! `HKCU\Software\Valve\Steam\ActiveProcess\SteamClientDll64`
//! (`docs/shim/loading.md`). In shim mode `rebornd`, right before it starts
//! a game, saves the values `SteamClientDll`, `SteamClientDll64` and `pid`
//! of that key, writes the shim's path and its own process ID (experiment 6,
//! `docs/shim/loading.md`), and puts the saved values back when the game
//! has exited, or removes the ones it created. It refuses while the
//! official client runs, because then the values are that client's live
//! state (ADR 0038). A leftover from an interrupted run is restored first.
//!
//! The registry access is behind [`ValuesStore`], so the logic is tested
//! without touching the registry.

use std::io;
use std::path::{Path, PathBuf};

/// The three values shim mode changes. `None` means the value does not
/// exist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveProcess {
    /// `SteamClientDll` (`REG_SZ`).
    pub client_dll: Option<String>,
    /// `SteamClientDll64` (`REG_SZ`).
    pub client_dll64: Option<String>,
    /// `pid` (`REG_DWORD`).
    pub pid: Option<u32>,
}

/// Reads and writes the three values.
pub trait ValuesStore {
    /// The values now.
    fn read(&self) -> io::Result<ActiveProcess>;
    /// Writes the values; a `None` removes that value.
    fn write(&self, values: &ActiveProcess) -> io::Result<()>;
}

/// Why shim mode could not be entered or left. Nothing was changed when it
/// refused.
#[derive(Debug, thiserror::Error)]
pub enum ShimModeError {
    /// The official client is running; its values are live state.
    #[error("the official client is running; close it to use shim mode")]
    OfficialClientRunning,
    /// The shim DLL does not exist.
    #[error("the shim {} does not exist", .0.display())]
    NoShim(PathBuf),
    /// Reading or writing failed.
    #[error("{what}: {source}")]
    Io {
        /// What was being done.
        what: &'static str,
        /// The error.
        source: io::Error,
    },
}

fn io_error(what: &'static str) -> impl FnOnce(io::Error) -> ShimModeError {
    move |source| ShimModeError::Io { what, source }
}

/// The file in Reborn's data folder that keeps the values from before
/// shim mode and the ones shim mode wrote.
const SAVED: &str = "shim-mode-saved.txt";

/// Shim mode's files in one folder of Reborn's.
#[derive(Clone, Debug)]
pub struct ShimMode {
    dir: PathBuf,
}

impl ShimMode {
    /// Shim mode keeping its saved values in `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn saved_path(&self) -> PathBuf {
        self.dir.join(SAVED)
    }

    /// Saves the current values and writes the shim's: `SteamClientDll64`
    /// = `shim`, `SteamClientDll` = `steamclient.dll` next to it, `pid` =
    /// `daemon_pid`. Restores a leftover from an interrupted run first.
    /// Refuses if `official_client_running` or the shim does not exist.
    pub fn enter(
        &self,
        store: &dyn ValuesStore,
        shim: &Path,
        daemon_pid: u32,
        official_client_running: bool,
    ) -> Result<(), ShimModeError> {
        if official_client_running {
            return Err(ShimModeError::OfficialClientRunning);
        }
        if !shim.is_file() {
            return Err(ShimModeError::NoShim(shim.to_path_buf()));
        }
        self.leave(store)?;
        let before = store.read().map_err(io_error("reading ActiveProcess"))?;
        let ours = ActiveProcess {
            client_dll: Some(
                shim.with_file_name("steamclient.dll")
                    .to_string_lossy()
                    .into_owned(),
            ),
            client_dll64: Some(shim.to_string_lossy().into_owned()),
            pid: Some(daemon_pid),
        };
        std::fs::create_dir_all(&self.dir).map_err(io_error("creating Reborn's folder"))?;
        let temporary = self.dir.join(format!("{SAVED}.part"));
        std::fs::write(&temporary, encode(&before, &ours))
            .map_err(io_error("saving the values"))?;
        std::fs::rename(&temporary, self.saved_path()).map_err(io_error("saving the values"))?;
        if let Err(error) = store.write(&ours) {
            // Put back whatever was partly written, then forget.
            let _ = store.write(&before);
            let _ = std::fs::remove_file(self.saved_path());
            return Err(io_error("writing ActiveProcess")(error));
        }
        Ok(())
    }

    /// Puts the saved values back if the key still holds what
    /// [`Self::enter`] wrote; otherwise (someone changed it since) leaves
    /// the key alone. Then forgets the saved values. Returns whether it
    /// restored. Safe to call any number of times, also at daemon start.
    pub fn leave(&self, store: &dyn ValuesStore) -> Result<bool, ShimModeError> {
        let text = match std::fs::read_to_string(self.saved_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io_error("reading the saved values")(error)),
        };
        let Some((before, ours)) = decode(&text) else {
            // A broken file must not block shim mode forever; nothing in
            // the registry is touched.
            std::fs::remove_file(self.saved_path())
                .map_err(io_error("removing the saved values"))?;
            return Ok(false);
        };
        let now = store.read().map_err(io_error("reading ActiveProcess"))?;
        let restored = if now == ours {
            store
                .write(&before)
                .map_err(io_error("restoring ActiveProcess"))?;
            true
        } else {
            false
        };
        std::fs::remove_file(self.saved_path()).map_err(io_error("removing the saved values"))?;
        Ok(restored)
    }
}

/// Six lines: before (dll, dll64, pid), then ours; an empty line is
/// `None`. Paths never contain line breaks.
fn encode(before: &ActiveProcess, ours: &ActiveProcess) -> String {
    let mut text = String::new();
    for values in [before, ours] {
        text.push_str(values.client_dll.as_deref().unwrap_or(""));
        text.push('\n');
        text.push_str(values.client_dll64.as_deref().unwrap_or(""));
        text.push('\n');
        text.push_str(&values.pid.map(|pid| pid.to_string()).unwrap_or_default());
        text.push('\n');
    }
    text
}

fn decode(text: &str) -> Option<(ActiveProcess, ActiveProcess)> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != 6 {
        return None;
    }
    let text_value = |line: &str| (!line.is_empty()).then(|| line.to_owned());
    let pid_value = |line: &str| -> Option<Option<u32>> {
        if line.is_empty() {
            Some(None)
        } else {
            line.parse().ok().map(Some)
        }
    };
    let values = |at: usize| -> Option<ActiveProcess> {
        Some(ActiveProcess {
            client_dll: text_value(lines[at]),
            client_dll64: text_value(lines[at + 1]),
            pid: pid_value(lines[at + 2])?,
        })
    };
    Some((values(0)?, values(3)?))
}

#[cfg(windows)]
pub use registry::{RegistryValues, official_client_running};

#[cfg(windows)]
#[allow(unsafe_code)]
mod registry {
    //! The real key, through `RegGetValueW`, `RegSetKeyValueW` and
    //! `RegDeleteKeyValueW` as Microsoft documents them, and the process
    //! list through the Tool Help snapshot functions.

    use std::io;

    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegDeleteKeyValueW,
        RegGetValueW, RegSetKeyValueW,
    };

    use super::{ActiveProcess, ValuesStore};

    const KEY: &str = r"Software\Valve\Steam\ActiveProcess";

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// `HKCU\Software\Valve\Steam\ActiveProcess`.
    #[derive(Clone, Debug)]
    pub struct RegistryValues {
        key: String,
    }

    impl RegistryValues {
        /// The key games read (`docs/shim/loading.md`).
        pub fn active_process() -> Self {
            Self {
                key: KEY.to_owned(),
            }
        }

        #[cfg(test)]
        pub(super) fn at(key: &str) -> Self {
            Self {
                key: key.to_owned(),
            }
        }

        fn get_text(&self, name: &str) -> io::Result<Option<String>> {
            let key = wide(&self.key);
            let value = wide(name);
            let mut buffer = vec![0u16; 1024];
            let mut size = u32::try_from(buffer.len() * 2).unwrap_or(u32::MAX);
            // SAFETY: `key` and `value` are NUL-terminated UTF-16 strings
            // that live for the call; `buffer` is writable for `size` bytes
            // and RegGetValueW writes at most that many and updates `size`.
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_SZ,
                    std::ptr::null_mut(),
                    buffer.as_mut_ptr().cast(),
                    &mut size,
                )
            };
            match status {
                ERROR_SUCCESS => {
                    buffer.truncate((size as usize) / 2);
                    while buffer.last() == Some(&0) {
                        buffer.pop();
                    }
                    String::from_utf16(&buffer)
                        .map(Some)
                        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "not UTF-16"))
                }
                ERROR_FILE_NOT_FOUND => Ok(None),
                _ => Err(io::Error::from_raw_os_error(status as i32)),
            }
        }

        fn get_dword(&self, name: &str) -> io::Result<Option<u32>> {
            let key = wide(&self.key);
            let value = wide(name);
            let mut data: u32 = 0;
            let mut size = 4u32;
            // SAFETY: as above; `data` is writable for the 4 bytes `size`
            // says, and RRF_RT_REG_DWORD accepts only a REG_DWORD.
            let status = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    RRF_RT_REG_DWORD,
                    std::ptr::null_mut(),
                    (&raw mut data).cast(),
                    &mut size,
                )
            };
            match status {
                ERROR_SUCCESS => Ok(Some(data)),
                ERROR_FILE_NOT_FOUND => Ok(None),
                _ => Err(io::Error::from_raw_os_error(status as i32)),
            }
        }

        fn set_text(&self, name: &str, text: &str) -> io::Result<()> {
            let key = wide(&self.key);
            let value = wide(name);
            let data = wide(text);
            let bytes = u32::try_from(data.len() * 2)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "too long"))?;
            // SAFETY: NUL-terminated UTF-16 strings that live for the call;
            // `bytes` is the size of `data` including its NUL, as REG_SZ
            // requires.
            let status = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_SZ,
                    data.as_ptr().cast(),
                    bytes,
                )
            };
            check(status)
        }

        fn set_dword(&self, name: &str, data: u32) -> io::Result<()> {
            let key = wide(&self.key);
            let value = wide(name);
            // SAFETY: as above; `data` is 4 readable bytes, as REG_DWORD
            // needs.
            let status = unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    value.as_ptr(),
                    REG_DWORD,
                    (&raw const data).cast(),
                    4,
                )
            };
            check(status)
        }

        fn delete(&self, name: &str) -> io::Result<()> {
            let key = wide(&self.key);
            let value = wide(name);
            // SAFETY: NUL-terminated UTF-16 strings that live for the call.
            let status =
                unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(());
            }
            check(status)
        }
    }

    fn check(status: u32) -> io::Result<()> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }

    impl ValuesStore for RegistryValues {
        fn read(&self) -> io::Result<ActiveProcess> {
            Ok(ActiveProcess {
                client_dll: self.get_text("SteamClientDll")?,
                client_dll64: self.get_text("SteamClientDll64")?,
                pid: self.get_dword("pid")?,
            })
        }

        fn write(&self, values: &ActiveProcess) -> io::Result<()> {
            match &values.client_dll {
                Some(text) => self.set_text("SteamClientDll", text)?,
                None => self.delete("SteamClientDll")?,
            }
            match &values.client_dll64 {
                Some(text) => self.set_text("SteamClientDll64", text)?,
                None => self.delete("SteamClientDll64")?,
            }
            match values.pid {
                Some(pid) => self.set_dword("pid", pid),
                None => self.delete("pid"),
            }
        }
    }

    /// Whether a process named `steam.exe` runs for any user (the official
    /// client's executable name on Windows).
    pub fn official_client_running() -> bool {
        // SAFETY: documented call; the handle is checked and closed below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            // Unknown: treat it as running, so shim mode refuses.
            return true;
        }
        // SAFETY: an all-zero PROCESSENTRY32W is valid; dwSize is set as
        // Process32FirstW requires.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        // SAFETY: `snapshot` is a valid snapshot handle and `entry` a
        // correctly sized PROCESSENTRY32W.
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        while more {
            let len = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("steam.exe") {
                found = true;
                break;
            }
            // SAFETY: as above.
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        // SAFETY: `snapshot` is a valid handle we own.
        unsafe { CloseHandle(snapshot) };
        found
    }

    #[cfg(test)]
    mod tests {
        use windows_sys::Win32::System::Registry::RegDeleteKeyW;

        use super::*;

        /// Writes, reads and removes values in a throwaway key of the
        /// current user, never Valve's, and deletes the key afterwards.
        #[test]
        fn writes_reads_and_removes_values() -> io::Result<()> {
            let key = format!(r"Software\RebornShimModeTest{}", std::process::id());
            let store = RegistryValues::at(&key);
            let result = (|| {
                assert_eq!(store.read()?, ActiveProcess::default());
                let values = ActiveProcess {
                    client_dll: Some(r"C:\a\steamclient.dll".to_owned()),
                    client_dll64: Some(r"C:\a\steamclient64.dll".to_owned()),
                    pid: Some(4242),
                };
                store.write(&values)?;
                assert_eq!(store.read()?, values);
                store.write(&ActiveProcess::default())?;
                assert_eq!(store.read()?, ActiveProcess::default());
                Ok(())
            })();
            let wide_key = wide(&key);
            // SAFETY: NUL-terminated UTF-16 string that lives for the call;
            // the key has no subkeys.
            unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide_key.as_ptr()) };
            result
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct FakeStore(RefCell<ActiveProcess>);

    impl ValuesStore for FakeStore {
        fn read(&self) -> io::Result<ActiveProcess> {
            Ok(self.0.borrow().clone())
        }
        fn write(&self, values: &ActiveProcess) -> io::Result<()> {
            *self.0.borrow_mut() = values.clone();
            Ok(())
        }
    }

    fn official() -> ActiveProcess {
        ActiveProcess {
            client_dll: Some(r"C:\Program Files (x86)\Steam\steamclient.dll".to_owned()),
            client_dll64: Some(r"C:\Program Files (x86)\Steam\steamclient64.dll".to_owned()),
            pid: Some(40300),
        }
    }

    struct Temp(PathBuf);
    impl Temp {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "reborn-shim-mode-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|t| t.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).ok();
            Self(dir)
        }
        fn shim(&self) -> PathBuf {
            let path = self.0.join("steamclient64.dll");
            std::fs::write(&path, b"shim").ok();
            path
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn enter_writes_the_shim_and_leave_restores() -> Result<(), ShimModeError> {
        let temp = Temp::new("cycle");
        let mode = ShimMode::new(temp.0.join("state"));
        let store = FakeStore(RefCell::new(official()));
        let shim = temp.shim();
        mode.enter(&store, &shim, 777, false)?;
        let now = store.read().map_err(io_error("test"))?;
        assert_eq!(
            now.client_dll64.as_deref(),
            Some(shim.to_string_lossy().as_ref())
        );
        assert_eq!(now.pid, Some(777));
        assert!(mode.leave(&store)?);
        assert_eq!(store.read().map_err(io_error("test"))?, official());
        assert!(!mode.leave(&store)?, "nothing left");
        Ok(())
    }

    #[test]
    fn values_that_did_not_exist_are_removed_again() -> Result<(), ShimModeError> {
        let temp = Temp::new("none");
        let mode = ShimMode::new(temp.0.join("state"));
        let store = FakeStore(RefCell::new(ActiveProcess::default()));
        mode.enter(&store, &temp.shim(), 777, false)?;
        assert!(mode.leave(&store)?);
        assert_eq!(
            store.read().map_err(io_error("test"))?,
            ActiveProcess::default()
        );
        Ok(())
    }

    #[test]
    fn refuses_while_the_official_client_runs_or_without_a_shim() {
        let temp = Temp::new("refuse");
        let mode = ShimMode::new(temp.0.join("state"));
        let store = FakeStore(RefCell::new(official()));
        assert!(matches!(
            mode.enter(&store, &temp.shim(), 777, true),
            Err(ShimModeError::OfficialClientRunning)
        ));
        assert!(matches!(
            mode.enter(&store, &temp.0.join("missing.dll"), 777, false),
            Err(ShimModeError::NoShim(_))
        ));
        assert_eq!(store.0.borrow().clone(), official(), "nothing changed");
    }

    #[test]
    fn a_changed_key_is_left_alone() -> Result<(), ShimModeError> {
        let temp = Temp::new("changed");
        let mode = ShimMode::new(temp.0.join("state"));
        let store = FakeStore(RefCell::new(official()));
        mode.enter(&store, &temp.shim(), 777, false)?;
        store.0.borrow_mut().pid = Some(1);
        assert!(!mode.leave(&store)?);
        assert_eq!(store.0.borrow().pid, Some(1));
        Ok(())
    }

    #[test]
    fn a_leftover_is_restored_before_entering_again() -> Result<(), ShimModeError> {
        let temp = Temp::new("leftover");
        let mode = ShimMode::new(temp.0.join("state"));
        let store = FakeStore(RefCell::new(official()));
        let shim = temp.shim();
        mode.enter(&store, &shim, 777, false)?;
        // An interrupted run: enter again without leaving.
        mode.enter(&store, &shim, 778, false)?;
        assert!(mode.leave(&store)?);
        assert_eq!(store.read().map_err(io_error("test"))?, official());
        Ok(())
    }

    #[test]
    fn saved_values_round_trip() {
        let before = official();
        let ours = ActiveProcess {
            client_dll: None,
            client_dll64: Some("x".to_owned()),
            pid: None,
        };
        assert_eq!(decode(&encode(&before, &ours)), Some((before, ours)));
        assert_eq!(decode("garbage"), None);
    }
}
