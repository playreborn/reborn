//! The observation proxy (ADR 0038 stage 2, ADR 0039): it forwards the five
//! entry points a game asks `steamclient64.dll` for
//! (`docs/shim/entry-points.md`) to the real DLL and logs each call's raw
//! argument registers and its raw result.
//!
//! Nothing is known about the entry points' arguments, so each one is a
//! small assembly stub (Microsoft x64 calling convention, as documented by
//! Microsoft):
//!
//! 1. It saves the four integer and four floating-point argument registers
//!    and hands a copy, the game's return address and the stack position
//!    to `observe_call`, which logs the call, remembers the return address
//!    in a per-thread stack and returns the real function's address.
//! 2. It replaces the return address on the stack with the return stub,
//!    puts the registers back, restores the stack pointer and jumps to the
//!    real function, which so sees the game's exact arguments.
//! 3. When the real function returns, it lands in the return stub, which
//!    hands `rax` and `xmm0` (the result registers) to `observe_return`,
//!    gets the game's return address back and jumps there with the result
//!    untouched.
//!
//! Logged per call: the raw register values; for an integer register that
//! points to readable memory holding 2 to 64 printable ASCII characters
//! and a NUL, that text. Memory is checked with `VirtualQuery` before it is
//! read. Nothing else is read, so binary data such as tickets never ends up
//! in the log.
//!
//! The real DLL is loaded on the first call, not while this DLL is being
//! loaded (Windows forbids loading libraries from `DllMain`). Its path is
//! the value `reborn-observe arm` saved next to this DLL.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::fmt::Write as _;
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
use windows_sys::Win32::System::Memory::{
    MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
    PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY,
    VirtualQuery,
};
use windows_sys::Win32::System::ProcessStatus::{K32GetModuleInformation, MODULEINFO};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

use crate::session::SAVED;

/// Position of `CreateInterface` in [`ENTRY_POINTS`].
const CREATE_INTERFACE: usize = 0;

/// Most table entries counted in step 3a (ADR 0040).
const MAX_TABLE: usize = 1024;

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

/// Longest text logged for a register that points to ASCII.
const MAX_TEXT: usize = 64;

/// Addresses of the real functions, in [`ENTRY_POINTS`] order.
static TARGETS: OnceLock<[usize; 5]> = OnceLock::new();
/// Start and end of the real `steamclient64.dll` in memory.
static MODULE: OnceLock<(usize, usize)> = OnceLock::new();
/// The log file, `None` if it cannot be opened (or in tests).
static LOG: OnceLock<Option<Mutex<File>>> = OnceLock::new();
static CALLS: [AtomicU64; 5] = [const { AtomicU64::new(0) }; 5];
static START: OnceLock<Instant> = OnceLock::new();

/// The argument registers as a stub saved them (offsets 0x20 to 0x80 of
/// its stack frame).
#[repr(C)]
struct Saved {
    xmm: [[u64; 2]; 4],
    rcx: u64,
    rdx: u64,
    r8: u64,
    r9: u64,
}

/// A call that has not returned yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Pending {
    /// Stack pointer at the stub's entry (where the return address is).
    entry_rsp: usize,
    return_address: usize,
    index: usize,
    call: u64,
    logged: bool,
}

thread_local! {
    static PENDING: RefCell<Vec<Pending>> = const { RefCell::new(Vec::new()) };
}

macro_rules! stub {
    ($name:literal, $index:literal) => {
        core::arch::global_asm!(
            ".text",
            concat!(".globl ", $name),
            ".p2align 4",
            concat!(".seh_proc ", $name),
            concat!($name, ":"),
            // Entry: rsp is 8 mod 16 and points to the return address.
            // 0x88 more makes it 16-aligned for the call, with 32 bytes of
            // shadow space at the bottom.
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
            "lea rdx, [rsp + 0x20]",
            "mov r8, [rsp + 0x88]",
            "lea r9, [rsp + 0x88]",
            "call {observe}",
            // The real function returns into the return stub.
            "lea r10, [rip + reborn_observe_return_stub]",
            "mov [rsp + 0x88], r10",
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

core::arch::global_asm!(
    ".text",
    ".globl reborn_observe_return_stub",
    ".p2align 4",
    ".seh_proc reborn_observe_return_stub",
    "reborn_observe_return_stub:",
    // Entry: the real function's `ret` popped the stub's address, so rsp
    // is 16-aligned and 8 above the stub entry's rsp. 0x40 keeps it
    // aligned, with shadow space and room for rax and xmm0.
    "sub rsp, 0x40",
    ".seh_stackalloc 0x40",
    ".seh_endprologue",
    "mov [rsp + 0x20], rax",
    "movdqu [rsp + 0x30], xmm0",
    "mov rcx, rax",
    "movq rdx, xmm0",
    "lea r8, [rsp + 0x40]",
    "call {observe}",
    // rax now holds the game's return address; r11 is free to use.
    "mov r11, rax",
    "mov rax, [rsp + 0x20]",
    "movdqu xmm0, [rsp + 0x30]",
    "add rsp, 0x40",
    "jmp r11",
    ".seh_endproc",
    observe = sym observe_return,
);

/// Called by the stubs: logs the call, remembers where it returns to and
/// gives the real function's address. Never unwinds into the stubs; a
/// panic ends the process.
extern "C" fn observe_call(
    index: u32,
    saved: *const Saved,
    return_address: usize,
    entry_rsp: usize,
) -> usize {
    let result = std::panic::catch_unwind(|| {
        let index = index as usize;
        let call = CALLS[index].fetch_add(1, Ordering::Relaxed) + 1;
        let logged = should_log(call);
        if logged {
            // SAFETY: the stub passes a pointer to its saved registers,
            // which stay in place until this function returns.
            let saved = unsafe { &*saved };
            log(&describe_call(index, call, saved));
        }
        PENDING.with(|pending| {
            pending.borrow_mut().push(Pending {
                entry_rsp,
                return_address,
                index,
                call,
                logged,
            });
        });
        targets()[index]
    });
    result.unwrap_or_else(|_| std::process::abort())
}

/// Called by the return stub: logs the result and gives the game's
/// return address. `after_rsp` is the stack pointer after the real
/// function returned.
extern "C" fn observe_return(rax: u64, xmm0: u64, after_rsp: usize) -> usize {
    let result = std::panic::catch_unwind(|| {
        let found = PENDING.with(|pending| take_pending(&mut pending.borrow_mut(), after_rsp - 8));
        let Some(call) = found else {
            log("return without a matching call; stopping");
            std::process::abort();
        };
        if call.logged {
            log(&format!(
                "{} ms {:?} {} return {} rax=0x{rax:x} xmm0=0x{xmm0:x}",
                elapsed_ms(),
                std::thread::current().id(),
                ENTRY_POINTS[call.index],
                call.call
            ));
            // Step 3a of ADR 0040: what does CreateInterface's result look
            // like? Read only; the game gets the result unchanged.
            if call.index == CREATE_INTERFACE
                && let Some(&module) = MODULE.get()
            {
                log(&format!(
                    "{} ms {:?} {} result {} {}",
                    elapsed_ms(),
                    std::thread::current().id(),
                    ENTRY_POINTS[call.index],
                    call.call,
                    describe_shape(rax, module)
                ));
            }
        }
        call.return_address
    });
    result.unwrap_or_else(|_| std::process::abort())
}

/// Finds the call that returns now: the one whose stub entry had stack
/// pointer `entry_rsp`. Entries for deeper frames left behind (a call that
/// never returned normally) are dropped first.
fn take_pending(pending: &mut Vec<Pending>, entry_rsp: usize) -> Option<Pending> {
    while pending
        .last()
        .is_some_and(|call| call.entry_rsp < entry_rsp)
    {
        pending.pop();
    }
    if pending
        .last()
        .is_some_and(|call| call.entry_rsp == entry_rsp)
    {
        pending.pop()
    } else {
        None
    }
}

fn should_log(call: u64) -> bool {
    call <= LOG_FIRST || call.is_multiple_of(LOG_EVERY)
}

fn elapsed_ms() -> u128 {
    START.get_or_init(Instant::now).elapsed().as_millis()
}

fn describe_call(index: usize, call: u64, saved: &Saved) -> String {
    let mut line = format!(
        "{} ms {:?} {} call {call} rcx=0x{:x} rdx=0x{:x} r8=0x{:x} r9=0x{:x}",
        elapsed_ms(),
        std::thread::current().id(),
        ENTRY_POINTS[index],
        saved.rcx,
        saved.rdx,
        saved.r8,
        saved.r9
    );
    for (number, xmm) in saved.xmm.iter().enumerate() {
        let _ = write!(line, " xmm{number}=0x{:x}", xmm[0]);
    }
    for (name, value) in [
        ("rcx", saved.rcx),
        ("rdx", saved.rdx),
        ("r8", saved.r8),
        ("r9", saved.r9),
    ] {
        if let Some(text) = ascii_at(value) {
            let _ = write!(line, " {name}->\"{text}\"");
        }
    }
    line
}

/// The printable ASCII text at `address`, if the memory there is readable
/// and holds 2 to [`MAX_TEXT`] printable characters followed by a NUL,
/// all within one memory region.
fn ascii_at(address: u64) -> Option<String> {
    // The lowest 64 KiB are never mapped on Windows; this also skips small
    // numbers that are not pointers.
    if address < 0x1_0000 {
        return None;
    }
    let start = usize::try_from(address).ok()?;
    let end = readable_end(start)?;
    let mut text = String::new();
    for offset in 0..=MAX_TEXT {
        let at = start.checked_add(offset)?;
        if at >= end {
            return None;
        }
        // SAFETY: `at` lies inside a committed, readable region that
        // `readable_end` just checked.
        let byte = unsafe { std::ptr::read_volatile(at as *const u8) };
        match byte {
            0 if text.len() >= 2 => return Some(text),
            0x20..=0x7e => text.push(char::from(byte)),
            _ => return None,
        }
    }
    None
}

/// Windows' information about the memory at `address`, if it is
/// committed and not a guard page.
fn region(address: usize) -> Option<MEMORY_BASIC_INFORMATION> {
    // SAFETY: MEMORY_BASIC_INFORMATION is plain data; all zeros is valid.
    let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
    let size = size_of::<MEMORY_BASIC_INFORMATION>();
    // SAFETY: VirtualQuery only inspects the address space; `info` is
    // writable for `size` bytes.
    let written = unsafe { VirtualQuery(address as *const _, &mut info, size) };
    if written == 0 || info.State != MEM_COMMIT || info.Protect & PAGE_GUARD != 0 {
        return None;
    }
    Some(info)
}

/// The end of the committed, readable memory region containing `address`.
fn readable_end(address: usize) -> Option<usize> {
    let info = region(address)?;
    let readable = PAGE_READONLY
        | PAGE_READWRITE
        | PAGE_WRITECOPY
        | PAGE_EXECUTE_READ
        | PAGE_EXECUTE_READWRITE
        | PAGE_EXECUTE_WRITECOPY;
    if info.Protect & readable == 0 {
        return None;
    }
    (info.BaseAddress as usize).checked_add(info.RegionSize)
}

/// Whether Windows marks the memory at `address` as executable. Only the
/// protection is asked for; the memory itself is not read.
fn is_executable(address: usize) -> bool {
    let executable =
        PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
    region(address).is_some_and(|info| info.Protect & executable != 0)
}

/// Eight bytes at `address`, if they lie in readable memory.
fn read_u64(address: usize) -> Option<u64> {
    let end = readable_end(address)?;
    if address.checked_add(8)? > end {
        return None;
    }
    // SAFETY: the eight bytes lie inside a committed, readable region that
    // `readable_end` just checked; no alignment is assumed.
    Some(unsafe { std::ptr::read_unaligned(address as *const u64) })
}

/// Step 3a of ADR 0040: whether the first field of `object` holds an
/// address inside the module `[base, end)`, and if so, how many
/// consecutive eight-byte values from there are addresses inside the
/// module that Windows marks as executable. Only pointer values in data
/// are read, never code bytes.
fn describe_shape(object: u64, (base, end): (usize, usize)) -> String {
    let Ok(object) = usize::try_from(object) else {
        return "shape: not an address".to_owned();
    };
    if object < 0x1_0000 {
        return "shape: not an address".to_owned();
    }
    let Some(first) = read_u64(object).and_then(|value| usize::try_from(value).ok()) else {
        return "shape: object not readable".to_owned();
    };
    if !(base..end).contains(&first) {
        return "shape: first field does not point into steamclient64".to_owned();
    }
    let mut entries = 0;
    while entries < MAX_TABLE {
        let Some(entry) = first
            .checked_add(entries * 8)
            .and_then(read_u64)
            .and_then(|value| usize::try_from(value).ok())
        else {
            break;
        };
        if !(base..end).contains(&entry) || !is_executable(entry) {
            break;
        }
        entries += 1;
    }
    format!(
        "shape: first field -> steamclient64+0x{:x}, {entries} entries point into its executable memory{}",
        first - base,
        if entries == MAX_TABLE {
            " (limit reached)"
        } else {
            ""
        }
    )
}

/// Start and end of a loaded module (Microsoft's GetModuleInformation).
fn module_range(module: HMODULE) -> Option<(usize, usize)> {
    // SAFETY: MODULEINFO is plain data; all zeros is valid.
    let mut info: MODULEINFO = unsafe { std::mem::zeroed() };
    let size = u32::try_from(size_of::<MODULEINFO>()).ok()?;
    // SAFETY: `module` is a module loaded in this process; `info` is
    // writable for `size` bytes; GetCurrentProcess returns a pseudo handle
    // that needs no closing.
    let found = unsafe { K32GetModuleInformation(GetCurrentProcess(), module, &mut info, size) };
    if found == 0 {
        return None;
    }
    let base = info.lpBaseOfDll as usize;
    Some((base, base.checked_add(info.SizeOfImage as usize)?))
}

fn log(line: &str) {
    if let Some(file) = LOG.get_or_init(open_log) {
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
    match module_range(module) {
        Some(range) => {
            let _ = MODULE.set(range);
        }
        None => log("cannot find the real DLL's memory range; shapes are not logged"),
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
    // through the stubs, and whatever comes back went through the return
    // stub.
    unsafe extern "C" {
        #[link_name = "CreateInterface"]
        fn stub_integers(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64) -> u64;
        #[link_name = "Steam_ReleaseThreadLocalMemory"]
        fn stub_floats(a: f64, b: f32, c: f64, d: f32, e: f64) -> f64;
        #[link_name = "Steam_BGetCallback"]
        fn stub_mixed(a: u32, b: f64, c: *const u8, d: f32, e: u8, f: f64) -> u64;
        #[link_name = "Steam_FreeLastCallback"]
        fn stub_nested() -> u64;
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

    /// Calls another stub from inside a forwarded call, as a real
    /// function calling back into the game would.
    extern "C" fn nested() -> u64 {
        // SAFETY: as in the test below.
        unsafe { stub_integers(1, 1, 1, 1, 1, 1, 1) + 1 }
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
            nested as *const () as usize,
            many as *const () as usize,
        ]);
        let byte = 7u8;
        // SAFETY: each stub jumps to the matching test function above,
        // which has exactly the declared signature.
        unsafe {
            for _ in 0..3 {
                assert_eq!(stub_integers(1, 2, 3, 4, 5, 6, 7), 7_654_321);
                assert_eq!(
                    stub_floats(1.0, 2.0, 3.0, 4.0, 5.0),
                    1.0 + 4.0 + 12.0 + 32.0 + 80.0
                );
                assert_eq!(
                    stub_mixed(1, 2.0, &byte, 3.0, 4, 5.0),
                    1 + 20 + 700 + 3_000 + 40_000 + 500_000
                );
                assert_eq!(stub_nested(), 1_111_111 + 1);
                assert_eq!(stub_many(1, 2, 3, 4, 5, 6, 7, 8, 9, 10), 385);
            }
        }
        PENDING.with(|pending| assert!(pending.borrow().is_empty(), "every call returned"));
        assert_eq!(CALLS[3].load(Ordering::Relaxed), 3);
    }

    #[test]
    fn reads_only_readable_ascii() {
        let text = c"SteamClient021";
        assert_eq!(
            ascii_at(text.as_ptr() as u64).as_deref(),
            Some("SteamClient021")
        );
        assert_eq!(ascii_at(c"x".as_ptr() as u64), None, "too short");
        assert_eq!(ascii_at(c"ab\x01cd".as_ptr() as u64), None, "not printable");
        let long = [b'a'; 100];
        assert_eq!(ascii_at(long.as_ptr() as u64), None, "too long");
        assert_eq!(ascii_at(0), None);
        assert_eq!(ascii_at(0x10), None);
        assert_eq!(ascii_at(0x7fff_ffff_0000), None, "not mapped");
        assert_eq!(ascii_at(u64::MAX), None);
    }

    fn pending(entry_rsp: usize, return_address: usize) -> Pending {
        Pending {
            entry_rsp,
            return_address,
            index: 0,
            call: 1,
            logged: false,
        }
    }

    #[test]
    fn returns_match_their_calls() {
        // Nested: the inner call (lower stack) returns first.
        let mut stack = vec![pending(0x9000, 1), pending(0x8000, 2)];
        assert_eq!(
            take_pending(&mut stack, 0x8000).map(|p| p.return_address),
            Some(2)
        );
        assert_eq!(
            take_pending(&mut stack, 0x9000).map(|p| p.return_address),
            Some(1)
        );
        assert!(stack.is_empty());
        // A deeper call that never returned is dropped.
        let mut stack = vec![pending(0x9000, 1), pending(0x7000, 3)];
        assert_eq!(
            take_pending(&mut stack, 0x9000).map(|p| p.return_address),
            Some(1)
        );
        assert!(stack.is_empty());
        // No match leaves the outer calls in place.
        let mut stack = vec![pending(0x9000, 1)];
        assert_eq!(take_pending(&mut stack, 0x8000), None);
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn logs_the_first_calls_then_every_ten_thousandth() {
        assert!(should_log(1));
        assert!(should_log(50));
        assert!(!should_log(51));
        assert!(should_log(10_000));
        assert!(!should_log(10_001));
    }

    extern "C" fn slot_a() {}
    extern "C" fn slot_b() {}
    extern "C" fn slot_c() {}

    /// The test program's own module stands in for steamclient64.dll.
    fn own_module() -> (usize, usize) {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        // SAFETY: a null name asks for the program's own module handle.
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        module_range(module).unwrap_or((0, 0))
    }

    #[test]
    fn counts_a_table_of_function_addresses() {
        let module = own_module();
        assert!(module.1 > module.0, "module range found");
        // An "object" whose first field points to a table of three function
        // addresses in the module, then something that is not one. The
        // table itself has to lie inside the module too, so it is a static.
        static TABLE: OnceLock<[usize; 4]> = OnceLock::new();
        let table = TABLE.get_or_init(|| {
            [
                slot_a as *const () as usize,
                slot_b as *const () as usize,
                slot_c as *const () as usize,
                0,
            ]
        });
        let object = [table.as_ptr() as usize];
        let shape = describe_shape(object.as_ptr() as u64, module);
        assert!(
            shape.contains("3 entries point into its executable memory"),
            "{shape}"
        );
    }

    #[test]
    fn reports_other_shapes_without_reading_further() {
        let module = own_module();
        // First field points outside the module (to the heap).
        let elsewhere = Box::new(5u64);
        let object = [&*elsewhere as *const u64 as usize];
        assert!(
            describe_shape(object.as_ptr() as u64, module)
                .contains("does not point into steamclient64")
        );
        assert!(describe_shape(0x10, module).contains("not an address"));
        assert!(describe_shape(0x7fff_ffff_0000, module).contains("not readable"));
        assert!(!is_executable(&elsewhere as *const _ as usize));
        assert!(is_executable(slot_a as *const () as usize));
    }
}
