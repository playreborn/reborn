# 0021: The user's own online status

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

With friends visible (ADR 0020), the next M3 step is to appear online. The
official client sets the status with `CMsgClientChangeStatus` after logon.
Changing the status acts on the account and is visible to friends
(AGENTS.md § 7). The official client may run at the same time with the
same account; both would then set the status.

## Problem

When Reborn sets the status, and how to avoid surprising users who still
run the official client.

## Alternatives considered

1. **Always appear online after sign-in**, like the official client:
   expected for a full client, but overrides the status chosen in the
   official client whenever both run.
2. **Only when the user chose a status in Reborn:** nothing changes until
   the user picks one; the choice is remembered and applied after every
   sign-in.

## Decision

Option 2.

- `reborn-cm` sends `CMsgClientChangeStatus` (EMsg 716) with only
  `persona_state` (node-steam-user `components/friends.js`, `setPersona`;
  values from SteamKit2 `enums.steamd`, `EPersonaState`). `player_name`
  is never sent: it would rename the profile.
- The choice is a setting (`own-status`: online, away, busy, invisible or
  offline; not set by default). `rebornd` applies it when a session starts
  and right away when it changes.
- IPC protocol 1.6 (compatible additions): `set_own_status { status }`
  answers `settings`, which gains `own_status`.
- The desktop app offers the choice in the Friends tab.

## Consequences

- Clearing the choice ("not set by Reborn") does not reset a status Reborn
  already set; it stays until the session ends or the official client sets
  another one.
- Which status wins when both clients set one is up to Steam.
- Friend requests (accept, decline, send) and chat follow as separate
  steps.
