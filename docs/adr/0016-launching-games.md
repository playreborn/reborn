# 0016: Launching games

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

M2 stage 4 adds "Play". The app info of every app has a launch section
(`config/launch/<n>`) with executable, arguments, working directory, type,
description and per-entry restrictions (`config/oslist`, `config/osarch`,
`config/betakey`). There is no public reference implementation that reads
it, so the layout is checked against recorded app info (fixtures in
`crates/cm/tests/data/`).

Most games use Steamworks and expect the official client to run. Until
the `steamclient` replacement (M4), Reborn reaches compatibility tier 1
from the plan (§ 8.5) for them: the game starts, and the official client
must run in the background. DRM-free games run on their own. Reborn
never patches games, writes `steam_appid.txt` into game folders or skips
any check (red lines, AGENTS.md § 2).

## Problem

Where does launching live, which entry is started, and how do frontends
start and observe games?

## Alternatives considered

1. **Frontends start the process:** each frontend would need app info
   (a Steam session) and path checks; breaks ADR 0002.
2. **The daemon starts and watches the process:** one place for checks,
   the running state is shared by all frontends, and it later grows into
   process supervision (Windows Job Objects, Proton).

## Decision

Option 2.

- `reborn-cm` parses the launch section into `LaunchOption` (public API
  addition) and `Session::app_info_text` returns the raw app info for
  recording fixtures.
- `reborn-core::launch` reads the appmanifest (only fully installed apps),
  picks an entry for this OS, architecture and branch (requested index,
  else the first of type `default` or without type, else the first
  matching), and resolves paths with the same checks as depot file names
  (`install_path`); a leading `./` is dropped. Arguments are split on
  whitespace with double quotes grouping words; nothing else is
  interpreted. The process gets `SteamAppId` and `SteamGameId`, which the
  official client sets and Valve's Proton launcher reads
  (ValveSoftware/Proton, `proton`, branch `proton_9.0`).
- IPC protocol 1.2 (compatible additions): request `launch { app_id,
  library, option }` answers `launched { app_id }`; events `game_started`
  and `game_exited { exit_code }`; `status` gains `running_games`. A
  launch is refused while the app is installing or already running.
- Windows-only games on Linux fail with a clear error until Proton
  support lands (it builds on the Proton discovery from the compatibility
  work, ADR 0015).

## Consequences

- Live test 2026-11-20 (Bongo Cat, test account, official client running
  with the same account): installing, starting and exiting work; Steam
  shows the account as playing the game, also to friends. Opening an
  in-game chest (Steam Inventory) failed once with "check your Steam
  connection" and worked on the next start through Reborn, so the first
  failure looks like a temporary Steam-side error.
- Games that hand over to a launcher look "exited" when the first process
  ends. Job Objects (Windows) and process groups (Linux) can follow.
- Install scripts (redistributables such as DirectX or VC++) are not run
  yet; games that need them may fail to start on a fresh system.
- Launch options chosen by the user and per-game command-line overrides
  come with the settings screen.
