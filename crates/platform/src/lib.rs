//! Operating system integration for Reborn.
//!
//! - [`secrets`]: refresh tokens and Steam Guard data in the OS credential
//!   store (Windows Credential Manager, Secret Service on Linux).
//! - [`settings`]: small non-secret settings such as the default account.
//!
//! Planned: locating Steam installations and library folders, process
//! launching and supervision (Windows Job Objects), Proton and Steam Linux
//! Runtime handling.
//!
//! This is one of the two crates where `unsafe` may be needed (the other is
//! the future shim). Every `unsafe` block must be justified with a
//! `// SAFETY:` comment and must be reviewed by a human.
#![deny(unsafe_code)]

pub mod secrets;
pub mod settings;
