//! Operating system integration for Reborn.
//!
//! Planned responsibilities: locating Steam installations and library
//! folders, OS secret storage (Windows Credential Manager, Secret Service),
//! process launching and supervision (Windows Job Objects), Proton and
//! Steam Linux Runtime handling.
//!
//! This is one of the two crates where `unsafe` may be needed (the other is
//! the future shim). Every `unsafe` block must be justified with a
//! `// SAFETY:` comment and must be reviewed by a human.
//!
//! **Status:** skeleton, nothing implemented yet (milestones M0–M2, see
//! `docs/plan.md` section 9).
#![deny(unsafe_code)]
