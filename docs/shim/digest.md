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

- 2026-11-23: experiment 34 (PR #55): Bongo Cat group matched,
  `STEAMINVENTORY_INTERFACE_V003` 26 confirmed as `RequestPrices` (under
  A1, A2, A5); Cave Crawlers group **mismatched** (the first of two 4700
  callbacks without 6's handle), so 6, 21 and the callbacks 4700/4701/4702 stay
  candidates. Inventory 2 (`accountid`) deferred by the agent under ADR
  0060. Now 151 of 220 (M4 alone: 141 of 205, 69 %).
- 2026-11-23: ADR 0060 (maintainer): the agent decides on the
  maintainer's behalf and may end a hung game; ADR 0059 accepted by the
  agent under it (achievement list at launch, name shadow first,
  leaderboard reads later, uploads and other players' scores stay
  stopped).
- 2026-11-23: experiment 33 **matched** (PR #52): first live runs of the
  cloud reads shadow (Cave Crawlers: `GetFileSize` and `FileRead` agree)
  and the persona name shadow (Dark Deception, A Park Full of Cats: in 2 games, 3
  calls match); `SteamFriends017` 0 confirmed as `GetPersonaName` (under A1).
  `rebornd` signed in with the stored login, no prompt; one online-status
  change from `own-status`, accepted by the maintainer (also happened in
  experiment 30). Answering `GetPersonaName` in the shim needs the
  maintainer (account data). Now 150 of 220 (M4 alone: 140 of 205, 68 %).
- 2026-11-23: experiment 32 **matched** (PR #51): nine UserStats positions
  confirmed: 011/012 14 `GetNumAchievements` (under A1 and A3), 011/012 15
  `GetAchievementName`, 012 23 and 013 22 `FindLeaderboard`, 012 29
  `DownloadLeaderboardEntriesForUsers` and 013 30 `UploadLeaderboardScore`
  (under A2), 013 23 `GetLeaderboardName`. Shim answers (LOOP step 7)
  still to come.
- 2026-11-22: the shim accepts `SteamUtils009` 16 `SetWarningMessageHook`
  and never calls the hook (PR #49); notice before the first shim-mode
  launch of a game, ADR 0057 (PR #46).
- 2026-11-22: experiment 31 **matched** (Gravitas, Dark Deception; PR
  #48): `SteamUtils009` 16 confirmed as `SetWarningMessageHook` (under
  A1 and A2) and `SteamGameServer012` 0 as `InitGameServer` (under A3).
  Two new proxy read kinds under ADR 0058 (maintainer, narrowed stack
  read: slot 6 low 32 bits and "slot 7 text yes/no" only; an executable
  check that reads no bytes). `…USERSTATS…013` 37 narrowed from recorded
  call results. Now 140 of 220 positions confirmed (M4 games alone: 132
  of 205, 64 %).
- 2026-11-22: ADR 0055 stage 1, cloud reads shadow (PR #47, parallel
  agent, no live run yet): `GetFileSize` (RemoteStorage014 15) and
  `FileRead` (014 1) compared against a shadow folder that `rebornd`
  fills once per arm (one fill at a time, at most one per minute, at most
  64 MiB). The proxy keeps only size and SHA-1; file names and sizes are
  hidden in the log. Next: a live run in Cave Crawlers.
- 2026-11-22: persona name shadow for `SteamFriends017` 0 (PR #45, ADR
  0056 decision 2): the proxy sends only a SHA-256 of the official text,
  `rebornd` answers match or mismatch from the `CMsgClientAccountInfo`
  Steam sends unasked at logon (login-path change approved by the
  maintainer). Fixture `account_info.bin` recorded on the test account
  (only "Player 1"). Shim protocol 0.3. Next: a live run.
- 2026-11-22: cloud folder filled at a shim-mode launch (PR #44, ADR
  0055): local/cloud/base rules with a base file; a real conflict stops
  the launch until the user picks a version (IPC 1.13 `CloudConflict`,
  GUI question, `reborn-cli daemon launch --cloud`); the local version is
  always kept in a backup folder. No uploads yet.
- 2026-11-22: experiment 30 **matched**: the stats shadow for callback
  0x44d agreed with the official client in Cave Crawlers, Gravitas and
  Dark Deception (size, app ID, result; Steam ID not compared). PR #39.
- 2026-11-22: experiment 30 planned (loop iteration 14a, parallel agent,
  no runs): first live run of the 0x44d stats shadow in Cave Crawlers,
  Gravitas and Dark Deception; prediction: every step-4b 0x44d line gets
  exactly one stats shadow line and each says `shadow match`. Script
  `tools/shim/experiments/030_stats_shadow.py`. Runs need `rebornd`
  signed in with the stored login next to the official client; stop on
  Steam Guard or any logoff/kick. A match confirms size, app ID and
  `EResult`; the Steam ID bytes, achievement names and answer mode stay
  open.
- 2026-11-22: loop iteration 13a results (PR #40): experiment 26 (VPet)
  matched: `STEAMUGC_INTERFACE_VERSION020` 2/4/5/9/12, `SteamUtils010` 13
  and `STEAMREMOTESTORAGE_INTERFACE_VERSION016` 0/19 confirmed; 18, 11, 77
  and `SteamUser023` 12 narrowed. Experiment 27 matched:
  `STEAMSCREENSHOTS_INTERFACE_VERSION003` 3 narrowed to `HookScreenshots`.
  Now 138 of 220 positions confirmed (M4 games alone: 130 of 205).
- 2026-11-22: loop iteration 13a (parallel agent, plan only, no runs):
  survey positions of experiment 25. Experiment 26 (VPet-Simulator, about
  27 minutes, closed from the tray by the maintainer) for
  `STEAMUGC_INTERFACE_VERSION020` 2/4/5/9/12/77, `SteamUtils010` 11/13,
  `SteamUser023` 12 and `STEAMREMOTESTORAGE_INTERFACE_VERSION016` 0/18/19
  (needs four `READS` entries, one under ADR 0056); experiment 27
  (HoloCure, Pixel Puzzles) for `STEAMSCREENSHOTS_INTERFACE_VERSION003` 3.
  No candidate: `STEAMUGC_INTERFACE_VERSION020` 74; no experiment yet:
  `SteamFriends017` 0 (ADR 0056 allows a match-only comparison, needs
  code). VPet-Simulator and Pixel Puzzles added to `games.json` as extra
  entries (ADR 0056): 220 positions, 130 confirmed (M4 games alone 205,
  130).
- 2026-11-22: ADR 0055 stage 0, cloud reads (parallel agent, no runs):
  shim protocol 0.2 gains `CloudFileSize`, `CloudRead`, `FileSize` and
  `FileData` (base64 without a new dependency, `None` for a missing
  file); `reborn_core::run_cloud` reads sizes and bytes from
  `<Reborn data>/cloud/<account ID>/<app ID>/` with name checks (no `..`,
  absolute paths, root tokens) and no symlinks followed, limited by the
  app's `ufs` quota (at most 16 MiB, `TooLarge` above); `rebornd` answers
  only a shim-mode run's own folder, shadow sessions get `NotAllowed`.
  No writes, no base file, nothing fills the folder yet; the shim does
  not send the requests yet.
- 2026-11-22: ADR 0056 (maintainer): the proxy may read call results
  (`SteamUtils010` 13), `rebornd` may compare the persona name (match or
  mismatch only), and recordings of non-M4 games count in coverage.
- 2026-11-22: ADR 0055 stage 1, stats shadow (parallel agent, no runs):
  when `Steam_BGetCallback` hands the game a 0x44d, the proxy reads only
  its size and payload bytes 0..12 and, on the shadow worker thread
  (bounded queue, shared with `BIsSubscribed`), sends `RequestStats` on
  the shadow session and waits up to 30 s for `StatsReceived`; match
  needs size 24, app ID (bytes 0..4) = the app given to `arm`, and the
  same `EResult` (bytes 8..12). Logs only match, mismatch or no answer.
  Not compared: bytes 12..20 (Steam ID, the review's experiment first),
  4..8, 20..24, achievement names (`GetAchievementDisplayAttribute` is a
  separate stage-1 shadow). The shadow link reads lines on its own
  thread so the wait can end. `rebornd` unchanged.
- 2026-11-22: `SteamClient018` and `019` in the shim with the positions
  experiment 24 confirmed (0, 1, 2, 3, 4, 5, 6, 9, 12, 23, 26);
  `CreateLocalUser` writes pipe 2 and returns user 1 as recorded, 23 is
  false while a pipe is open; getters on pipe 2 and positions 7, 34, 38
  stop. No game runs further yet (7 and 34 come first).
- 2026-11-22: shim session follow-up from the review of PR #42 (parallel
  agent, no runs): the socket connect waits at most `ANSWER_TIMEOUT`
  (`ConnectWaitMode::Timeout`) for the first connect and the reconnect;
  the event queue lives outside the link lock, so taking events never
  waits behind an exchange; docs on how long the reconnect can hold the
  lock and that a lost write uses it up.
- 2026-11-22: shim session follow-up from the review of PR #35 (parallel
  agent, no runs): a reader thread per connection routes responses by
  `id` and queues events (`StatsReceived`; no 0x44d yet); every exchange,
  `Hello` included, waits at most 10 s (`ANSWER_TIMEOUT`), then "rebornd
  did not answer in time" and the call stops the game. Timeout, an answer
  for no open request, an early event or a broken line end the connection
  for the run without the reconnect; a lost connection is reconnected
  once (same app checked) and only reads (`AppOwnership`, `RequestStats`)
  are sent again, writes never (red line 4). Fake-transport tests.
- 2026-11-22: answer mode for `BIsSubscribed` in the shim (parallel
  agent, no runs; maintainer OK in PR #34): `STEAMAPPS_INTERFACE_VERSION008`
  object with position 0 answered by `rebornd`'s `AppOwnership` for the
  session's app (1 or 0); `Unavailable` or a lost daemon stops the game.
  The shim keeps its `Hello` connection, exchanges run under one lock with
  their own ids, one reconnect per run. Every other position, 7 included,
  stops. PR #35.
- 2026-11-22: first live shadow run with real account data (ADR 0055):
  `BIsSubscribed` shadow **matched** in Dark Deception, A Park Full of
  Cats and An Arcade Full of Cats; `rebornd` and the official client were
  signed in at once without kicks or Steam Guard. The maintainer allowed
  answer mode for this one call (the game's own ownership); a game as a
  whole qualifies only when all its account-data calls have matched. PR #34.
- 2026-11-22: ADR 0055 stage 0, shadow sessions (parallel agent, no
  runs): shim protocol 0.2 gains `RegisterShadow`, `CancelShadow` and
  `ShadowHello` and `Request::writes`; `reborn-observe arm --app`
  registers the app with a signed-in `rebornd` (never signs in; owned set
  from the cached licence list, no DLC), the proxy's `ShadowHello` claims
  it once for its process, reads only (`AppOwnership` for the own app,
  `RequestStats`), writes refused, dropped when the connection closes and
  for all on any end of the Valve session. Proxy shadow for
  `STEAMAPPS_INTERFACE_VERSION008` 0 (`BIsSubscribed`), asked on a worker
  thread, logs only match or mismatch. Position 7 not shadowed.
- 2026-11-22: ADR 0055 stage 0, stats read (parallel agent, no runs):
  shim protocol 0.2 gains `RequestStats`, `Accepted` and the first event,
  `StatsReceived { result, achievement_names }`; `reborn-cm` has a typed
  `Session::user_stats` (result, `crc_stats`, raw schema, stat values,
  unlock times), replayed against `user_stats.bin`; `rebornd` answers
  only the run's own app, fetches once per run on the first request,
  waits through a reconnect, backs off 1 s doubling for 5 tries, and
  sends no event when it gives up. No writes. The shim does not ask yet.
- 2026-11-22: loop iteration 12a: experiment 25 **mismatched** as
  scripted (Super Jigsaw is 32-bit; Pixel Puzzles' E: `rdx` 0/1 at
  `StoreStats`). VPet-Simulator matched fully and is the third recorded
  `StoreStats` game for ADR 0055. Three free games added and installed.
  Deviation: Pixel Puzzles was ended with a forced kill (ADR 0044 says
  never); VPet stayed running in the tray (maintainer asked to close it).
  PR #29.
- 2026-11-22: experiment 25 planned (parallel agent, no runs): first
  recordings of VPet-Simulator (1920960), Pixel Puzzles Ultimate Jigsaw
  Puzzles (351030) and Super Jigsaw Puzzle: Generations (1036950), a
  survey of their calls and a search for a third `StoreStats` → 0x44e
  game (ADR 0055); no license added, PR on `loop/plan-12a`.
- 2026-11-22: ADR 0055 stage 0, first part (parallel agent, no runs):
  shim protocol 0.2 with `AppOwnership`, `Ownership` and `Unavailable`;
  once shim mode is entered, `rebornd` takes the run's ownership set from
  `Session::licensed_app_ids` (active licences incl. package 0, package
  step only, cached per Valve session; `docs/protocol/licenses-pics.md`),
  refuses the launch without it, binds each connection to its first
  `Hello`, and answers only the game and the DLC its depots name,
  everything else `NotAllowed`. The shim does not ask yet. Stats, cloud,
  events and shadow sessions are later stage-0 work.
- 2026-11-22: loop iteration 11a (planned by a parallel agent, run by the
  main session): experiment 24 **matched** (Gravitas, Dark Deception):
  `SteamClient018`/`019` positions 0, 1, 2, 3 (`CreateLocalUser`, read
  through a new `READS` entry), 4, 5, 6, 9, 12, 23, 26 confirmed; the pipe and user argument
  slots told apart for the first time (both users were 1). PR #27.
- 2026-11-22: loop iteration 11a (planned by a parallel agent, no runs):
  every recorded position of `SteamClient018` (Gravitas) and
  `SteamClient019` (Dark Deception) has a candidate from the
  `ISteamClient` page; the second pipe comes from position 3
  (`CreateLocalUser`, account type 3). Experiment 24 planned (needs a
  READS entry for position 3); games to run: Gravitas, Dark Deception.
- 2026-11-22: callback delivery in the shim (parallel agent, no runs):
  `Steam_BGetCallback` and `Steam_FreeLastCallback` work on a queue per
  handle with the header experiment 19 confirmed (bytes 0..4 and 20..24
  written as recorded, still candidates); nothing fills the queue yet.
  `Steam_GetAPICallResult` and `Steam_ReleaseThreadLocalMemory` still
  stop the process. PR #24.
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
