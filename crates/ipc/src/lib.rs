//! API types shared between `rebornd` and its frontends.
//!
//! Planned: versioned request/response messages and an event stream
//! (download progress, friend status, game status) over a local socket
//! (named pipe on Windows, Unix socket on Linux). Community frontends build
//! on this crate, so its public API only changes through an ADR.
//!
//! **Status:** skeleton, nothing implemented yet (milestone M2, see
//! `docs/plan.md` section 6.2).
#![forbid(unsafe_code)]
