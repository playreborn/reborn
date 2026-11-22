//! Reborn's `steamclient` replacement (the shim), first skeleton.
//!
//! A game in shim mode (ADR 0048) loads this DLL instead of the official
//! client's. It exports the five entry points games ask for
//! (`docs/shim/entry-points.md`), gets its session from `rebornd` over the
//! shim socket (ADR 0047), and answers **only** calls whose meaning
//! `docs/shim/meanings.md` lists as confirmed or as matched in a planned
//! experiment (ADR 0042, ADR 0046). Every other call is logged and the
//! process ends with a clear message: no guessed answers. With what is
//! confirmed today no game runs on the shim yet; the skeleton is there so
//! each newly confirmed meaning has a place and a test.
//!
//! Provenance of what is implemented:
//! - `CreateInterface` returning an object for a `SteamClient…` text:
//!   `docs/shim/objects.md` (stage 3).
//! - `SteamClient020`, `021`, `023`: positions 0, 2, 9, 12 and the
//!   per-interface positions (`meanings.md`, experiment 7), and the calls
//!   at close: 4, 1 and 23 (020, 021) or 22 (023) (experiments 7, 11).
//! - `SteamUtils010` position 9, `GetAppID` (`meanings.md`, experiments 3
//!   to 5, confirmed), and position 14, `GetCurrentBatteryPower`
//!   (experiment 10, confirmed).
#![allow(unsafe_code)]

mod objects;
mod power;
mod session;

use std::ffi::{CStr, c_char, c_void};
use std::io::Write as _;
use std::sync::{Mutex, OnceLock};

pub use objects::{interface_for, known_interfaces};

/// The log file, opened on first use; `None` if it cannot be opened.
static LOG: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

/// Writes one line to `shim-<pid>.log` in Reborn's data folder.
pub(crate) fn log(line: &str) {
    let file = LOG.get_or_init(|| {
        let dir = log_dir()?;
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(format!("shim-{}.log", std::process::id()));
        std::fs::File::create(path).ok().map(Mutex::new)
    });
    if let Some(file) = file {
        let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = writeln!(file, "{line}");
    }
}

/// `%LOCALAPPDATA%\Reborn\data\shim` on Windows, the temp folder elsewhere
/// until the Linux shim is decided (ADR 0048).
fn log_dir() -> Option<std::path::PathBuf> {
    if cfg!(windows) {
        let base = std::env::var_os("LOCALAPPDATA")?;
        Some(
            std::path::PathBuf::from(base)
                .join("Reborn")
                .join("data")
                .join("shim"),
        )
    } else {
        Some(std::env::temp_dir().join("reborn-shim"))
    }
}

/// Logs a call the shim has no confirmed meaning for and ends the process:
/// a game must never run on a guessed answer.
pub(crate) fn not_implemented(what: &str) -> ! {
    log(&format!(
        "not implemented: {what}; stopping (no confirmed meaning)"
    ));
    std::process::abort();
}

/// Reads a NUL-terminated text a game passed, `None` for a null pointer or
/// text that is not UTF-8.
///
/// # Safety
///
/// `text` must be null or point to a NUL-terminated string that stays
/// valid for the call, as the x64 calling convention gives it to us.
pub(crate) unsafe fn text_arg<'a>(text: *const c_char) -> Option<&'a str> {
    if text.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a NUL-terminated string (see above).
    unsafe { CStr::from_ptr(text) }.to_str().ok()
}

/// `CreateInterface`: the game asks with a version text
/// (`docs/shim/objects.md`). Returns the shim's object for a known text
/// once a session exists, otherwise nothing.
///
/// # Safety
///
/// Called by games with a NUL-terminated text in `rcx`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn CreateInterface(version: *const c_char) -> *mut c_void {
    let result = std::panic::catch_unwind(|| {
        // SAFETY: see the function's safety section.
        let Some(version) = (unsafe { text_arg(version) }) else {
            log("CreateInterface without a readable text");
            return std::ptr::null_mut();
        };
        if session::get().is_none() {
            log(&format!(
                "CreateInterface({version}): no session, returning nothing"
            ));
            return std::ptr::null_mut();
        }
        match interface_for(version) {
            Some(object) => {
                log(&format!("CreateInterface({version}): answered"));
                object
            }
            None => {
                log(&format!(
                    "CreateInterface({version}): not known, returning nothing"
                ));
                std::ptr::null_mut()
            }
        }
    });
    result.unwrap_or_else(|_| std::process::abort())
}

/// `Steam_BGetCallback`: meaning not confirmed yet (`entry-points.md`).
#[unsafe(no_mangle)]
pub extern "C" fn Steam_BGetCallback() -> u8 {
    not_implemented("Steam_BGetCallback")
}

/// `Steam_FreeLastCallback`: meaning not confirmed yet.
#[unsafe(no_mangle)]
pub extern "C" fn Steam_FreeLastCallback() -> u8 {
    not_implemented("Steam_FreeLastCallback")
}

/// `Steam_GetAPICallResult`: meaning not confirmed yet.
#[unsafe(no_mangle)]
pub extern "C" fn Steam_GetAPICallResult() -> u8 {
    not_implemented("Steam_GetAPICallResult")
}

/// `Steam_ReleaseThreadLocalMemory`: meaning not confirmed yet.
#[unsafe(no_mangle)]
pub extern "C" fn Steam_ReleaseThreadLocalMemory() {
    not_implemented("Steam_ReleaseThreadLocalMemory")
}
