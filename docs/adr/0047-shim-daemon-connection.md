# 0047: How the shim talks to rebornd

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

The shim (ADR 0012) is the `steamclient` replacement inside the game
process. So far the only answer Reborn gives a game is `GetAppID`
(ADR 0046), from a value handed over at launch. Almost every other
confirmed or candidate meaning needs data only `rebornd` has: who is
signed in, ownership and DLC, stats and achievements, cloud files, and
callbacks. `docs/plan.md` § 6.2 sets the direction ("own, minimal,
low-latency channel"; "the shim keeps as little state as possible; logic
and the Steam connection stay in the daemon") but leaves the details
open. Facts from the recordings that matter here:

- Games poll `Steam_BGetCallback` about every 3 ms
  (`docs/shim/entry-points.md`), and call `RunFrame`-like positions and
  `SteamUtils010` position 14 about once per frame
  (`docs/shim/meanings.md`).
- Games ask for many objects in the first 100 ms after start
  (`docs/shim/objects.md`).
- The frontend channel (ADR 0008) is JSON lines over a per-user named
  pipe or Unix socket, with message types in `reborn-ipc` (GPL).
- The plan puts the shim under MIT OR Apache-2.0. MIT/Apache crates must
  not depend on GPL crates (AGENTS.md § 3), so the shim cannot use
  `reborn-ipc`.

Out of scope here: how the game finds the shim without the official
client (a loading ADR, building on `docs/shim/loading.md`), and 32-bit
games (M5).

## Problem

How does the shim reach `rebornd`, which data does it keep itself, and
how are per-frame calls answered without a round trip each time, while
tokens stay in the daemon and ownership still comes from Valve?

## Alternatives considered

1. **Reuse the frontend socket and `reborn-ipc`.** One protocol, but the
   licence rule forbids it, and frontends and games have different needs
   (events for windows versus per-frame polling).
2. **A separate shim channel with its own small protocol crate.** Same
   transport technique as ADR 0008, a second socket, and message types in
   a new MIT OR Apache-2.0 crate that both the shim and `rebornd` use.
3. **Shared memory.** Lowest latency, but much harder to get right and to
   test, and unnecessary if per-frame calls are answered locally.

## Decision

Option 2.

### Crates

- `crates/shim-protocol` (MIT OR Apache-2.0, published later): the
  message types and framing, nothing else. `rebornd` (GPL) depends on it,
  which the licence rule allows.
- `crates/shim` (MIT OR Apache-2.0, `cdylib`): the shim. It depends on
  `shim-protocol` and never on `cm`, `content`, `core`, `platform` or
  `ipc`.

### Transport

- A second per-user local socket next to the frontend one, made the same
  way (`interprocess`, ADR 0008): `\\.\pipe\reborn-shim-<user>` on
  Windows with the same access list, `$XDG_RUNTIME_DIR/reborn-shim.sock`
  with mode `0600` on Linux.
- The shim uses a blocking client on its own thread; it starts no async
  runtime inside the game process.

### Format

- JSON lines, as in ADR 0008: easy to read, to record as fixtures and to
  replay in tests. Requests carry an `id`; the daemon answers with the
  same `id` or sends events without one.
- The first exchange is `Hello` with the protocol version and the game's
  process ID. A later ADR may switch to a binary format if measurements
  show JSON is too slow; per-frame calls do not cross the socket (below),
  so this is not expected.

### Session

- `rebornd` remembers which app it launched with which process ID. In
  `Hello` the shim sends its process ID; the daemon answers with the
  session: the app ID and the data the shim may cache (next section). A
  process the daemon did not launch gets a refusal, and the shim then
  reports failure to the game in whatever way recordings show the
  official client does; until that is recorded, `CreateInterface`
  returns nothing.
- Every later request is checked against that session's app: a game gets
  answers only for itself, never for other apps or about the account
  beyond what the session allows.

### What the shim keeps, and what it asks for

- **Cached at `Hello`, answered locally:** values that do not change
  during a run and are confirmed for the shim (today: the app ID).
  Further values (for example the signed-in account's ID, once
  `GetSteamID` is confirmed under its own ADR) are added to the session
  message by later decisions, each with its source.
- **Pushed by the daemon:** callbacks. The daemon sends each callback
  once as an event; a reader thread in the shim puts it into a local
  queue, and `Steam_BGetCallback` and `Steam_FreeLastCallback` work on
  that queue without a round trip. What a callback contains is decided
  per callback number once its meaning is confirmed.
- **Asked per call:** everything that needs Valve or changes state
  (ownership checks, tickets, stats reads and writes, cloud reads and
  writes). These go to the daemon as requests; the daemon answers from
  Valve's servers through its own client code. Ownership, licences, DLC
  and tickets always come from Valve (red line 3); stats and achievement
  writes happen only because the game asked (red line 4); cloud writes
  follow AGENTS.md § 5.
- **Per-frame calls without a result** (`RunFrame`-like) never cross the
  socket.

### Security

- The shim never sees tokens or passwords; they stay in the daemon and
  the OS keyring (AGENTS.md § 5).
- Only the user's own processes can connect (socket permissions). The
  daemon trusts a connection only for the app it launched under that
  process ID.
- No telemetry: the channel carries only what the game asks for.

### Tests

- `shim-protocol` gets round-trip tests for every message.
- The shim and the daemon side get replay tests against recorded,
  redacted conversations in `fixtures/` (AGENTS.md § 4), no live servers.
- `docs/plan.md` § 8.4 suggests a test game built with `steamworks-rs`.
  That crate bundles the Steamworks SDK and is excluded by red line 6 and
  ADR 0012; test games for the shim are owned games observed with the
  proxy, as in `docs/shim/`.

## Consequences

- The shim stays small and MIT/Apache; logic, the Steam connection and
  all secrets stay in `rebornd`.
- Per-frame calls stay fast without shared memory, at the cost of a
  reader thread and a queue in the shim.
- Games started without Reborn do not get answers until a later decision
  covers that case.
- Adding a confirmed meaning that needs daemon data means adding a
  message to `shim-protocol`, which changes a public API and so needs an
  ADR or an amendment each time (AGENTS.md § 3); small ones can be
  grouped.
- The JSON format can be revisited by measurement, not by guess.
