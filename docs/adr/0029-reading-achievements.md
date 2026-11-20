# 0029: Reading achievements

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

ADR 0012 lists "user stats and achievements (reading and displaying)" as
daemon-side M4 work. Steam answers `ClientGetUserStats` (EMsg 818) with
`ClientGetUserStatsResponse` (EMsg 819). A SteamKit maintainer points to
these messages for achievements and expects the `schema` field to be
binary KeyValues (SteamKit issue #531). The request values come from
`scripts/generate_emu_config.py` of the Goldberg emulator (LGPL, read
only, nothing copied). The schema layout (`stats → <id> → bits → <bit>`
with `name` and `display`) is described by Steam Achievement Manager
`SAM.Game/Manager.cs` (zlib, read only). Neither tool is a usual Reborn
source, so the behaviour counts as verified only after our own recording.

## Problem

How `reborn-cm` reads an account's achievements without guessing, and
without ever writing stats (red line 4).

## Alternatives considered

1. **Web API `ISteamUserStats/GetPlayerAchievements`:** needs a Web API
   key and a public profile. Rejected.
2. **Service method `Player.GetGameAchievements`:** names, icons and
   global percentages, but no unlock state for the account.
3. **`ClientGetUserStats` over the CM connection:** schema and unlock
   times in one answer, the same messages the shim needs later.

## Decision

Option 3. Option 2 may be added later for icons and global percentages.

- `Session::achievements(app_id, language)` sends `ClientGetUserStats`
  as a job and returns `Achievement` entries: internal name, display
  name and description in the asked language (falling back to English),
  hidden flag, unlock time. `Session::record_achievements(app_id)` returns
  the answer for fixtures. New public items in `reborn-cm`: `Achievement`,
  `achievements_from`, `FALLBACK_LANGUAGE`, `Error::StatsSchema`.
- steam-vent-proto-steam does not tie the user stats messages to EMsg
  numbers, so `reborn-cm` wraps them in two private types that do.
- `reborn-cli achievements <app_id> [--language] [--record <dir>]`
  prints the list; `--record` writes `user_stats.bin`.
- Reborn never sends `ClientStoreUserStats` or `ClientStoreUserStats2`.
  Writing happens only later in the shim, when a game asks (red line 4).
- No daemon, IPC or GUI part yet; they follow once the recording has
  confirmed the format.

## Consequences

- The answer has the game's schema, but no SteamID. Unlock times of the
  test account end up in the fixture.
- One request per call; nothing runs in the background (red line 8).
