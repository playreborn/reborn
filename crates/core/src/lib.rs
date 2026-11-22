//! Reborn domain logic, shared by `reborn-cli`, `rebornd` and the GUI.
//!
//! - [`account`]: signing in (password + Steam Guard, stored token), signing out.
//! - [`cloud`]: comparing an app's Steam Cloud files with the local ones (reading only).
//! - [`compat`]: the compatibility database in `compat/` (tiers 0-5 per app and platform).
//! - [`install`]: installing, updating and verifying apps in a library.
//! - [`launch`]: starting installed games.
//! - [`library`]: apps installed in a library folder.
//! - [`run_cloud`]: the cloud folder a game reads through the shim (ADR 0055; reading only).
//! - [`uninstall`]: removing an installed app.
//!
//! This crate glues `reborn-cm` (Steam), `reborn-content` (downloads),
//! `reborn-formats` (Steam files) and `reborn-platform` (keyring, paths)
//! together. It never prints; progress is reported through callbacks.
#![forbid(unsafe_code)]

pub mod account;
pub mod cloud;
pub mod compat;
mod error;
pub mod install;
pub mod launch;
pub mod library;
pub mod run_cloud;
pub mod run_cloud_fill;
pub mod uninstall;

pub use error::CoreError;
