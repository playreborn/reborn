# Digest

One line per iteration of the shim loop (`LOOP.md`, ADR 0051), newest
first, so the maintainer can read a day's work quickly and stop anything.

## Waiting for the maintainer

- Nothing.

## Log

- 2026-11-22: ADR 0052 (maintainer): no daily start cap, 1 minute between
  starts, 50 free games a day. PR #15.
- 2026-11-22: loop iteration 6: `STEAMINVENTORY_INTERFACE_V003`: the proxy
  reads the handle position 6 writes; experiment 14 **matched** (Cave
  Crawlers, Bongo Cat): 6 `GetAllItems`, 3 `GetResultTimestamp`
  confirmed; 1 stays candidate `GetResultItems`. Not in the shim yet
  (needs the inventory from Valve). PR #16.
- 2026-11-22: loop iteration 5: `SteamClient020`/`021` position 22 and
  `SteamClient023` position 21 (one call early, the same result value in
  all eight games, so probably the same function): **no candidate**, no
  function on the `ISteamClient` page fits. `SteamUtils010` 34: no
  candidate (arguments differ between games). Experiment 13 planned
  (experiment 12 with each prediction scoped to its games).
  Experiment 13 **matched** (6 games): `STEAMUSERSTATS_INTERFACE_VERSION012`
  0 `RequestCurrentStats`, 10 `StoreStats`, `SteamInput006` 0 `Init`
  confirmed; not in the shim yet (needs those objects, stats from Valve
  and callbacks). PR #14.
- 2026-11-22: loop iteration 4: ADR 0050 in the proxy: 24 header bytes
  at `rdx`, size and SHA-256 of the callback data, the first 64 bytes only
  for listed numbers (none listed yet). Live in Cave Crawlers: all 14
  callbacks had a readable size (1 to 784), the same size per number
  (`entry-points.md`).
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
