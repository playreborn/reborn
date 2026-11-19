//! Reborn domain logic.
//!
//! Planned responsibilities: accounts and sessions, the game library,
//! the download queue state machine and game launching. This crate glues
//! `reborn-cm`, `reborn-content` and `reborn-platform` together and is used
//! by `rebornd`. Frontends never depend on it directly; they use `reborn-ipc`.
//!
//! **Status:** skeleton, nothing implemented yet (milestones M0–M2, see
//! `docs/plan.md` section 6).
#![forbid(unsafe_code)]
