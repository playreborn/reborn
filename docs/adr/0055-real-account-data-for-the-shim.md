# 0055: Real account data for the shim

- **Status:** Proposed — waiting for the maintainer
- **Date:** 2026-11-22
- **Touches:** red lines 3, 4 and 8, AGENTS.md section 5 (cloud), the
  public API of `reborn-cm` and `reborn-shim-protocol`. Under ADR 0051
  the agent cannot accept it; the maintainer decides.

## Context

The shim answers a game only for confirmed meanings (ADR 0042, 0046,
0051). Several confirmed meanings need data that only Valve's servers
have, and so far the shim has none of it (`docs/shim/PROGRESS.md`, "Real
data from Valve inside the shim: 0 %"). What is confirmed today
(`docs/shim/meanings.md`):

| Interface and position | Meaning | Status |
| --- | --- | --- |
| `STEAMAPPS_INTERFACE_VERSION008` 0 | `BIsSubscribed` | confirmed (experiment 8, maintainer OK) |
| `STEAMAPPS_INTERFACE_VERSION008` 7 | `BIsDlcInstalled` **or** `BIsSubscribedApp` | narrowed to two; the same answer only for a DLC that is not owned, or owned and installed |
| `STEAMAPPS_INTERFACE_VERSION008` 6 | `BIsSubscribedApp` or `BIsAppInstalled` | candidate |
| `STEAMUSERSTATS_INTERFACE_VERSION011` 0, `…012` 0 | `RequestCurrentStats` | confirmed (experiments 13, 21) |
| `STEAMUSERSTATS_INTERFACE_VERSION011` 10, `…013` 9 | `StoreStats` | confirmed (experiment 23); `…012` 10 narrowed to `StoreStats` or `ResetAllStats` |
| `STEAMUSERSTATS_INTERFACE_VERSION011` 12 | `GetAchievementDisplayAttribute` | confirmed (experiment 2), only the key `name` recorded |
| `…012` 6, `…013` 5 | read a value by name (`GetAchievement`, `GetStat`, …) | candidates |
| `…011` 4 | set a value by name (`SetStat`, `SetAchievement`, …) | candidates |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION014` 0, 1, 15 | `FileWrite`, `FileRead`, `GetFileSize` | confirmed (experiment 1) |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION014` 13, `…016` 13 | `FileExists`, `FilePersisted`, `FileForget` | candidates |
| callback 0x44d (1101) | `UserStatsReceived_t`: app ID 0..4, `EResult` 8..12, a Steam ID 12..20, size 24 | confirmed (experiment 21); bytes 12..20 still to be compared with the account's own Steam ID (review finding) |
| callback 0x44e (1102) | `UserStatsStored_t`: app ID 0..4, 4..8 zero, `EResult` 8..12, size 16 | confirmed (experiment 23) |
| callback 703 | `SteamAPICallCompleted_t`; `Steam_GetAPICallResult` | confirmed (experiment 22) |
| `Steam_BGetCallback`, `Steam_FreeLastCallback` | callback delivery from a queue | confirmed (experiment 19); queue implemented in PR #24 (open), filled by nothing yet |

What Reborn's own client code (`crates/cm`, `crates/core`, `crates/content`)
already does, and what it does not:

| Need | Exists | Where |
| --- | --- | --- |
| The account's licences and owned apps (incl. DLC, app type `dlc`) | yes | `crates/cm/src/library.rs` `Session::owned_apps` (licence list → packages → apps via PICS, expired licences dropped); `crates/cm/src/pics.rs`; `docs/protocol/licenses-pics.md` |
| Which depots of an app belong to a DLC | yes | `crates/cm/src/apps.rs` `Session::app_details`, `DepotInfo` (`dlcappid`) |
| The account's Steam ID | yes | `crates/cm/src/session.rs` `Session::steam_id` |
| Stats schema, stat values and unlock times (`ClientGetUserStats`, EMsg 818/819) | **partly**: the request exists, but only an achievement list leaves the crate; stat values, `crc_stats` and the raw schema stay private | `crates/cm/src/achievements.rs` (`user_stats_response`, `achievements_from`); `docs/protocol/achievements.md`; fixture `user_stats.bin` |
| Storing stats and achievements | **no**. `steam-vent-proto-steam` 0.5.2 has generated types `CMsgClientStoreUserStats2` and `CMsgClientStoreUserStatsResponse`, but no allowed reference for the flow is cited in `docs/protocol/`, there is no fixture, and `achievements.rs` says "Reborn never writes stats" | — |
| Listing an app's cloud files | yes | `crates/cm/src/cloud.rs` `Session::cloud_files` (`Cloud.EnumerateUserFiles`, paging capped at 20 pages) |
| Downloading a cloud file (SHA-1 checked) | yes | `crates/content` `cloud::download_cloud_file_into`; `crates/core/src/cloud.rs` `restore_missing_saves`, run before each game start (ADR 0035) |
| Mapping files a game writes through the API (names without a root token, such as Cave Crawlers' `MainPlayerData.json`) to a local place | **no**: `crates/core/src/cloud.rs` reports them as not mapped | `docs/protocol/cloud.md`, "Auto-Cloud paths" |
| Uploading or deleting cloud files | **no**: `docs/protocol/cloud.md` "Upload and delete": no allowed reference, upload body format unknown, "not used until it is recorded and confirmed" | — |
| Telling Valve a game is running (`ClientGamesPlayed`) | **no** | — whether Valve accepts stats from a client that has not said so is **unknown** |
| Licence changes during a session | **no**: the licence list is read once after logon | `crates/cm/src/session.rs` `receive_initial` |
| The shim channel | yes, `Hello` only | `crates/shim-protocol/src/lib.rs`, `apps/rebornd/src/shim_server.rs` (line limit 64 KiB), `crates/shim/src/session.rs` |
| Reconnecting after a lost connection, with backoff | yes | `apps/rebornd/src/daemon.rs` (`connection_lost`, `reconnect_delay`, ADR 0025) |

ADR 0046 forbids answer mode **in the observation proxy** for ownership,
DLC, tickets, stats, achievements and cloud writes, because the proxy
must never invent such answers. ADR 0047 already says how the shim gets
them instead: "asked per call", answered by `rebornd` from Valve's
servers through its own client code. What it leaves open is the content
of those messages, what may be cached, and the rules around them.

## Problem

How does the shim answer the confirmed ownership, stats and cloud
meanings, and deliver their callbacks, with data that really comes from
the user's account through `rebornd`, so that

- ownership is never invented (red line 3),
- stats, achievements and cloud files are written only because the game
  asked (red line 4, AGENTS.md section 5),
- Valve's servers see no more requests than the official client would
  cause (red line 8),
- a cloud conflict never overwrites anything (section 5),
- and every call the shim cannot answer honestly still stops the game
  instead of guessing?

## Alternatives considered

1. **The shim talks to Valve itself.** It would need tokens inside the
   game process and the CM client in an MIT/Apache crate. Rejected by
   ADR 0047 (tokens stay in the daemon, the shim stays small) and by the
   licence split (`cm` is fine, but `core` and the keyring are GPL).
2. **Snapshot at launch.** `rebornd` fetches ownership, stats and cloud
   files before the game starts and hands everything to the shim in
   `Hello`. Few messages, but it fetches stats for games that never ask
   (more traffic than the official client), puts a lot of account data
   into the game process up front, and cannot carry writes.
3. **Per-call requests, daemon-side cache per run, typed callbacks
   pushed as events.** The shim sends one request per meaning; `rebornd`
   answers from what it fetched from Valve for this run, fetching only on
   the game's first request; asynchronous results come back as events
   that the shim turns into callbacks. More messages, but each one maps
   to one confirmed meaning and one Valve request at most.
4. **Shadow-mode answers from the official client's files** (its
   `userdata` folder, its stats cache). Rejected: Reborn's answers must
   come from Reborn's own data (ADR 0046), and those files are the
   official client's live state (ADR 0038).

## Decision

Option 3, rolled out in stages (below). Everything here is a proposal
until the maintainer accepts it.

### Principles

- **Source:** every answer about the account comes from a Valve
  response that `rebornd` received in the current daemon session through
  `reborn-cm`, or from a file Reborn downloaded from Valve and
  SHA-1-checked. Nothing is ever filled in from defaults, from the
  official client's files or from memory.
- **Only confirmed meanings, only confirmed bytes.** A field of a
  callback is written only if `meanings.md` confirms it; bytes whose
  meaning is unknown are written as recorded (for example 4..8 of
  0x44e is zero) or the case stops the game.
- **Honest failure.** When Reborn cannot get the real answer, the shim
  never makes one up. Where no recorded behaviour says what the official
  client does in that case, the shim stops the game with a log line
  (`not_implemented`), as it does today.
- **The game decides writes.** `rebornd` writes stats only after a
  `StoreStats` from the game, and cloud files only after a `FileWrite`
  from the game. Nothing is written at launch, at exit or in the
  background unless the game's own call asked for it in this run.

### Protocol messages (`crates/shim-protocol`, MIT OR Apache-2.0)

The crate keeps depending on nothing GPL (`serde`, `serde_json`,
`thiserror` only). The protocol goes to version 0.2 (messages only
added). `DaemonMessage` gets an `Event` variant without `id` (ADR 0047
"events without one"). Names below are the proposal; the exact Rust
names are settled in the implementation PR.

Requests (shim → daemon), each answered by a `Response` with the same
`id`:

| Request | Answer | Backed by |
| --- | --- | --- |
| `AppOwnership { app_id }` | `Ownership { owned, installed }` | licence list (`owned_apps`); `installed` from the game's install record and, for a DLC, whether its depots (`dlcappid`) are installed or it has none |
| `RequestStats` | `Accepted`, later the event `StatsReceived` | `ClientGetUserStats` |
| `StoreStats { changes }` | `Accepted`, later the event `StatsStored` | store request (does not exist yet, below) |
| `CloudFileSize { name }` | `FileSize { size: Option<u32> }` | the run's cloud folder |
| `CloudRead { name, max }` | `FileData { data }` (base64) | the run's cloud folder |
| `CloudWrite { name, data }` | `Written { ok }` | the run's cloud folder |

Events (daemon → shim):

- `StatsReceived { result, achievement_names }`: the `EResult` from
  Valve, and for `GetAchievementDisplayAttribute("name")` the display
  name of each achievement in the language below. Stat values and unlock
  state are **not** sent while the readers (`…012` 6, `…013` 5) are
  candidates; they are added by the amendment that confirms one.
- `StatsStored { result }`: the `EResult` of the store.

Every request can also be answered with `Unavailable { reason }`:
`NotSignedIn`, `Offline`, `NotAllowed` (not this session's business),
`TooLarge`.

The shim, not the daemon, turns an event into callback bytes, because
the byte layout is shim knowledge with provenance in `docs/shim/`
(`UserStatsReceived_t` and `UserStatsStored_t` above). It puts the
callback into the PR #24 queue under the handle (`rcx`) of the pipe on
which the game got the `SteamUserStats` object; while it is a candidate
whether `rcx` is the pipe or the user handle, a game that polls on more
than one handle with a stats request open stops with a log line.

`AppOwnership` is answered for any app ID: the `ISteamApps` page
documents `BIsSubscribedApp` as taking any app ID (`meanings.md`,
position 7), and the recorded games asked only about their own DLC. The
alternative, `NotAllowed` for app IDs that are not this game or its DLC,
needs a sourced PICS field for "DLC of", which `reborn-cm` does not read
yet (maintainer decision). Every request is still checked
against the session of ADR 0047: one launched process, one app.

### How the shim answers each confirmed meaning

| Call | Shim answer |
| --- | --- |
| `BIsSubscribed` (`…008` 0) | `AppOwnership` for the session's app; `owned` |
| `…008` 7 (`BIsDlcInstalled` or `BIsSubscribedApp`) | `AppOwnership`; if `owned == false`, false; if owned **and** installed, true (both candidates agree); if owned but not installed, **stop** (the two candidates disagree) |
| `…008` 6 | **stop** (candidate) |
| `RequestCurrentStats` | sends `RequestStats`, returns true as recorded (result 1 in eleven games); the callback 0x44d follows when the event arrives. Returns true only after the daemon's `Accepted`; `Unavailable` → see "Offline" |
| `GetAchievementDisplayAttribute(name, "name")` | from `achievement_names` of the last `StatsReceived`, a NUL-terminated text the shim keeps alive for the run; other keys, unknown names or no stats yet → **stop** |
| `StoreStats` (`…011` 10, `…013` 9) | sends `StoreStats` with the changes the game made; returns true after `Accepted`; callback 0x44e when `StatsStored` arrives. Because every setter is still a candidate (and stops the game), `changes` is empty until a setter is confirmed (see below) |
| `…012` 10 (`StoreStats` or `ResetAllStats`) | **stop** until it is told apart: a reset is a write the game might not mean, and is never guessed |
| `GetFileSize` (`…014` 15) | `CloudFileSize`; a missing file is answered only once the official client's answer for a missing file is recorded, until then **stop** |
| `FileRead` (`…014` 1) | `CloudRead`; returns the number of bytes copied into the game's buffer, at most the buffer size the game passed |
| `FileWrite` (`…014` 0) | `CloudWrite`; returns true when the file is written in the run's cloud folder |
| `…014` 13, `…016` 13, readers, setters, `RequestUserStats`, tickets, language | **stop** (candidates, or not decided here) |

**An empty `StoreStats`.** With no confirmed setter, a game that reaches
`StoreStats` has changed nothing through the shim. The proposal: the
daemon sends nothing to Valve and the shim delivers 0x44e with
`EResult` 1 (OK). That is a local answer, not account data, but it says
"stored" without asking Valve, so the maintainer decides it (decision 4).
The other choice is to stop the game at `StoreStats` until a setter and
the store request exist.

**Real stats writes** need, before any code: a confirmed setter
(experiment with the bytes at `r8`, ADR 0043/0053), an allowed reference
for the store message flow cited in `docs/protocol/achievements.md`, a
recorded and redacted fixture of a store from the secondary test
account, and an answer to whether Valve needs `ClientGamesPlayed` first.
These are separate PRs under this ADR; the line "Reborn never writes
stats" in `achievements.rs` and ADR 0029 changes to "only when the game
asks (red line 4)".

### What is cached, and where

- **In `rebornd`, in memory, per shim run** (keyed by the launched
  process ID, dropped when the game exits):
  - the set of owned app IDs and the installed DLC, derived once per run
    from the licence list of the current Valve session and the install
    records;
  - the stats response of `ClientGetUserStats` for this app (schema,
    values, `crc_stats`), fetched on the game's first `RequestStats`;
  - the open stats changes, until they are stored.

  Nothing of it is written to disk or SQLite, and nothing of it is
  logged (stats and achievement names are account data; logs say only
  sizes, counts and `EResult`).
- **In the shim**, in memory: the session (ADR 0047), the last
  `achievement_names`, and the texts handed to the game (they must stay
  valid). No ownership is cached in the shim; every ownership call asks
  the daemon, which answers from its per-run set without contacting
  Valve.
- **On disk, the run's cloud folder:** `<Reborn data>/cloud/<account
  ID>/<app ID>/` holds the files a game reads and writes through the
  API, separate per account (the official client keeps accounts apart
  the same way, `docs/protocol/cloud.md`). Next to it a small state file
  records for each file the SHA-1, size and timestamp last downloaded
  from Valve. These are the user's saves, not secrets. File names from
  the game and from Valve are checked like install paths (no `..`, no
  absolute paths, no root tokens, no symlinks followed) before use
  (section 5).

### Cloud

- **At launch** (extends ADR 0035): `rebornd` lists the app's files
  (`cloud_files`) and downloads those missing in the run's cloud folder,
  SHA-1-checked, as `restore_missing_saves` does for Auto-Cloud files.
  A file that differs locally and in the cloud is **not** overwritten;
  the game is started with the local file and the frontend reports the
  conflict, as ADR 0035 does today.
- **During the run:** `FileRead`, `GetFileSize` and `FileWrite` work on
  the run's cloud folder only. A write goes to a temporary file first
  and replaces the old file only when complete. The size limit is the
  app's `ufs` `quota` from its app info (Cave Crawlers: 1 000 000), at
  most 16 MiB; the shim socket's line limit for a session grows to fit
  it. A larger write gets `TooLarge` and stops the game.
- **Upload:** none, until the upload protocol is backed by an allowed
  source and a fixture (`docs/protocol/cloud.md`) and the maintainer
  accepts a separate ADR. Until then saves written through the shim stay
  on this computer, and the frontend says so before a shim-mode launch.
  When uploads come, the rule is fixed here already: before uploading,
  `rebornd` lists the cloud again; if a file changed in the cloud since
  it was downloaded (other SHA-1 or timestamp), nothing is uploaded and
  the user decides (section 5).

### Rate limits (red line 8)

- Ownership: no Valve request per call; one licence list per daemon
  session, as today.
- Stats: at most one `ClientGetUserStats` per app per run; a repeated
  `RequestCurrentStats` is answered from the run's cache (the recorded
  games call it once). Store requests (when they exist) are merged: at
  most one in flight, at most one every 10 seconds per app, the changes
  of several `StoreStats` calls combined (Bongo Cat called it four times
  within 2 ms); each `StoreStats` still gets its own 0x44e.
- Cloud: one listing and the needed downloads at launch, nothing per
  call.
- Failures back off exponentially (from 1 s, doubling, at most 5 min)
  and give up after 5 tries per request; the reconnect logic of ADR
  0025 stays the only place that signs in again.
- Tests never contact Valve: the daemon side is tested with recorded,
  redacted fixtures (`user_stats.bin`, `cloud_files.bin`, new ones for
  each new message), the shim side with recorded shim conversations
  (ADR 0047).

### Offline, not signed in, daemon gone

- **Not signed in at launch:** a shim-mode launch is refused and the
  frontend asks the user to sign in. The shim is not started without a
  Valve session, so it can never answer ownership without Valve.
- **Connection lost during the run:** ownership is answered from the
  run's set (it came from Valve in this daemon session); cloud reads and
  writes keep working on the local folder. A `RequestStats` that has not
  reached Valve yet waits while `rebornd` reconnects (ADR 0025). The
  shim never delivers a 0x44d with an invented OK; delivering a failure
  `EResult` instead of waiting is decided after an observation of what
  a game does with one.
- **`rebornd` not running or not reachable at `Hello`:** no session, so
  `CreateInterface` returns nothing (ADR 0047, unchanged).
- **`rebornd` stops during the run:** the shim tries to reconnect once;
  a call that needs the daemon after that stops the game with a log
  line, because the launch record that ties the process to its app is
  gone.

### Staged rollout

1. **Stage 0, groundwork (no game affected):** the protocol messages
   with round-trip tests; in `reborn-cm` a public read of the raw stats
   response (an API change this ADR covers); the run's cloud folder and
   its path checks in `reborn-core`; daemon handlers tested against
   fixtures. The experiment comparing bytes 12..20 of 0x44d with the
   account's own Steam ID (match or mismatch only) runs before the shim
   writes that field.
2. **Stage 1, shadow mode (ADR 0046):** the observation proxy, with the
   official client running and `rebornd` signed in to the same account,
   asks `rebornd` for Reborn's answer next to the official one, for
   `BIsSubscribed`, position 7, `GetFileSize`, `FileRead` (compared by
   size and SHA-1 only), `RequestCurrentStats` (compared by 0x44d's size
   and a hash of its confirmed fields) and `GetAchievementDisplayAttribute`.
   The log says only `shadow match` or `shadow mismatch` (account data).
   `reborn-observe arm --app <id>` registers the process with `rebornd`
   as a read-only shadow session. Writes are **never** run in shadow
   mode: the official client already writes, and a second write would
   be one the game did not ask Reborn for.
3. **Stage 2, answer mode in the shim, reads first:** per call, after
   shadow mode matched in at least three games and the maintainer gave
   the OK (ADR 0046). This happens in shim-mode launches (ADR 0048,
   official client closed), never in the proxy; ADR 0046's ban for the
   proxy stays. Order: ownership, then stats reads and 0x44d, then cloud
   reads.
4. **Stage 3, writes:** cloud writes to the local folder, then (decision
   4) the empty `StoreStats`, then real stats stores once their
   prerequisites exist. Each step: three games, maintainer OK.
5. **Later, separate ADR:** cloud upload.

Games for the three-game rule, from `docs/shim/m4-games.md`:

- Cave Crawlers (2143030): stats `…011`, `StoreStats`,
  `GetAchievementDisplayAttribute`, cloud files through the API, DLC at
  position 7;
- Bongo Cat (3419430): stats `…013`, `StoreStats` five times;
- Plant Therapy (2505120) or A Park Full of Cats (2610090): DLC
  checks at position 7; An Arcade Full of Cats (2368470), Dark Deception
  (332950) or A Park Full of Cats for `BIsSubscribed`.

All runs use the secondary test account (ADR 0044), which owns these
free games and no DLC; telling the two candidates at position 7 apart
needs a DLC that is owned, which the account does not have.

## Consequences

- The shim gets its first real account data without ever holding a
  token, and every answer can be traced to one Valve response or one
  checked file.
- Several confirmed meanings still stop the game: `…008` 6, owned but
  not installed DLC at position 7, every stats reader and setter,
  `…012` 10, `FileExists`-like calls, and missing files at `GetFileSize`.
  Games on the M4 list that use them keep stopping until each is
  confirmed. That is intended.
- Cloud saves written by a game in shim mode stay local until uploads
  are decided. A user who switches back to the official client on
  another computer does not see them; the frontend has to say so
  clearly.
- Stats writes need a store request that `reborn-cm` does not have; they
  are the slowest part and the one with the most account risk (ADR 0012).
- `reborn-cm`, `reborn-shim-protocol` and possibly `reborn-ipc` (a
  conflict and "saves stay local" notice) change their public API; each
  change is covered by this ADR or an amendment.
- Shadow mode needs `rebornd` signed in while the official client runs
  on the same account. Experiments already list the library that way
  (`reborn-cli library --all`, experiment 8); whether a second session
  disturbs the official client's game session is watched in the first
  shadow runs and stops the stage if it does.
- More moving parts in `rebornd`: a per-run state, a cloud folder and a
  stats queue, all with tests against fixtures.
