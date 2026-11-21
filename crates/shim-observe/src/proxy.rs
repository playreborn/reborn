//! The observation proxy (ADR 0038 stage 2, ADR 0039), part 2: it forwards
//! the five entry points a game asks `steamclient64.dll` for
//! (`docs/shim/entry-points.md`) to the real DLL and logs that they were
//! called. Arguments and results are not looked at yet.
//!
//! Nothing is known about the entry points' arguments, so each one is a
//! small assembly stub (Microsoft x64 calling convention, as documented by
//! Microsoft): it saves the four integer and four floating-point argument
//! registers, asks `observe_call` for the real function's address, puts
//! the registers back, restores the stack pointer and jumps there. The
//! real function then sees exactly the call the game made and returns to
//! the game directly.
//!
//! The real DLL is loaded on the first call, not while this DLL is being
//! loaded (Windows forbids loading libraries from `DllMain`). Its path is
//! the value `reborn-observe arm` saved next to this DLL.
#![allow(unsafe_code)]

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use windows_sys::Win32::Foundation::HMODULE;
use windows_sys::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW, GetProcAddress, LOAD_WITH_ALTERED_SEARCH_PATH,
    LoadLibraryExW,
};

use crate::session::SAVED;

/// The entry points, in the order of the assembly stubs below.
pub const ENTRY_POINTS: [&str; 5] = [
    "CreateInterface",
    "Steam_ReleaseThreadLocalMemory",
    "Steam_BGetCallback",
    "Steam_FreeLastCallback",
    "Steam_GetAPICallResult",
];

/// Calls of one entry point logged one by one; after that, every
/// `LOG_EVERY`th call (some are called once per frame).
const LOG_FIRST: u64 = 50;
const LOG_EVERY: u64 = 10_000;

/// Addresses of the real functions, in [`ENTRY_POINTS`] order.
static TARGETS: OnceLock<[usize; 5]> = OnceLock::new();
/// The log file, `None` if it cannot be opened (or in tests).
static LOG: OnceLock<Option<Mutex<File>>> = OnceLock::new();
static CALLS: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];
static START: OnceLock<Instant> = OnceLock::new();

macro_rules! stub {
    ($name:literal, $index:literal) => {
        core::arch::global_asm!(
            ".text",
            concat!(".globl ", $name),
            ".p2align 4",
            concat!(".seh_proc ", $name),
            concat!($name, ":"),
            // Entry: rsp is 8 mod 16. 0x88 more makes it 16-aligned for
            // the call, with 32 bytes of shadow space at the bottom.
            "sub rsp, 0x88",
            ".seh_stackalloc 0x88",
            ".seh_endprologue",
            "mov [rsp + 0x60], rcx",
            "mov [rsp + 0x68], rdx",
            "mov [rsp + 0x70], r8",
            "mov [rsp + 0x78], r9",
            "movdqu [rsp + 0x20], xmm0",
            "movdqu [rsp + 0x30], xmm1",
            "movdqu [rsp + 0x40], xmm2",
            "movdqu [rsp + 0x50], xmm3",
            concat!("mov ecx, ", $index),
            "call {observe}",
            "movdqu xmm0, [rsp + 0x20]",
            "movdqu xmm1, [rsp + 0x30]",
            "movdqu xmm2, [rsp + 0x40]",
            "movdqu xmm3, [rsp + 0x50]",
            "mov rcx, [rsp + 0x60]",
            "mov rdx, [rsp + 0x68]",
            "mov r8, [rsp + 0x70]",
            "mov r9, [rsp + 0x78]",
            "add rsp, 0x88",
            // rax holds the real function; it is not an argument register.
            "jmp rax",
            ".seh_endproc",
            observe = sym observe_call,
        );
    };
}

stub!("CreateInterface", "0");
stub!("Steam_ReleaseThreadLocalMemory", "1");
stub!("Steam_BGetCallback", "2");
stub!("Steam_FreeLastCallback", "3");
stub!("Steam_GetAPICallResult", "4");

/// Called by the stubs: logs the call and returns the real function's
/// address. Never unwinds into the stubs; a panic ends the process.
extern "C" fn observe_call(index: u32) -> usize {
    match std::panic::catch_unwind(|| {
        let index = index as usize;
        record(index);
        targets()[index]
    }) {
        Ok(address) => address,
        Err(_) => std::process::abort(),
    }
}

fn record(index: usize) {
    let count = CALLS[index].fetch_add(1, Ordering::Relaxed) + 1;
    if count > LOG_FIRST && !count.is_multiple_of(LOG_EVERY) {
        return;
    }
    let elapsed = START.get_or_init(Instant::now).elapsed().as_millis();
    log(&format!(
        "{elapsed} ms {:?} {} call {count}",
        std::thread::current().id(),
        ENTRY_POINTS[index]
    ));
}

fn log(line: &str) {
    if let Some(Some(file)) = LOG.get_or_init(open_log).as_ref().map(Some) {
        let mut file = file.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = writeln!(file, "{line}");
    }
}

fn open_log() -> Option<Mutex<File>> {
    let path = own_dir()?.join(format!("observe-{}.log", std::process::id()));
    File::create(path).ok().map(Mutex::new)
}

fn targets() -> &'static [usize; 5] {
    TARGETS.get_or_init(|| match load_real() {
        Ok(targets) => targets,
        Err(reason) => {
            log(&format!("cannot forward: {reason}"));
            std::process::abort()
        }
    })
}

/// Loads the real `steamclient64.dll` from the saved path and looks up the
/// five entry points in it.
fn load_real() -> Result<[usize; 5], String> {
    let dir = own_dir().ok_or("cannot find the proxy's own folder")?;
    let original = std::fs::read_to_string(dir.join(SAVED))
        .map_err(|error| format!("cannot read the saved path: {error}"))?;
    log(&format!("loading the real DLL from {original}"));
    if !std::path::Path::new(&original).is_absolute() {
        return Err(format!("the saved path is not absolute: {original}"));
    }
    let wide: Vec<u16> = original.encode_utf16().chain(Some(0)).collect();
    // The real DLL needs DLLs from its own folder (`tier0_s64.dll` and
    // `vstdlib_s64.dll` load from the official client's folder right after
    // it, `docs/shim/loading.md`); a plain LoadLibraryW looked in the
    // game's folder and failed with error 126 (live, 2026-11-21). With an
    // absolute path, LOAD_WITH_ALTERED_SEARCH_PATH makes Windows search the
    // loaded DLL's folder first (Microsoft's LoadLibraryExW documentation).
    // SAFETY: `wide` is a NUL-terminated UTF-16 absolute path that lives for
    // the call; the file handle argument must be null.
    let module = unsafe {
        LoadLibraryExW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            LOAD_WITH_ALTERED_SEARCH_PATH,
        )
    };
    if module.is_null() {
        return Err(format!(
            "LoadLibraryExW failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut targets = [0usize; 5];
    for (slot, name) in targets.iter_mut().zip(ENTRY_POINTS) {
        let mut ascii = name.as_bytes().to_vec();
        ascii.push(0);
        // SAFETY: `module` is a loaded module that is never freed; `ascii`
        // is a NUL-terminated ASCII name that lives for the call.
        let address = unsafe { GetProcAddress(module, ascii.as_ptr()) };
        *slot = address
            .map(|function| function as usize)
            .ok_or_else(|| format!("the real DLL has no {name}"))?;
    }
    Ok(targets)
}

/// The folder this DLL was loaded from.
fn own_dir() -> Option<PathBuf> {
    let mut module: HMODULE = std::ptr::null_mut();
    // SAFETY: with FROM_ADDRESS the "name" is an address inside this module
    // (this function); UNCHANGED_REFCOUNT takes no reference that would
    // need releasing. `module` is a valid place for the handle.
    let found = unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            own_dir as *const () as *const u16,
            &mut module,
        )
    };
    if found == 0 {
        return None;
    }
    let mut buffer = vec![0u16; 32_768];
    let capacity = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
    // SAFETY: `module` is this module's handle; `buffer` is writable for
    // `capacity` UTF-16 units.
    let length = unsafe { GetModuleFileNameW(module, buffer.as_mut_ptr(), capacity) };
    if length == 0 || length >= capacity {
        return None;
    }
    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..length as usize]));
    path.parent().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The stubs, called with made-up signatures. The functions behind them
    // are the test functions below, so whatever arrives there arrived
    // through the stubs.
    unsafe extern "C" {
        #[link_name = "CreateInterface"]
        fn stub_integers(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64) -> u64;
        #[link_name = "Steam_ReleaseThreadLocalMemory"]
        fn stub_floats(a: f64, b: f32, c: f64, d: f32, e: f64) -> f64;
        #[link_name = "Steam_BGetCallback"]
        fn stub_mixed(a: u32, b: f64, c: *const u8, d: f32, e: u8, f: f64) -> u64;
        #[link_name = "Steam_FreeLastCallback"]
        fn stub_none();
        #[link_name = "Steam_GetAPICallResult"]
        fn stub_many(
            a: u64,
            b: u64,
            c: u64,
            d: u64,
            e: u64,
            f: u64,
            g: u64,
            h: u64,
            i: u64,
            j: u64,
        ) -> u64;
    }

    extern "C" fn integers(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64) -> u64 {
        a + 10 * b + 100 * c + 1_000 * d + 10_000 * e + 100_000 * f + 1_000_000 * g
    }

    extern "C" fn floats(a: f64, b: f32, c: f64, d: f32, e: f64) -> f64 {
        a + f64::from(b) * 2.0 + c * 4.0 + f64::from(d) * 8.0 + e * 16.0
    }

    extern "C" fn mixed(a: u32, b: f64, c: *const u8, d: f32, e: u8, f: f64) -> u64 {
        // SAFETY: the test passes a pointer to a live byte.
        let byte = unsafe { *c };
        u64::from(a)
            + (b * 10.0) as u64
            + u64::from(byte) * 100
            + (d * 1_000.0) as u64
            + u64::from(e) * 10_000
            + (f * 100_000.0) as u64
    }

    static NONE_CALLED: AtomicU64 = AtomicU64::new(0);

    extern "C" fn none() {
        NONE_CALLED.fetch_add(1, Ordering::Relaxed);
    }

    #[allow(clippy::too_many_arguments)]
    extern "C" fn many(
        a: u64,
        b: u64,
        c: u64,
        d: u64,
        e: u64,
        f: u64,
        g: u64,
        h: u64,
        i: u64,
        j: u64,
    ) -> u64 {
        [a, b, c, d, e, f, g, h, i, j]
            .iter()
            .enumerate()
            .map(|(position, value)| value * (position as u64 + 1))
            .sum()
    }

    #[test]
    fn stubs_forward_arguments_and_results_unchanged() {
        let _ = LOG.set(None);
        let _ = TARGETS.set([
            integers as *const () as usize,
            floats as *const () as usize,
            mixed as *const () as usize,
            none as *const () as usize,
            many as *const () as usize,
        ]);
        let byte = 7u8;
        // SAFETY: each stub jumps to the matching test function above,
        // which has exactly the declared signature.
        unsafe {
            assert_eq!(stub_integers(1, 2, 3, 4, 5, 6, 7), 7_654_321);
            assert_eq!(
                stub_floats(1.0, 2.0, 3.0, 4.0, 5.0),
                1.0 + 4.0 + 12.0 + 32.0 + 80.0
            );
            assert_eq!(
                stub_mixed(1, 2.0, &byte, 3.0, 4, 5.0),
                1 + 20 + 700 + 3_000 + 40_000 + 500_000
            );
            stub_none();
            assert_eq!(stub_many(1, 2, 3, 4, 5, 6, 7, 8, 9, 10), 385);
        }
        assert_eq!(NONE_CALLED.load(Ordering::Relaxed), 1);
        assert_eq!(CALLS[0].load(Ordering::Relaxed), 1);
        assert_eq!(CALLS[4].load(Ordering::Relaxed), 1);
    }
}
