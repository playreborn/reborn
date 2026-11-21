//! Exports the observation proxy's entry points from the DLL (ADR 0039).
//!
//! The entry points are defined in assembly (`src/proxy.rs`), so the
//! linker is told to export them by name. Only for 64-bit Windows with the
//! MSVC linker; elsewhere the proxy does not exist.

/// The names a game asks `steamclient64.dll` for (`docs/shim/entry-points.md`).
const ENTRY_POINTS: [&str; 5] = [
    "CreateInterface",
    "Steam_ReleaseThreadLocalMemory",
    "Steam_BGetCallback",
    "Steam_FreeLastCallback",
    "Steam_GetAPICallResult",
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let x86_64 = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64");
    if windows && msvc && x86_64 {
        for name in ENTRY_POINTS {
            println!("cargo:rustc-cdylib-link-arg=/EXPORT:{name}");
        }
    }
}
