//! Build steps for the observation proxy (ADR 0039, ADR 0040), only for
//! 64-bit Windows with the MSVC linker; elsewhere the proxy does not exist.
//!
//! 1. Exports the five entry points (defined in assembly in
//!    `src/proxy.rs`) from the DLL by name.
//! 2. Writes `method_stubs.rs` into `OUT_DIR`: the assembly stubs for the
//!    stand-in objects of step 3b, one per object and table position, and
//!    the tables pointing to them. `src/proxy.rs` includes it.

use std::fmt::Write as _;

/// The names a game asks `steamclient64.dll` for (`docs/shim/entry-points.md`).
const ENTRY_POINTS: [&str; 5] = [
    "CreateInterface",
    "Steam_ReleaseThreadLocalMemory",
    "Steam_BGetCallback",
    "Steam_FreeLastCallback",
    "Steam_GetAPICallResult",
];

/// Stand-in objects at most, and table positions per object. Keep in sync
/// with `STAND_INS` and `SLOTS` in `src/proxy.rs`.
const STAND_INS: usize = 32;
const SLOTS: usize = 128;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let x86_64 = std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("x86_64");
    let mut code = String::new();
    if windows && msvc && x86_64 {
        for name in ENTRY_POINTS {
            println!("cargo:rustc-cdylib-link-arg=/EXPORT:{name}");
        }
        method_stubs(&mut code);
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    if let Err(error) = std::fs::write(out.join("method_stubs.rs"), code) {
        panic!("cannot write method_stubs.rs: {error}");
    }
}

/// One stub per stand-in object and position. Each passes its number
/// (`object * SLOTS + position`) to `observe_method`; otherwise it is the
/// same as the entry point stubs in `src/proxy.rs` (see there for the
/// stack layout).
fn method_stubs(code: &mut String) {
    for object in 0..STAND_INS {
        for slot in 0..SLOTS {
            let name = format!("reborn_method_{object}_{slot}");
            let number = object * SLOTS + slot;
            let _ = write!(
                code,
                r#"core::arch::global_asm!(
    ".text",
    ".globl {name}",
    ".p2align 4",
    ".seh_proc {name}",
    "{name}:",
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
    "mov ecx, {number}",
    "lea rdx, [rsp + 0x20]",
    "mov r8, [rsp + 0x88]",
    "lea r9, [rsp + 0x88]",
    "call {{observe}}",
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
    "jmp rax",
    ".seh_endproc",
    observe = sym observe_method,
);
"#
            );
        }
    }
    code.push_str("unsafe extern \"C\" {\n");
    for object in 0..STAND_INS {
        for slot in 0..SLOTS {
            let _ = writeln!(code, "    fn reborn_method_{object}_{slot}();");
        }
    }
    code.push_str("}\n");
    for object in 0..STAND_INS {
        let _ = write!(
            code,
            "static METHOD_TABLE_{object}: [unsafe extern \"C\" fn(); {SLOTS}] = ["
        );
        for slot in 0..SLOTS {
            let _ = write!(code, "reborn_method_{object}_{slot}, ");
        }
        code.push_str("];\n");
    }
    let _ = write!(
        code,
        "/// The stub tables, one per stand-in object.\nstatic METHOD_TABLES: [&[unsafe extern \"C\" fn(); {SLOTS}]; {STAND_INS}] = ["
    );
    for object in 0..STAND_INS {
        let _ = write!(code, "&METHOD_TABLE_{object}, ");
    }
    code.push_str("];\n");
}
