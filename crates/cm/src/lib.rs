//! Client for Steam's Connection Manager (CM) protocol.
//!
//! Planned responsibilities: CM server discovery, WebSocket transport over TLS,
//! message framing, job IDs, heartbeats, reconnects with backoff, the
//! `IAuthenticationService` login flows (QR code, password + Steam Guard) and
//! unified service method calls.
//!
//! Every protocol behaviour implemented here must cite a reference
//! implementation or a recorded fixture (see `AGENTS.md`).
//!
//! **Status:** skeleton, nothing implemented yet (milestone M0, see
//! `docs/plan.md` sections 7.1–7.3).
#![forbid(unsafe_code)]
