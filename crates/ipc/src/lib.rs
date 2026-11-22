//! The local API between `rebornd` and its frontends (ADR 0008).
//!
//! - [`protocol`]: requests, responses and events, serialized as JSON.
//! - [`frame`]: one JSON object per line on the wire.
//! - [`socket`]: where the daemon listens (per-user named pipe or Unix socket).
//! - [`Client`]: connects, says hello, sends requests and receives events.
//!
//! Community frontends can use this crate, or speak the same JSON directly:
//! connect to the socket, send `{"id":1,"request":{"type":"hello",...}}`
//! and read one JSON object per line.
#![forbid(unsafe_code)]

mod client;
pub mod frame;
pub mod protocol;
pub mod socket;

pub use client::{Client, ClientError};
pub use protocol::{
    ChatText, Event, PROTOCOL_MAJOR, PROTOCOL_MINOR, Request, Response, SHIM_MODE_NOTICE, Secret,
};
