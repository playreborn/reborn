# Digest

One line per iteration of the shim loop (`LOOP.md`, ADR 0051), newest
first, so the maintainer can read a day's work quickly and stop anything.

## Waiting for the maintainer

- `GetSteamID` (SteamUser020 to 023 position 2): confirming it needs the
  proxy to read the account's own Steam ID behind the returned address; a
  new ADR, the maintainer decides.
- `BLoggedOn` vs `GetHSteamUser` (SteamUser021/023 position 1): telling
  them apart needs a run with the client offline, which changes the
  sign-in state.
- Implementing `SetRichPresence`, `GetFriendCount`, `GetFriendByIndex` in
  the shim (account and friends data).

## Log

- 2026-11-22: loop iteration 10a (planned by a parallel agent, run by the
  main session): experiments 22 and 23 **matched** (6 games). Callback 703
  is `SteamAPICallCompleted_t`, `Steam_GetAPICallResult` is
  `GetAPICallResult`, UserStats013 15 is `RequestUserStats`; callback 1102
  is `UserStatsStored_t`, UserStats011 10 and 013 9 are `StoreStats`
  (assuming a `bool` arrives as 0 or 1). 703 listed in `PAYLOAD_LISTED`.
  PR #25.
- 2026-11-22: loop iteration 10a (plan, parallel agent): experiments 22
  (703 = `SteamAPICallCompleted_t`) and 23 (0x44e = `UserStatsStored_t`,
  the trigger positions `StoreStats`) planned, not run yet. Correction
  found by the review pass: UserStats012 position 10 goes back from
  confirmed `StoreStats` to "narrowed: `StoreStats` or `ResetAllStats`"
  (experiment 13 did not rule out `ResetAllStats`); a Linsips log breaks
  experiment 19's D and E outside its five games (notes in
  `meanings.md`). PR #25.
- 2026-11-22: loop iteration 9a (planned by a parallel agent, run by the
  main session): experiments 16, 17, 18 **matched** (8 games). Confirmed:
  `RunFrame` (SteamController007/008 2, SteamInput006 3),
  `SetRichPresence`, `GetFriendCount`, `GetFriendByIndex` (SteamFriends017
  43, 3, 4). Narrowed: SteamInput001 2, SteamFriends017 36, SteamUser020
  12. Waiting for the maintainer: see the section above. PR #22.
- 2026-11-22: loop iteration 9b (planned by a parallel agent, run by the
  main session): experiment 19 **matched** (Dark Deception, Gravitas,
  Bongo Cat, Coin Maker, Cave Crawlers): how callbacks are delivered is
  confirmed (header layout, one free per poll, `Steam_GetAPICallResult`
  only inside a 703 callback). 703's content stays open. PR #21.
- 2026-11-22: loop iteration 9: first callback meaning. Experiment 20
  **mismatched** (the Steam ID sits at bytes 12 to 20, not 16 to 24);
  experiment 21 with that layout **matched** (Cave Crawlers, Gravitas,
  Dark Deception): callback 0x44d is `UserStatsReceived_t`,
  UserStats011 0 is `RequestCurrentStats`. ADR 0054 (maintainer): the
  agent lists callback numbers itself. Two parallel agents planned
  experiments 16 to 19 (PRs #21, #22). PR #23.
- 2026-11-22: loop iteration 8: experiment 15 matched (4 games):
  `SteamUtils009` 9 `GetAppID` and 14 `GetCurrentBatteryPower` confirmed;
  added to the shim and to shadow mode (live: 68 of 68 calls matched).
  `run.sh` now finds a game's log by process ID when the game prints no
  `[S_API]` line. PR #18.
- 2026-11-22: loop iteration 7: candidates only, no runs:
  `STEAMREMOTESTORAGE_INTERFACE_VERSION016` 13 (a function taking only a
  file name; nine on the page fit), UserStats012 6 and UserStats013 5
  (read a value by name, or a name-only write). Review found two wrong
  counts and missing candidates before merge; fixed. PR #17.
- 2026-11-22: ADR 0053 (maintainer): the agent adds proxy reads itself
  with a review pass. PR #16.
- 2026-11-22: ADR 0052 (maintainer): no daily start cap, 1 minute between
  starts, 50 free games a day. PR #15.
- 2026-11-22: loop iteration 6: `STEAMINVENTORY_INTERFACE_V003`: the proxy
  reads the handle position 6 writes; experiment 14 matched as scripted
  (Cave Crawlers, Bongo Cat), but the review found other functions with
  the same shape: only 3 `GetResultTimestamp` confirmed; 6 (writes a
  result handle) and 1 stay candidates. PR #16.
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
