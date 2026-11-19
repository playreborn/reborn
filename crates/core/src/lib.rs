//! Reborn domain logic, shared by `reborn-cli`, `rebornd` and the GUI.
//!
//! - [`account`]: signing in (password + Steam Guard, stored token), signing out.
//! - [`install`]: installing, updating and verifying apps in a library.
//! - [`library`]: apps installed in a library folder.
//!
//! This crate glues `reborn-cm` (Steam), `reborn-content` (downloads),
//! `reborn-formats` (Steam files) and `reborn-platform` (keyring, paths)
//! together. It never prints; progress is reported through callbacks.
#![forbid(unsafe_code)]

pub mod account;
mod error;
pub mod install;
pub mod library;

pub use error::CoreError;
