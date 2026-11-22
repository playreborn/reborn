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
experiment 1, which went into a game; the games recorded from 2026-11-22 on ran
with `GetAppID` answered by Reborn, ADR 0046). "Recorded by: agent" means an AI
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
| Fallout Shelter (588430) | FS | `SteamClient021` | agent |
| HoloCure - Save the Fans! (2420510) | HC | `SteamClient020` | agent |
| Dark Deception (332950) | DD | `SteamClient019` | agent |
| UNBEATABLE [white label] (1290490) | UB | `SteamClient020` | agent |
| A Park Full of Cats (2610090) | PK | `SteamClient021` | agent |
| Cartapli : Fold Quest (4314560) | CA | `SteamClient023` | agent |

## Positions by interface version

"Games" is how many of the games above called the interface at all.
Positions are listed in the order of their numbers.

| Interface version | Games | Positions called | Status |
| --- | --- | --- | --- |
| `STEAMAPPS_INTERFACE_VERSION008` | 10 | 7 (CC, PT, OC); 4 (LS, FS); 0 (AC, GR, DD); 6 7 (PY); 0 7 (PK) | confirmed: 0 BIsSubscribed, 7 BIsDlcInstalled or BIsSubscribedApp; candidate: 4 GetCurrentGameLanguage or GetAvailableGameLanguages, 6 BIsSubscribedApp or BIsAppInstalled |
| `SteamClient018` | 1 | 0 1 2 3 4 5 6 7 9 12 23 34 | not known |
| `SteamClient019` | 1 | 0 1 2 3 4 5 6 7 9 12 23 26 34 38 | not known |
| `SteamClient020` | 9 | 0 1 2 4 5 12 23 34 (CC, OC, HC); 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 22 23 24 27 28 29 30 31 34 35 36 37 38 39 40 (BB, PT); 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 23 24 27 28 29 30 31 34 35 36 37 38 39 40 (PL, PY); 0 2 5 12 34 (TW); 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 22 23 24 26 27 28 29 30 31 34 35 36 37 38 39 40 (UB) | confirmed: 0 CreateSteamPipe, 1 BReleaseSteamPipe, 2 ConnectToGlobalUser, 4 ReleaseUser, 5 GetISteam… (per interface), 8 GetISteam… (per interface), 9 GetISteamUtils, 10 GetISteam… (per interface), 11 GetISteam… (per interface), 12 GetISteamGenericInterface, 13 GetISteam… (per interface), 15 GetISteam… (per interface), 16 GetISteam… (per interface), 17 GetISteam… (per interface), 18 GetISteam… (per interface), 19 GetISteam… (per interface), 23 BShutdownIfAllPipesClosed, 24 GetISteam… (per interface), 26 GetISteam… (per interface), 27 GetISteam… (per interface), 28 GetISteam… (per interface), 29 GetISteam… (per interface), 30 GetISteam… (per interface), 31 GetISteam… (per interface), 35 GetISteam… (per interface), 36 GetISteam… (per interface), 37 GetISteam… (per interface), 38 GetISteam… (per interface), 39 GetISteam… (per interface), 40 GetISteam… (per interface); candidate: 34 SetWarningMessageHook |
| `SteamClient021` | 6 | 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 19 22 23 24 26 27 28 29 32 33 34 35 36 37 38 (BC, CH, FS); 0 1 2 4 5 12 23 32 (AC, OE, PK) | confirmed: 0 CreateSteamPipe, 1 BReleaseSteamPipe, 2 ConnectToGlobalUser, 4 ReleaseUser, 5 GetISteam… (per interface), 8 GetISteam… (per interface), 9 GetISteamUtils, 10 GetISteam… (per interface), 11 GetISteam… (per interface), 12 GetISteamGenericInterface, 13 GetISteam… (per interface), 15 GetISteam… (per interface), 16 GetISteam… (per interface), 17 GetISteam… (per interface), 18 GetISteam… (per interface), 19 GetISteam… (per interface), 23 BShutdownIfAllPipesClosed, 24 GetISteam… (per interface), 26 GetISteam… (per interface), 27 GetISteam… (per interface), 28 GetISteam… (per interface), 29 GetISteam… (per interface), 33 GetISteam… (per interface), 34 GetISteam… (per interface), 35 GetISteam… (per interface), 36 GetISteam… (per interface), 37 GetISteam… (per interface), 38 GetISteam… (per interface); candidate: 32 SetWarningMessageHook |
| `SteamClient023` | 3 | 0 1 2 4 5 8 9 10 11 12 13 15 16 17 18 21 22 23 25 26 27 30 31 32 33 34 35 36 (CM, CA); 0 1 2 4 5 12 22 30 (LS) | confirmed: 0 CreateSteamPipe, 1 BReleaseSteamPipe, 2 ConnectToGlobalUser, 4 ReleaseUser, 5 GetISteam… (per interface), 8 GetISteam… (per interface), 9 GetISteamUtils, 10 GetISteam… (per interface), 11 GetISteam… (per interface), 12 GetISteamGenericInterface, 13 GetISteam… (per interface), 15 GetISteam… (per interface), 16 GetISteam… (per interface), 17 GetISteam… (per interface), 18 GetISteam… (per interface), 22 BShutdownIfAllPipesClosed, 23 GetISteam… (per interface), 25 GetISteam… (per interface), 26 GetISteam… (per interface), 27 GetISteam… (per interface), 31 GetISteam… (per interface), 32 GetISteam… (per interface), 33 GetISteam… (per interface), 34 GetISteam… (per interface), 35 GetISteam… (per interface), 36 GetISteam… (per interface); candidate: 30 SetWarningMessageHook |
| `SteamController007` | 4 | 2 | candidate: 2 RunFrame |
| `SteamController008` | 14 | 2 | candidate: 2 RunFrame |
| `SteamFriends017` | 8 | 43 44 (CC); 0 (PL, AC, DD, UB, PK); 3 4 36 37 (PT); 3 4 (FS) | candidate: 0 GetPersonaName |
| `SteamFriends018` | 1 | 6 9 36 | not known |
| `SteamGameServer012` | 2 | 0 (GR); 0 7 (DD) | not known |
| `SteamInput001` | 4 | 2 | candidate: 2 shape only: per-frame call like RunFrame |
| `SteamInput006` | 14 | 3 (BC, CM, LS, PL, PT, AC, PY, TW, FS, PK, CA); 0 3 6 (BB, CH, HC) | candidate: 0 Init, 3 RunFrame, 6 GetConnectedControllers |
| `STEAMINVENTORY_INTERFACE_V003` | 2 | 1 6 20 21 (CC); 1 2 3 5 6 11 13 26 (BC) | not known |
| `SteamMatchMakingServers002` | 1 | 10 | not known |
| `SteamNetworking005` | 1 | 7 | not known |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION014` | 1 | 0 1 13 15 21 22 | confirmed: 0 FileWrite, 1 FileRead, 15 GetFileSize; candidate: 13 FileExists, FilePersisted or FileForget |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION016` | 3 | 13 (BC, LS); 22 (CA) | not known |
| `STEAMSCREENSHOTS_INTERFACE_VERSION003` | 1 | 3 | not known |
| `STEAMUGC_INTERFACE_VERSION016` | 1 | 67 | not known |
| `SteamUser020` | 5 | 2 12 (CC, OC); 2 (GR, DD, UB) | candidate: 2 GetSteamID |
| `SteamUser021` | 3 | 2 (BB, PL); 1 2 (HC) | candidate: 2 GetSteamID |
| `SteamUser022` | 1 | 2 | candidate: 2 GetSteamID |
| `SteamUser023` | 11 | 1 2 (BC, AC, OE, FS, PK); 2 (CM, LS, PT, PY, CH, CA) | candidate: 1 BLoggedOn (or another no-argument bool), 2 GetSteamID |
| `STEAMUSERSTATS_INTERFACE_VERSION011` | 3 | 0 4 6 10 12 (CC); 0 14 15 (OC); 0 (DD) | candidate: 0 RequestCurrentStats or StoreStats, 4 SetStat, 6 GetAchievement, GetStat or GetGlobalStat; confirmed: 12 GetAchievementDisplayAttribute |
| `STEAMUSERSTATS_INTERFACE_VERSION012` | 7 | 0 (PT, TW, HC); 0 10 (AC, PK); 0 6 10 14 15 (OE); 0 2 6 23 29 (FS) | candidate: 0 RequestCurrentStats or StoreStats (start: RequestCurrentStats fits), 10 StoreStats or RequestCurrentStats (close: StoreStats fits) |
| `STEAMUSERSTATS_INTERFACE_VERSION013` | 4 | 1 3 5 8 9 15 37 (BC); 22 37 39 (CM); 15 (LS); 5 15 (CA) | candidate: 15 RequestUserStats |
| `SteamUtils009` | 5 | 9 14 (CC, OC, UB); 9 16 (GR); 9 14 16 (DD) | candidate: 9 GetAppID, 14 GetCurrentBatteryPower |
| `SteamUtils010` | 15 | 9 14 (BC, CM, BB, LS, PL, AC, PY, CH, PK, CA); 5 6 9 14 34 (PT); 9 10 14 18 26 34 (TW); 9 23 (OE); 4 9 14 17 (FS); 9 14 34 (HC) | confirmed: 5 GetImageSize, 6 GetImageRGBA, 9 GetAppID, 14 GetCurrentBatteryPower |

Beyond these, the exported entry points (`entry-points.md`) and the
callback numbers seen through `Steam_BGetCallback` are needed by every
game; their meanings are not known yet either.

## What this shows so far

- Every one of the 20 games calls position 9 of `SteamUtils`
  (`GetAppID`, confirmed for `SteamUtils010`) and position 2 of
  `SteamUser` (in four versions).
- Twelve or more of the 20 use `SteamUtils010` (positions 9 and 14),
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
