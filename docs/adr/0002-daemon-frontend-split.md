# 0002: Daemon / frontend split

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

A Steam client must keep downloading when its window is closed, and
Steamworks games need a long-running process with a Steam connection to talk
to (see the shim, plan section 8).

## Problem

Single GUI process, or a background daemon with separate frontends?

## Alternatives considered

1. **Single process (GUI does everything).** Simpler at first; downloads stop
   with the window; a GUI crash kills everything; no headless use; the shim
   would need the GUI to run.
2. **Daemon `rebornd` + thin frontends** (Iced GUI, CLI, community frontends)
   over local IPC.

## Decision

Option 2. `rebornd` owns the Steam connection, downloads and state. Frontends
only display state and send commands through `reborn-ipc`.

## Consequences

- An IPC API has to be designed and versioned (ADR 0008).
- Headless use (servers, scripts, SteamOS) and alternative frontends come for free.
- In M0/M1 the CLI may talk to Steam directly to speed up protocol work; it
  moves behind the daemon in M2.
