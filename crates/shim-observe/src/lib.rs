//! Observation tools for the `steamclient` replacement (ADR 0038,
//! ADR 0039). Not published; kept apart from the client (ADR 0012).
//!
//! - [`session`]: arming and disarming an observation session.
//! - `registry` (Windows only): the one registry value a session changes.
//!
//! Only `registry` may use `unsafe`, each block with a `// SAFETY:` comment.
//! Changes here need human review (AGENTS.md section 7).
#![deny(unsafe_code)]

#[cfg(windows)]
pub mod registry;
pub mod session;
