# Coverage

Which interface positions games were seen calling, and how far each one
is. Positions come only from recordings (`objects.md`, the raw logs of
the runs named below); meanings only from `meanings.md` (ADR 0042).
The shim implements only confirmed meanings, so this table is also the
shim's to-do list. Nothing is implemented yet.

Status: **confirmed** (experiment matched, maintainer's OK), **candidate**
(fits a public documentation page), **not known**.

## Games and runs

| Game | `SteamClient` | Runs (Windows) |
| --- | --- | --- |
| Cave Crawlers (2143030) | `SteamClient020` | maintainer: stage 3c, step 4b, experiments 1 and 2 (into a game); agent: one run to the menu (2026-11-21) |
| Bongo Cat (3419430) | `SteamClient021` | maintainer: stage 3 (step 4b proxy) |
| Coin Maker Simulator Demo (5191630) | `SteamClient023` | maintainer: stage 3 (step 4b proxy), into a game and back |
| Baldi's Basics Classic Remastered (1712830) | `SteamClient020` | agent: stage 3, to the first screen |
| Linsips (4301370) | `SteamClient023` | agent: stage 3, to the first screen |
| Paper Lily - Chapter 1 (2220360) | `SteamClient020` | agent: stage 3, to the first screen |
| Please, Touch The Artwork 2 (2600140) | `SteamClient020` | agent: stage 3, to the first screen |
| An Arcade Full of Cats (2368470) | `SteamClient021` | agent: stage 3, to the first screen |

All eight also asked `CreateInterface` for `SteamClient017` and made no
calls through it.

## Positions by interface version

Positions are listed in the order of their numbers, not of the calls.
"CC", "BC", "CM", "BB", "LS", "PL", "PT" and "AC" name the games above.

| Interface version | Games | Positions called | Status |
| --- | --- | --- | --- |
| `SteamClient020` | CC, BB, PL, PT | 0 1 2 4 5 12 23 34 (CC); 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 22 23 24 27 28 29 30 31 34 35 36 37 38 39 40 (BB); PL and PT much like BB | 12 and the per-interface positions hand out objects by version text (`objects.md`); rest not known |
| `SteamClient021` | BC, AC | 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 22 23 24 26 27 28 29 32 33 34 35 36 37 38 (BC); 0 1 2 4 5 12 23 32 (AC) | positions that hand out objects by text: see `objects.md`; rest not known |
| `SteamClient023` | CM, LS | 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 21 22 23 25 26 27 30 31 32 33 34 35 36 (CM); 0 1 2 4 5 12 22 30 (LS) | as for 021 |
| `SteamUser020` | CC | 2 12 | not known |
| `SteamUser021` | BB, PL | 2 | not known |
| `SteamUser023` | BC, CM, LS, PT, AC | 1 (BC, AC) 2 | not known |
| `SteamUtils009` | CC | 9 14 | 9 `GetAppID`: candidate (fits, as for 010); 14 not known |
| `SteamUtils010` | BC, CM, BB, LS, PL, PT, AC | 5 6 34 (PT) 9 14 | 9 `GetAppID`: experiment 3 matched, waiting for the maintainer's OK; rest not known |
| `SteamFriends017` | CC, PL, PT, AC | 43 44 (CC); 0 (PL, AC); 3 4 36 37 (PT) | not known |
| `SteamFriends018` | BC | 6 9 36 | not known |
| `SteamInput001` | CC | 2 | not known |
| `SteamInput006` | BC, CM, BB, LS, PL, PT, AC | 0 6 (BB) 3 | not known |
| `SteamController007` | CC | 2 | not known |
| `SteamController008` | BC, CM, BB, LS, PL, PT, AC | 2 | not known |
| `STEAMAPPS_INTERFACE_VERSION008` | CC, LS, PT, AC | 7 (CC, PT); 4 (LS); 0 (AC) | not known |
| `STEAMINVENTORY_INTERFACE_V003` | CC, BC | 1 6 20 21 (CC); 1 2 3 5 6 11 13 26 (BC) | not known |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION014` | CC | 0 1 13 15 21 22 | 0 `FileWrite`, 1 `FileRead`, 15 `GetFileSize`: confirmed; 13: candidate; 21, 22: not known |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION016` | BC, LS | 13 | not known |
| `STEAMUSERSTATS_INTERFACE_VERSION011` | CC | 0 4 6 10 12 | 12 `GetAchievementDisplayAttribute`: confirmed; 4, 6: candidates; 0, 10: not known |
| `STEAMUSERSTATS_INTERFACE_VERSION012` | PT, AC | 0 (PT); 0 10 (AC) | not known |
| `STEAMUSERSTATS_INTERFACE_VERSION013` | BC, CM, LS | 1 3 5 8 9 15 37 (BC); 22 37 39 (CM); 15 (LS) | not known |

Beyond these, the exported entry points (`entry-points.md`) and the
callback numbers seen through `Steam_BGetCallback` are needed by every
game; their meanings are not known yet either.

## What this shows so far

- Even small games use about 10 interface versions and 20 to 40
  positions, most of them for objects they ask for at start.
- Different `SteamClient` versions come with different versions of the
  other interfaces, so a position confirmed for one version is not
  confirmed for the next (ADR 0042).
- Shared between all eight games (five before the agent's second batch): two positions of `SteamUtils` (9 and
  14, in two versions), one of `SteamController` (2), and `SteamUser`
  position 2 (in three versions). Seven of eight use `SteamUtils010`,
  `SteamController008` and `SteamInput006`: good first targets.
- Whether a game asks for objects at position 12 or at a position per
  interface depends on the game, not on the `SteamClient` version
  (`objects.md`).
