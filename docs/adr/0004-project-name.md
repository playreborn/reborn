# 0004: Project name

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

The working title was "Steam Reborn". "Steam" is a trademark of Valve; using
it invites a takedown and suggests an official product.

## Decision

- Project name: **Reborn**.
- GitHub organisation: `playreborn` (`reborn` was taken); main repository `playreborn/reborn`.
- crates.io: `reborn`; further crates use the `reborn-` prefix.
- Binaries: `reborn` (GUI), `rebornd` (daemon), `reborn-cli`.
- Tagline wherever the project is presented: *"Reborn: a native, open-source
  client for your Steam library."* "Steam" appears only descriptively, never
  in the name or logo.

## Consequences

- "Reborn" is generic and hard to search for; the fixed tagline compensates.
- Placeholder crates are not allowed on crates.io; further names are secured
  by publishing real crates early (first: `reborn-formats`).
- A short trademark search (DPMA/EUIPO, software class) is done before the public launch.
