//! Observation tools for the `steamclient` replacement (ADR 0038,
//! ADR 0039). Not published; kept apart from the client (ADR 0012).
//!
//! - [`session`]: arming and disarming an observation session.
//! - [`daemon`]: the shim channel to `rebornd` for shadow answers about
//!   account data (ADR 0055).
//! - `registry` (Windows only): the one registry value a session changes.
//! - `proxy` (64-bit Windows only): the DLL a session loads instead of
//!   `steamclient64.dll`; forwards the entry points and logs the calls.
//!
//! Only `registry` and `proxy` may use `unsafe`, each block with a
//! `// SAFETY:` comment.
//! Changes here need human review (AGENTS.md section 7).
#![deny(unsafe_code)]

pub mod daemon;
#[cfg(all(windows, target_arch = "x86_64"))]
pub mod proxy;
#[cfg(windows)]
pub mod registry;
pub mod session;
