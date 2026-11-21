# Coverage

Which interface positions games were seen calling, and how far each one
is. Positions come only from recordings (`objects.md` and the raw logs
of the runs below); meanings only from `meanings.md` (ADR 0042). The
shim implements only confirmed meanings, so this table is also the
shim's to-do list. Nothing is implemented yet.

Status: **confirmed** (experiment matched, maintainer's OK), **candidate**
(fits a public documentation page), **not known**.

The tables are generated from the local raw logs (one stage 3 run per
game, 64-bit Windows, 2026-11-21; the maintainer's Cave Crawlers run is
experiment 1, which went into a game). "Recorded by: agent" means an AI
agent on the maintainer's computer under ADR 0044 and 0045. Every game
also asked `CreateInterface` for `SteamClient017` and made no calls
through it.

## Games

| Game | Code | `SteamClient` | Recorded by |
| --- | --- | --- | --- |
| Cave Crawlers (2143030) | CC | `SteamClient020` | maintainer |
| Bongo Cat (3419430) | BC | `SteamClient021` | maintainer |
| Coin Maker Simulator Demo (5191630) | CM | `SteamClient023` | maintainer |
| Baldi's Basics Classic Remastered (1712830) | BB | `SteamClient020` | agent |
| Linsips (4301370) | LS | `SteamClient023` | agent |
| Paper Lily - Chapter 1 (2220360) | PL | `SteamClient020` | agent |
| Please, Touch The Artwork 2 (2600140) | PT | `SteamClient020` | agent |
| An Arcade Full of Cats (2368470) | AC | `SteamClient021` | agent |
| Gravitas (1067310) | GR | `SteamClient018` | agent |
| Outcore: Desktop Adventure (1275670) | OC | `SteamClient020` | agent |
| Plant Therapy (2505120) | PY | `SteamClient020` | agent |
| Chronigma (4051150) | CH | `SteamClient021` | agent |
| The Thief, the Witch, the Toad, and the Mushroom. (2233770) | TW | `SteamClient020` | agent |
| One's Own Exile (3086720) | OE | `SteamClient021` | agent |

## Positions by interface version

"Games" is how many of the games above called the interface at all.
Positions are listed in the order of their numbers.

| Interface version | Games | Positions called | Status |
| --- | --- | --- | --- |
| `STEAMAPPS_INTERFACE_VERSION008` | 7 | 7 (CC, PT, OC); 4 (LS); 0 (AC, GR); 6 7 (PY) | not known |
| `SteamClient018` | 1 | 0 1 2 3 4 5 6 7 9 12 23 34 | not known |
| `SteamClient020` | 7 | 0 1 2 4 5 12 23 34 (CC, OC); 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 22 23 24 27 28 29 30 31 34 35 36 37 38 39 40 (BB, PT); 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 23 24 27 28 29 30 31 34 35 36 37 38 39 40 (PL, PY); 0 2 5 12 34 (TW) | 12 and per-interface positions hand out objects by version text (`objects.md`); rest not known |
| `SteamClient021` | 4 | 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 22 23 24 26 27 28 29 32 33 34 35 36 37 38 (BC, CH); 0 1 2 4 5 12 23 32 (AC, OE) | as for 020 |
| `SteamClient023` | 2 | 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 21 22 23 25 26 27 30 31 32 33 34 35 36 (CM); 0 1 2 4 5 12 22 30 (LS) | as for 020 |
| `SteamController007` | 2 | 2 | not known |
| `SteamController008` | 10 | 2 | not known |
| `SteamFriends017` | 4 | 43 44 (CC); 0 (PL, AC); 3 4 36 37 (PT) | not known |
| `SteamFriends018` | 1 | 6 9 36 | not known |
| `SteamGameServer012` | 1 | 0 | not known |
| `SteamInput001` | 2 | 2 | not known |
| `SteamInput006` | 10 | 3 (BC, CM, LS, PL, PT, AC, PY, TW); 0 3 6 (BB, CH) | not known |
| `STEAMINVENTORY_INTERFACE_V003` | 2 | 1 6 20 21 (CC); 1 2 3 5 6 11 13 26 (BC) | not known |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION014` | 1 | 0 1 13 15 21 22 | 0 `FileWrite`, 1 `FileRead`, 15 `GetFileSize`: confirmed; 13: candidate; rest not known |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION016` | 2 | 13 | not known |
| `SteamUser020` | 3 | 2 12 (CC, OC); 2 (GR) | not known |
| `SteamUser021` | 2 | 2 | not known |
| `SteamUser022` | 1 | 2 | not known |
| `SteamUser023` | 8 | 1 2 (BC, AC, OE); 2 (CM, LS, PT, PY, CH) | not known |
| `STEAMUSERSTATS_INTERFACE_VERSION011` | 2 | 0 4 6 10 12 (CC); 0 14 15 (OC) | 12 `GetAchievementDisplayAttribute`: confirmed; 4, 6: candidates; rest not known |
| `STEAMUSERSTATS_INTERFACE_VERSION012` | 4 | 0 (PT, TW); 0 10 (AC); 0 6 10 14 15 (OE) | not known |
| `STEAMUSERSTATS_INTERFACE_VERSION013` | 3 | 1 3 5 8 9 15 37 (BC); 22 37 39 (CM); 15 (LS) | not known |
| `SteamUtils009` | 3 | 9 14 (CC, OC); 9 16 (GR) | 9 `GetAppID`: candidate; rest not known |
| `SteamUtils010` | 11 | 9 14 (BC, CM, BB, LS, PL, AC, PY, CH); 5 6 9 14 34 (PT); 9 10 14 18 26 34 (TW); 9 23 (OE) | 9 `GetAppID`: confirmed, shadow mode matched in 3 games; rest not known |

Beyond these, the exported entry points (`entry-points.md`) and the
callback numbers seen through `Steam_BGetCallback` are needed by every
game; their meanings are not known yet either.

## What this shows so far

- Every one of the 14 games calls position 9 of `SteamUtils`
  (`GetAppID`, confirmed for `SteamUtils010`) and position 2 of
  `SteamUser` (in four versions).
- Ten or more of the 14 use `SteamUtils010` (positions 9 and 14),
  `SteamController008` (2) and `SteamInput006` (3): the best next
  targets after `GetAppID`.
- These games use 4 to 10 interface versions each; most positions they
  call hand out objects at start.
- A position confirmed for one interface version is not confirmed for
  another (ADR 0042); `SteamUtils009` and `SteamUtils010` are recorded
  separately for that reason.
- Whether a game asks for objects at position 12 or at a position per
  interface depends on the game, not on the `SteamClient` version
  (`objects.md`).
