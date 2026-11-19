# 0007: steam-vent as the base of `reborn-cm`

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

M0 needs a working connection to Steam's CM servers: server discovery,
WebSocket transport over TLS, message framing, the `IAuthenticationService`
login flow, `CMsgClientLogon`, heartbeats and service method calls. Writing
this from scratch is weeks of work and exactly the kind of undocumented
protocol code where AI-generated code is most likely to be wrong.

## Alternatives considered

1. **Own implementation from scratch.** Full control, but slow and risky;
   references (SteamKit2, node-steam-user) are LGPL/MIT in other languages.
2. **[steam-vent](https://codeberg.org/steam-vent/steam-vent) 0.5** (Rust,
   MIT, updated April 2026, ~2.6k recent downloads). Has WebSocket and TCP
   transports, password login with Steam Guard (code and mobile-app
   confirmation), access/refresh-token login, service method calls, game
   coordinator support and generated protobufs (`steam-vent-proto-steam`).
   Missing: QR-code login; the unauthenticated service-method helper needed
   to add it from outside is crate-private.
3. **steamkit-rs** (Rust port of SteamKit2). LGPL-2.1-only, which conflicts
   with our MIT/Apache protocol crates (ADR 0003), and only days old.

## Decision

Use steam-vent behind `reborn-cm`'s own API (`login_with_password`,
`login_with_token`, `Session::owned_games`, ...). Only `reborn-cm` depends on
steam-vent; the rest of Reborn uses `reborn-cm` types. Steam Guard
confirmation handlers are re-exported from steam-vent for now
(`reborn_cm::confirmation`).

Protobufs come from `steam-vent-proto-steam`, which is generated from the
community-maintained SteamDatabase/Protobufs definitions. `reborn-proto`
stays reserved for messages steam-vent does not ship.

## Consequences

- M0 login and library listing work now, with password + Steam Guard.
- **QR login** needs an upstream contribution (expose unauthenticated
  service calls or add `BeginAuthSessionViaQR`). Tracked as a follow-up.
- steam-vent pins `protobuf =3.5.1`, which has RUSTSEC-2024-0437 (fixed in
  3.7.2). Ignored in `deny.toml` with a reason; an upstream bump is tracked.
- The dependency tree grows to several hundred crates (reqwest, rustls,
  tokio-tungstenite, rsa, ...). Acceptable for a network client; cargo-deny
  checks licenses and advisories.
- MSRV becomes 1.88 (steam-vent and keyring require it).
- If steam-vent stalls, `reborn-cm`'s API lets us replace it piece by piece
  without touching the rest of Reborn. Contributing upstream is preferred
  over forking.
