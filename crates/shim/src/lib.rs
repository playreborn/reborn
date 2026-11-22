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
//! - `SteamUtils009` and `SteamUtils010` position 9, `GetAppID`
//!   (`meanings.md`, experiments 3 to 5 and 15, confirmed), and position
//!   14, `GetCurrentBatteryPower` (experiments 10 and 15, confirmed).
//! - `Steam_BGetCallback` and `Steam_FreeLastCallback`: handing over and
//!   freeing queued callbacks (`meanings.md`, experiment 19, confirmed
//!   for the header's number, address and size and the poll and free
//!   order; bytes 0..4 and 20..24 written as recorded, candidates; a
//!   free with nothing held returns 0, as recorded in Linsips). The queue (`callbacks`) has no source of callbacks yet: no callback
//!   content is confirmed for the shim, so nothing is delivered to a game
//!   today.
//! - `STEAMAPPS_INTERFACE_VERSION008` position 0, `BIsSubscribed`
//!   (`meanings.md`, experiment 8, confirmed; shadow mode matched in three
//!   games, and the maintainer allowed answer mode for this call, for the
//!   game itself only, 2026-11-22): answered with `rebornd`'s
//!   `AppOwnership` for the session's app (ADR 0055), lowest byte 1 or 0.
//!   `Unavailable` or a lost daemon (after one reconnect, `session`) is
//!   never turned into an answer: the process stops. Every other position
//!   of the object, position 7 (DLC) included, stops. The object is handed
//!   out by `CreateInterface` and by every `SteamClient` getter position,
//!   which hand out by text (recorded for this text: position 12 of 020,
//!   15 of 021; the `SteamClient023` game asked for `…009`, which has no
//!   object and stops).
//! - `Steam_GetAPICallResult` and `Steam_ReleaseThreadLocalMemory`: not
//!   answered (no confirmed meaning).
//!
//! The connection to `rebornd` (`session`, ADR 0047 and ADR 0055): a
//! reader thread per connection routes responses by `id` to the waiting
//! request and events (such as `StatsReceived`) to a queue no callback
//! reads yet. Every connect and exchange waits at most 10 s; a stalled
//! daemon, an answer for no open request or a broken line ends the
//! connection for the run, and the call that needed it stops the process with the reason
//! ("rebornd did not answer in time"). Only a lost connection is
//! reconnected, once per run, and only reads are sent again on it.
#![allow(unsafe_code)]

mod callbacks;
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

/// `Steam_BGetCallback`: hands the game the next callback queued for the
/// handle in `rcx` (`crate::callbacks`).
///
/// Provenance: `docs/shim/meanings.md`, "Callback delivery …" (loop
/// iteration 9b), result of experiment 19, and `docs/shim/entry-points.md`
/// (stage 2 part 3, steps 4b and 4d):
///
/// - `rcx` is a handle (1 or 2 in the recordings, compared on its low 32
///   bits), read here as `i32`; whether it is the pipe or the user handle
///   stays a candidate, the queue is keyed by it either way.
/// - `rdx` points to the 24 bytes the game gets: written as experiment 19
///   confirmed (see `crate::callbacks`).
/// - `r8` held another stack address near `rdx`, but the 16 bytes there
///   showed no shared pattern and whether the client writes them is not
///   known (step 4b). Its meaning is not established, so the shim does not
///   take or touch it.
/// - The result: 1 when a callback was handed over (confirmed, D). 0 when
///   nothing was handed over is the observed reading: in every recording
///   the lowest byte was only 0 or 1, with a free after every 1 (D); in
///   experiment 19's games never after a 0, in Linsips once after a 0
///   (`meanings.md`, loop iteration 9b, Linsips note). Only the lowest
///   byte is returned; the upper bits of `rax` held leftovers in the
///   recordings (`0xffffff00`).
///
/// A null `rdx`, or a poll while the last callback is not freed yet, was
/// never recorded and has no confirmed answer: logged, and the process
/// ends.
///
/// # Safety
///
/// `header` must be null or point to 24 writable bytes, as the recorded
/// calls pass a stack address there.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn Steam_BGetCallback(pipe: i32, header: *mut u8) -> u8 {
    let result = std::panic::catch_unwind(|| {
        if header.is_null() {
            not_implemented("Steam_BGetCallback with a null header pointer");
        }
        match callbacks::fetch(pipe) {
            callbacks::Fetch::Empty => 0,
            callbacks::Fetch::Delivered(bytes) => {
                // SAFETY: `header` is not null and points to 24 writable
                // bytes (the function's safety section); `bytes` is a
                // local array, so the ranges do not overlap. A byte copy
                // needs no alignment.
                unsafe {
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), header, callbacks::HEADER_SIZE)
                };
                1
            }
            callbacks::Fetch::StillHeld => {
                not_implemented("Steam_BGetCallback before the last callback was freed")
            }
        }
    });
    result.unwrap_or_else(|_| std::process::abort())
}

/// `Steam_FreeLastCallback`: releases the callback last handed over for
/// the handle in `rcx`; its data stays valid until then.
///
/// Provenance: `docs/shim/meanings.md`, experiment 19 (D, A2: exactly one
/// free per header, on the same `rcx`, before the next poll). The result:
/// `rax` was 0 in every recorded call (`docs/shim/entry-points.md`, stage
/// 2 part 3), so the whole register is 0 here. `rdx`, `r8` and `r9` held
/// leftovers (`r8` the payload size in Dark Deception, an address in
/// Bongo Cat; `meanings.md`, loop iteration 9b) and are not taken.
///
/// A free with nothing held is recorded: Linsips (`observe-55108`, 10
/// headers and 11 frees) freed once after a poll that returned 0, and
/// that free returned 0 too (`meanings.md`, loop iteration 9b, Linsips
/// note). So it changes nothing and returns 0.
#[unsafe(no_mangle)]
pub extern "C" fn Steam_FreeLastCallback(pipe: i32) -> u64 {
    let result = std::panic::catch_unwind(|| match callbacks::free_last(pipe) {
        callbacks::Free::Freed | callbacks::Free::NothingHeld => 0,
    });
    result.unwrap_or_else(|_| std::process::abort())
}

/// `Steam_GetAPICallResult`: not answered. Experiment 19 confirmed only
/// when games call it (inside a 703 callback, once per 703); what 703
/// holds and which function the export is stays open until F is checked
/// (`docs/shim/meanings.md`, loop iteration 9b).
#[unsafe(no_mangle)]
pub extern "C" fn Steam_GetAPICallResult() -> u8 {
    not_implemented("Steam_GetAPICallResult")
}

/// `Steam_ReleaseThreadLocalMemory`: meaning not confirmed yet (Dark
/// Deception calls it, `docs/shim/meanings.md`, loop iteration 9b; no
/// experiment has tested a meaning).
#[unsafe(no_mangle)]
pub extern "C" fn Steam_ReleaseThreadLocalMemory() {
    not_implemented("Steam_ReleaseThreadLocalMemory")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Handles not used by the tests of `callbacks`.

    #[test]
    fn poll_with_an_empty_queue_returns_0_and_writes_nothing() {
        let mut header = [0xeeu8; callbacks::HEADER_SIZE];
        // SAFETY: `header` is 24 writable bytes.
        let result = unsafe { Steam_BGetCallback(200, header.as_mut_ptr()) };
        assert_eq!(result, 0);
        assert_eq!(header, [0xee; callbacks::HEADER_SIZE]);
    }

    #[test]
    fn poll_writes_the_header_and_free_returns_0() {
        callbacks::enqueue(201, 1101, &[5; 24]).unwrap();
        let mut header = [0xeeu8; callbacks::HEADER_SIZE];
        // SAFETY: `header` is 24 writable bytes.
        let result = unsafe { Steam_BGetCallback(201, header.as_mut_ptr()) };
        assert_eq!(result, 1);
        assert_eq!(header[0..8], [1, 0, 0, 0, 0x4d, 0x04, 0, 0]);
        assert_eq!(header[16..24], [24, 0, 0, 0, 0, 0, 0, 0]);
        let mut address = [0u8; 8];
        address.copy_from_slice(&header[8..16]);
        let address = u64::from_le_bytes(address) as usize as *const u8;
        // SAFETY: the callback is held until the free below.
        let data = unsafe { std::slice::from_raw_parts(address, 24) };
        assert_eq!(data, &[5; 24]);
        assert_eq!(Steam_FreeLastCallback(201), 0);
        // SAFETY: as above.
        let result = unsafe { Steam_BGetCallback(201, header.as_mut_ptr()) };
        assert_eq!(result, 0, "drained");
    }

    #[test]
    fn free_with_nothing_held_returns_0_and_keeps_the_queue() {
        // As recorded in Linsips (`observe-55108`): a free after a poll
        // that returned 0.
        assert_eq!(Steam_FreeLastCallback(202), 0, "never used");
        callbacks::enqueue(202, 304, &[3; 16]).unwrap();
        assert_eq!(Steam_FreeLastCallback(202), 0, "queued, not held");
        let mut header = [0u8; callbacks::HEADER_SIZE];
        // SAFETY: `header` is 24 writable bytes.
        let result = unsafe { Steam_BGetCallback(202, header.as_mut_ptr()) };
        assert_eq!(result, 1, "the queued callback is still there");
        assert_eq!(header[4..8], [0x30, 0x01, 0, 0]);
        assert_eq!(Steam_FreeLastCallback(202), 0);
        assert_eq!(Steam_FreeLastCallback(202), 0, "a second free");
        // SAFETY: as above.
        let result = unsafe { Steam_BGetCallback(202, header.as_mut_ptr()) };
        assert_eq!(result, 0);
    }
}
