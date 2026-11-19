# 0003: License split GPL / MIT-Apache

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

Reborn handles account tokens. A closed or tampered redistribution would be
dangerous for users. At the same time, good Rust crates for the Steam
protocol benefit the whole ecosystem and attract contributors.

## Problem

Which license(s) do we use?

## Alternatives considered

1. **MIT/Apache everywhere.** Maximum reuse, but closed forks of the client are allowed.
2. **GPL everywhere.** Strong protection, but other projects cannot easily reuse the protocol crates.
3. **Split.**

## Decision

- `rebornd`, `reborn-cli`, `reborn-core`, `reborn-platform`, `reborn-ipc`,
  `reborn-testkit` and the future GUI: **GPL-3.0-or-later**.
- `reborn-proto`, `reborn-formats`, `reborn-cm`, `reborn-content` and the
  future shim (loaded into proprietary game processes): **MIT OR Apache-2.0**.
- Contributions are signed off with the DCO; no CLA.

## Consequences

- MIT/Apache crates must never depend on GPL crates; `deny.toml` allows GPL
  only for the GPL crates by name.
- Code from SteamKit2 (LGPL-2.1) and DepotDownloader (GPL-2.0) may be read
  as reference but not copied or ported.
