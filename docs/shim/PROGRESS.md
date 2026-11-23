# Shim progress

How far Reborn is on the way to **milestone M4**: the 20 games of
[`m4-games.md`](m4-games.md) start, play, keep achievements and cloud
saves on a computer where the official Steam client was never
installed. Written for people, not for tools; the detailed records are
linked at the end.

**Last updated:** 2026-11-23 (experiment 34: `RequestPrices` confirmed; the Cave Crawlers inventory group mismatched).

## In one sentence

We know what roughly half of the Steam functions our 20 games use mean,
but no game runs fully without Steam yet: **M4 is about 15 to 20 % done**
(an estimate, see below).

## Where we are

| Area | State | Roughly |
| --- | --- | --- |
| Steam functions whose meaning is confirmed | 141 of the 205 positions our 20 M4 games call (this percentage counts the M4 games only; with the 2 recorded games outside the M4 list, ADR 0056: 151 of 220, 69 %) | **69 %** |
| Of those, answered by the shim itself | connecting to Steam, handing out the interfaces, cleaning up at exit, the app ID, the battery level, accepting the warning hook (`SteamUtils009` 16) | **about 40 %** of all positions |
| Callbacks (messages from Steam to the game) | how they are delivered is confirmed (header, polling, freeing), and the shim has that delivery machinery (a queue per handle, handing over and freeing); no callback content is in the shim yet, so it delivers nothing to games; 3 of 15 recorded message types have a confirmed meaning ("stats received", "stats stored", "call completed") | **about 25 %** |
| Real data from Valve inside the shim (ownership, DLC, achievements, cloud) | ADR 0055 accepted; stage 0 started: `rebornd` answers `AppOwnership` over the shim channel for the game and the DLC its depots name (strict, other apps refused), from the account's licence list loaded at a shim-mode launch; it also answers `RequestStats` for the run's own app (`Accepted`, then the event `StatsReceived` with Valve's result and English achievement names, from `Session::user_stats`, one `ClientGetUserStats` per run, no writes), and the cloud reads `CloudFileSize` and `CloudRead` from the run's cloud folder (`<Reborn data>/cloud/<account>/<app>/`, names checked, no symlinks, quota at most 16 MiB; at a shim-mode launch the folder is filled from Valve's cloud by the L/C/B rules with a base file next to it, a true conflict refuses the launch until the user picks a version (IPC 1.13 `CloudConflict`, GUI question, `reborn-cli daemon launch --cloud`); no writes by the game, no uploads); read-only shadow sessions: `reborn-observe arm --app` registers the game with a signed-in `rebornd`, the proxy claims it and compares `BIsSubscribed` with Reborn's answer (match or mismatch only; any logoff stops it) — first live run matched in 3 of 3 games; it also compares the size, app ID and result of each stats callback 0x44d (ran live in experiment 30: matched), and `GetFileSize` and `FileRead` (size and SHA-1) with a shadow folder `rebornd` fills from Valve once per shadow run (ran live in experiment 33: matched), and the persona name at `SteamFriends017` 0 by digest (ran live in experiment 33: matched); in shim mode the shim now answers `BIsSubscribed` (`STEAMAPPS_INTERFACE_VERSION008` 0) with `rebornd`'s `AppOwnership` for the game itself (maintainer OK; no answer from `rebornd` stops the game); DLC (position 7), stats, stores and cloud writes still to come, and the shim does not send the cloud requests yet, so no game runs on it yet | **about 10 %** (estimate; one ownership call answered by Reborn in the shim, not yet run in a game) |
| Games that run with no Steam installed | not tested on a clean computer yet | **0 of 20** |
| **M4 overall** (estimate) | | **about 15–20 %** |

The percentages for "M4 overall", the shim share and callbacks are
judgement calls: the work left (callbacks, real data from Valve, a clean
test computer) is larger per step than the positions done so far. The
first two rows are counts from [`coverage.md`](coverage.md).

## What we have

- **The observation proxy:** a stand-in `steamclient64.dll` that logs
  what a game asks the official client and what it answers. It can also
  read selected return values (ADR 0043, 0053) and the size and hash of
  callback data (ADR 0050).
- **Shadow mode:** for confirmed functions the proxy computes Reborn's own
  answer next to the real one and logs whether they match (ADR 0046).
- **The autonomous loop:** an AI agent picks the next gap, plans an
  experiment before running anything, runs the games, checks the result
  with a script, has it reviewed by a second agent, and merges (ADR 0051,
  [`LOOP.md`](LOOP.md)).
- **In the shim** (`crates/shim`):
  - `SteamClient020`, `021`, `023`: the connection to Steam, handing out
    the other interfaces, cleaning up when the game closes;
  - `SteamClient018`, `019` (Gravitas, Dark Deception): the same, plus
    the second (game server) pipe the games open and close; nothing is
    handed out over that pipe yet, and positions 7 and 34, which both
    games call early, still stop them;
  - `SteamUtils009`, `010`: the app ID and the battery level;
  - `STEAMAPPS_INTERFACE_VERSION008` position 0 (`BIsSubscribed`): whether
    the account owns the game, asked from `rebornd` per call (ADR 0055;
    the first real account data in the shim); every other position of
    that object, DLC included, still stops the game;
  - callback delivery: `Steam_BGetCallback` and `Steam_FreeLastCallback`
    hand over and free queued callbacks as experiment 19 confirmed;
    nothing fills the queue yet (callback contents must come from
    `rebornd`).
- **Confirmed but not in the shim yet** (they need real data from Valve):
  - whether the game's DLC are owned;
  - cloud saves: write, read, file size;
  - stats: request at start, store at close; achievement display data;
  - reading images, starting controller input, an inventory
    timestamp.

## What is missing, in order

1. **Callbacks.** Many games wait for messages such as "stats are ready"
   or "cloud is done". Without them they stall.
2. **Real data from Valve.** Ownership, DLC, achievements and cloud must
   come from the user's account through `rebornd`. This touches sign-in
   and the account, so the maintainer decides each step.
3. **The other 75 positions**, mostly friends, the user's own data (such
   as the Steam ID) and controllers. Many of them look the same as other
   functions on Valve's pages or involve account data, so they go slower.
4. **A real test** on a computer (a virtual machine) where Steam was never
   installed.

## Details

- [`digest.md`](digest.md): one line per loop iteration, newest first.
- [`gaps.md`](gaps.md): the open positions, in the order the loop takes
  them.
- [`coverage.md`](coverage.md): every recorded position per game.
- [`meanings.md`](meanings.md): every candidate, experiment and result.
