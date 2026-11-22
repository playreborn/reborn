# Shim progress

How far Reborn is on the way to **milestone M4**: the 20 games of
[`m4-games.md`](m4-games.md) start, play, keep achievements and cloud
saves on a computer where the official Steam client was never
installed. Written for people, not for tools; the detailed records are
linked at the end.

**Last updated:** 2026-11-22 (after loop iteration 8).

## In one sentence

We know what roughly half of the Steam functions our 20 games use mean,
but no game runs fully without Steam yet: **M4 is about 15 to 20 % done**
(an estimate, see below).

## Where we are

| Area | State | Roughly |
| --- | --- | --- |
| Steam functions whose meaning is confirmed | 100 of the 205 positions our 20 games call | **49 %** |
| Of those, answered by the shim itself | connecting to Steam, handing out the interfaces, cleaning up at exit, the app ID, the battery level | **about 40 %** of all positions |
| Callbacks (messages from Steam to the game) | the proxy records their size and a hash; no message type has a confirmed meaning yet | **about 5 %** |
| Real data from Valve inside the shim (ownership, DLC, achievements, cloud) | some meanings known, nothing connected to `rebornd` yet | **0 %** |
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
  - `SteamUtils009`, `010`: the app ID and the battery level.
- **Confirmed but not in the shim yet** (they need real data from Valve):
  - whether the game and its DLC are owned;
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
3. **The other 105 positions**, mostly friends, the user's own data (such
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
