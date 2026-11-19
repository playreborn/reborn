# 0008: IPC between rebornd and its frontends

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

ADR 0002 splits Reborn into the daemon `rebornd` and frontends (GUI, CLI,
community frontends). They need a local, cross-platform channel with
requests, responses and pushed events (download progress, job finished).

## Alternatives considered

1. **gRPC / HTTP on localhost:** heavy, opens a TCP port other local users
   or browsers could reach.
2. **D-Bus:** Linux only.
3. **Local socket with length-delimited binary (postcard):** compact, but
   hard to debug and to use from other languages.
4. **Local socket with newline-delimited JSON:** easy to debug (`nc`/pipe
   tools), usable from any language, more than fast enough for control
   messages (bulk data never goes through IPC).

## Decision

- Transport: named pipe on Windows, Unix domain socket on Linux, via the
  [`interprocess`](https://crates.io/crates/interprocess) crate (0BSD OR
  Apache-2.0) with its Tokio support. The socket is per user; on Linux it
  lives in `$XDG_RUNTIME_DIR` with permissions `0600`.
- Format: one JSON object per line (`serde_json`). Messages:
  - client → daemon: `{"id": <u64>, "request": {...}}`
  - daemon → client: `{"id": <u64>, "response": {...}}` or
    `{"event": {...}}` (no id; pushed to subscribed clients)
- The first exchange is a `Hello` with the protocol version; a daemon
  refuses clients with an incompatible major version.
- The message types live in `reborn-ipc`; community frontends depend on
  that crate or follow its documented JSON.

## Consequences

- No network port; only the local user can connect.
- Message types are the daemon's public API: changes go through an ADR,
  additions bump the minor protocol version.
