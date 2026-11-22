# Digest

One line per iteration of the shim loop (`LOOP.md`, ADR 0051), newest
first, so the maintainer can read a day's work quickly and stop anything.

## Waiting for the maintainer

- Nothing.

## Log

- 2026-11-22: loop iteration 3: seven new candidates and one narrowed (UserStats012 0/10,
  UserStats013 15, SteamInput006 0/6, SteamInput001 2, SteamUser023 1,
  SteamFriends017 0). Experiment 12 **mismatched** as scripted: the check
  applied each prediction to all six logs, not only to the games it named;
  nothing confirmed. Also caught and corrected before the runs: candidate
  sources written without re-reading the pages.
- 2026-11-22: loop iteration 2: UserStats 011/012 position 0 recorded as
  candidates (`RequestCurrentStats` or `StoreStats`, no experiment yet);
  experiment 11 matched (4 games): `SteamClient021` 4/1/23 and
  `SteamClient023` 4/1/22 are the close calls; added to the shim.
- 2026-11-22: loop iteration 1: `SteamUtils010` 14 confirmed as
  `GetCurrentBatteryPower` (experiment 10, 3 games, 238 calls, all 255 on
  AC power); `SteamUtils009` 14 stays a candidate (Outcore not run).
  Implemented in the shim and in shadow mode (live: every call matched);
  the review pass found a Linux test abort, fixed before merging. PR #10.
- 2026-11-22: loop set up (ADR 0051): `tools/shim/` (coverage, gap
  list, runs), `LOOP.md`. Gaps: 205 positions recorded, 87 confirmed,
  118 open; first gap `SteamUtils010` position 14 (14 of 20 games).
- 2026-11-22: experiment 9 matched, `SteamUtils010` 5 and 6
  (`GetImageSize`, `GetImageRGBA`), PR #8.
- 2026-11-22: experiment 8 matched, `STEAMAPPS_INTERFACE_VERSION008` 0
  and 7, PR #7.
- 2026-11-22: experiment 7 matched, `SteamClient020/021/023` pipe, user,
  interface and shutdown positions, PR #4.
