//! Steam protocol definitions for Reborn.
//!
//! This crate will contain the protobuf messages, `EMsg` values and enums
//! needed to talk to Steam. All protobuf code is **generated** from the
//! community-maintained definitions (SteamDatabase/Protobufs) at build time.
//! Never write or edit protobuf structs by hand.
//!
//! **Status:** skeleton, nothing implemented yet (milestone M0, see
//! `docs/plan.md` section 7).
#![forbid(unsafe_code)]
