# 0055: Real account data for the shim

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22
- **Amended by:** ADR 0059 (when the achievement list is fetched; achievement
  shadow comparisons) and ADR 0061 (the survey lines of the achievement
  shadow)
- **Amends:** ADR 0046 (shadow answers for account data come from
  `rebornd`, not from values given to `reborn-observe arm`; writes are
  never shadowed) and ADR 0047 (a read-only shadow session for a process
  `rebornd` did not launch; "answers only for itself" covers the game's
  own DLC).
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
| `STEAMAPPS_INTERFACE_VERSION008` 7 | `BIsDlcInstalled` **or** `BIsSubscribedApp` | narrowed to two; both give false for a DLC that is not owned |
| `STEAMAPPS_INTERFACE_VERSION008` 6 | `BIsSubscribedApp` or `BIsAppInstalled` | candidate |
| `STEAMUSERSTATS_INTERFACE_VERSION011` 0, `…012` 0 | `RequestCurrentStats` | confirmed (experiments 13, 21) |
| `STEAMUSERSTATS_INTERFACE_VERSION011` 10, `…013` 9 | `StoreStats` | confirmed (experiment 23), recorded in two games (Cave Crawlers, Bongo Cat); `…012` 10 narrowed to `StoreStats` or `ResetAllStats` |
| `STEAMUSERSTATS_INTERFACE_VERSION011` 12 | `GetAchievementDisplayAttribute` | confirmed (experiment 2), only the key `name` recorded |
| `…012` 6, `…013` 5 | read a value by name (`GetAchievement`, `GetStat`, …) | candidates |
| `…011` 4 | set a value by name (`SetStat`, `SetAchievement`, …) | candidates |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION014` 0, 1, 15 | `FileWrite`, `FileRead`, `GetFileSize` | confirmed (experiment 1) |
| `STEAMREMOTESTORAGE_INTERFACE_VERSION014` 13, `…016` 13 | `FileExists`, `FilePersisted`, `FileForget` | candidates |
| callback 0x44d (1101) | `UserStatsReceived_t`: app ID 0..4, `EResult` 8..12, a Steam ID 12..20, size 24 | confirmed (experiment 21); bytes 12..20 still to be compared with the account's own Steam ID (review finding) |
| callback 0x44e (1102) | `UserStatsStored_t`: app ID 0..4, 4..8 zero, `EResult` 8..12, size 16; recorded 1 to 139 ms after the `StoreStats` of the same rank | confirmed (experiment 23) |
| callback 703 | `SteamAPICallCompleted_t`; `Steam_GetAPICallResult` | confirmed (experiment 22) |
| `Steam_BGetCallback`, `Steam_FreeLastCallback` | callback delivery from a queue | confirmed (experiment 19); queue in `crates/shim/src/callbacks.rs` (PR #24, merged), filled by nothing yet |

What Reborn's own client code (`crates/cm`, `crates/core`, `crates/content`)
already does, and what it does not:

| Need | Exists | Where |
| --- | --- | --- |
| The account's licences and owned apps (incl. DLC, app type `dlc`) | yes | `crates/cm/src/library.rs` `Session::owned_apps` (licence list → packages → apps via PICS, expired licences dropped); `crates/cm/src/pics.rs`; `docs/protocol/licenses-pics.md` |
| Which depots of an app belong to which DLC | yes | `crates/cm/src/apps.rs` `Session::app_details`, `DepotInfo::dlc_app_id` (`depots/<id>/dlcappid`) |
| A complete "DLC of this game" list, including DLC without depots | **no** sourced PICS field is read | — |
| The account's Steam ID | yes | `crates/cm/src/session.rs` `Session::steam_id` |
| Stats schema, stat values and unlock times (`ClientGetUserStats`, EMsg 818/819) | **partly**: the request exists; typed, only an achievement list leaves the crate; the whole response leaves it as protobuf bytes through `Session::record_achievements` (meant for fixtures). A typed public read of stat values and `crc_stats` is missing | `crates/cm/src/achievements.rs`; `docs/protocol/achievements.md`; fixture `user_stats.bin` |
| Storing stats and achievements | **no** (below, "Real stats writes") | — |
| Listing an app's cloud files | yes | `crates/cm/src/cloud.rs` `Session::cloud_files` (`Cloud.EnumerateUserFiles`, paging capped at 20 pages) |
| Downloading a cloud file (SHA-1 checked) | yes | `crates/content` `cloud::download_cloud_file_into`; `crates/core/src/cloud.rs` `restore_missing_saves`, run before each game start (ADR 0035) |
| Mapping files a game writes through the API (names without a root token, such as Cave Crawlers' `MainPlayerData.json`) to a local place | **no**: `crates/core/src/cloud.rs` reports them as not mapped | `docs/protocol/cloud.md`, "Auto-Cloud paths" |
| Uploading or deleting cloud files | **no**: `docs/protocol/cloud.md` "Upload and delete": no allowed reference, upload body format unknown, "not used until it is recorded and confirmed" | — |
| Telling Valve a game is running (`ClientGamesPlayed`) | **no** | — whether Valve accepts stats from a client that has not said so is **unknown** |
| Licence changes during a session | **no**: the licence list is read once after logon | `crates/cm/src/session.rs` `receive_initial` |
| The shim channel | yes, `Hello` only | `crates/shim-protocol/src/lib.rs`, `apps/rebornd/src/shim_server.rs` (line limit 64 KiB), `crates/shim/src/session.rs` |
| Reconnecting after a lost connection, with backoff | yes | `apps/rebornd/src/daemon.rs` (`connection_lost`, `reconnect_delay`, ADR 0025) |

ADR 0029 already says that Reborn's CM code never sends a store message
and that "writing happens only later in the shim, when a game asks (red
line 4)". Only the module comment of `achievements.rs` ("Reborn never
writes stats or achievements") says more than the ADR and has to be
brought in line.

ADR 0046 forbids answer mode **in the observation proxy** for ownership,
DLC, tickets, stats, achievements and cloud writes, and its shadow
answers come from values given to `reborn-observe arm`. ADR 0047 says
how the shim gets account data instead ("asked per call", answered by
`rebornd` from Valve's servers), trusts only processes `rebornd`
launched, and gives a game answers "only for itself". What it leaves
open is the content of those messages, what may be cached, and the rules
around them.

## Problem

How does the shim answer the confirmed ownership, stats and cloud
meanings, and deliver their callbacks, with data that really comes from
the user's account through `rebornd`, so that

- ownership is never invented (red line 3),
- stats, achievements and cloud files are written only because the game
  asked (red line 4, AGENTS.md section 5),
- the requests Reborn adds to Valve's servers are few, bounded and
  stated (red line 8),
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
   `Hello`. Few messages, but it fetches stats for games that never ask,
   puts a lot of account data into the game process up front, and
   cannot carry writes.
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
  SHA-1-checked, or from a file the game itself wrote through the shim.
  Nothing is ever filled in from defaults, from the official client's
  files or from memory.
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

### Amendments of ADR 0046 and ADR 0047

- **ADR 0046, shadow source:** for the calls in this ADR, the proxy's
  shadow answer comes from `rebornd` over the shim channel (it is
  account data that `arm` cannot know), not from a value given to
  `reborn-observe arm`. `GetAppID` stays as it is.
- **ADR 0046, writes:** stats stores and cloud writes are never run in
  shadow mode (below, stage 1). They go straight to a planned
  answer-mode experiment in shim mode, with the success rule of stage 3.
- **ADR 0047, trust:** besides processes it launched, `rebornd` accepts
  a **shadow session** for a process that `reborn-observe arm`
  registered with it for one app. A shadow session gets only read
  answers (`AppOwnership`, `RequestStats`, `CloudFileSize`, `CloudRead`
  against a separate shadow folder) and never a write; any write request
  on it is refused with `NotAllowed`.
- **ADR 0047, "only for itself":** a session gets ownership answers
  only for its own app and for app IDs that the own app's PICS info
  names as DLC (`DepotInfo::dlc_app_id`). Every other app ID gets
  `NotAllowed`, and the shim stops the game. Every request is checked
  against the session (one process, one app) before anything else; the
  daemon handler tests include a request for another app.

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
| `AppOwnership { app_id }` | `Ownership { owned }` | licence list (`owned_apps`) of the current Valve session |
| `RequestStats` | `Accepted`, later the event `StatsReceived` | `ClientGetUserStats` |
| `StoreStats { changes }` | `Accepted`, later the event `StatsStored` | store request (does not exist yet, below) |
| `CloudFileSize { name }` | `FileSize { size: Option<u32> }` | the run's cloud folder |
| `CloudRead { name, max }` | `FileData { data }` (base64) | the run's cloud folder |
| `CloudWrite { name, data }` | `Written { ok }` | the run's cloud folder |

Events (daemon → shim):

- `StatsReceived { result, achievement_names }`: the `EResult` from
  Valve, and for `GetAchievementDisplayAttribute("name")` the display
  name of each achievement in the language chosen below. Stat values and
  unlock state are **not** sent while the readers (`…012` 6, `…013` 5)
  are candidates; they are added by the amendment that confirms one.
- `StatsStored { result }`: the `EResult` of one store.

Every request can also be answered with `Unavailable { reason }`:
`NotSignedIn`, `Offline`, `NotAllowed` (not this session's business),
`TooLarge`.

The shim, not the daemon, turns an event into callback bytes, because
the byte layout is shim knowledge with provenance in `docs/shim/`
(`UserStatsReceived_t` and `UserStatsStored_t` above). It puts the
callback into the queue of `crates/shim/src/callbacks.rs` under the
handle (`rcx`) of the pipe on which the game got the `SteamUserStats`
object; while it is a candidate whether `rcx` is the pipe or the user
handle, a game that polls on more than one handle with a stats request
open stops with a log line.

**Alternative for `AppOwnership` (not proposed):** answer for any app
ID (the `ISteamApps` page documents `BIsSubscribedApp` as taking an
app ID, without limiting which; `meanings.md`, position 7). It would also cover DLC without depots, but
it lets a game find out what else is in the user's library by asking
about app IDs one by one, a privacy cost the strict rule avoids.

### How the shim answers each confirmed meaning

| Call | Shim answer |
| --- | --- |
| `BIsSubscribed` (`…008` 0) | `AppOwnership` for the session's app; `owned` |
| `…008` 7 (`BIsDlcInstalled` or `BIsSubscribedApp`) | `AppOwnership`; not owned → false (both candidates agree); **owned → stop**. When an owned DLC counts as "installed" is not sourced (in particular for a DLC without depots), and the two candidates may differ there; the test account owns no DLC, so this was never observed |
| `…008` 6 | **stop** (candidate) |
| `RequestCurrentStats` | sends `RequestStats`, returns true after the daemon's `Accepted`, as recorded (result 1 in eleven games); the callback 0x44d follows when the event arrives. `Unavailable` → see "Offline" |
| `GetAchievementDisplayAttribute(name, "name")` | from `achievement_names` of the last `StatsReceived`, a NUL-terminated text the shim keeps alive for the run; other keys, unknown names or no stats yet → **stop** |
| `StoreStats` (`…011` 10, `…013` 9) | **stop** for now (below, "An empty `StoreStats`"); later sends `StoreStats` with the game's changes and returns true after `Accepted`; one 0x44e per `StoreStats` when its `StatsStored` arrives |
| `…012` 10 (`StoreStats` or `ResetAllStats`) | **stop** until it is told apart: a reset is a write the game might not mean, and is never guessed |
| `GetFileSize` (`…014` 15) | `CloudFileSize`; a missing file is answered only once the official client's answer for a missing file is recorded, until then **stop** |
| `FileRead` (`…014` 1) | `CloudRead`; returns the number of bytes copied into the game's buffer, at most the buffer size the game passed |
| `FileWrite` (`…014` 0) | `CloudWrite`; returns true when the file is written in the run's cloud folder |
| `…014` 13, `…016` 13, readers, setters, `RequestUserStats`, tickets, language | **stop** (candidates, or not decided here) |

**Language of achievement names.** The recorded key `name` is the
achievement's name in the player's language, but which language the
game expects is not observed (`GetCurrentGameLanguage` at `…008` 4 is a
candidate). The proposal: a Reborn setting, and English
(`FALLBACK_LANGUAGE` in `achievements.rs`) until that setting exists.

**An empty `StoreStats`.** With no confirmed setter, a game that reaches
`StoreStats` has changed nothing through the shim. Answering it with a
local 0x44e and `EResult` 1 would be a carve-out from the principle that
every answer comes from a Valve response: it says "stored" without
asking Valve, and nobody has recorded what the official client does for
a store without changes. **Default: stop the game at `StoreStats`**
until an experiment records an empty store (for example timing and
result of 0x44e after a `StoreStats` with no setter call before it). In
practice: Cave Crawlers calls position 4 (a setter candidate) before its
first `StoreStats` and stops there anyway; Bongo Cat calls positions 1,
3, 5 and 8 of `…013`, none confirmed, so only Bongo Cat might reach an
empty store, and only if the recording shows it calls none of them first.

**Store timing.** One 0x44e per `StoreStats`, delivered as soon as
Valve's answer for that store arrives; the recorded official client
delivered it 1 to 139 ms after the call. Stores of one app are sent one
at a time (a `StoreStats` while one is in flight waits for it); they are
not merged and not delayed by a fixed interval, because nothing recorded
says the official client does that. If bursts (Bongo Cat: four calls
within 2 ms) turn out to need a limit, an experiment on the official
client's callback timing decides it first.

**Real stats writes** need, before any code:

1. a confirmed setter (experiment with the bytes at `r8`, ADR 0043/0053);
2. a source for the store message flow. A GitHub code search on
   2026-11-22 found only message definitions and EMsg numbers, no code
   that sends them: SteamKit2
   `SteamKit2/SteamKit2/Base/Generated/SteamMsgClientServerUserStats.cs`
   (`CMsgClientStoreUserStats2`) and `Resources/SteamLanguage/emsg.steamd`
   (`ClientStoreUserStats2 = 5466`); node-steam-user
   `protobufs/steammessages_clientserver_userstats.proto` and
   `enums/EMsg.js`; ValvePython/steam
   `protobufs/steammessages_clientserver_userstats.proto` and
   `steam/enums/emsg.py`. `steam-vent-proto-steam` 0.5.2 has the
   generated types. Field names (`settor_steam_id`, `settee_steam_id`,
   `crc_stats`, `explicit_reset`, `stats`) are not a flow. The search
   index may be incomplete; finding a flow is open work;
3. a fixture. This is **unsolved**: the official client's CM traffic is
   encrypted, so it cannot be recorded from outside, and recording from
   Reborn means sending an unconfirmed message live from the test
   account, which changes that account's stats and needs the maintainer's
   explicit OK for each run (AGENTS.md section 7, ADR 0051);
4. an answer to whether Valve needs `ClientGamesPlayed` first.

### What is cached, and where

- **In `rebornd`, in memory, per shim run** (keyed by the process ID,
  dropped when the game exits):
  - the set of owned app IDs, from the licence list of the current Valve
    session, and the own app's DLC IDs from its PICS info;
  - the stats response of `ClientGetUserStats` for this app (schema,
    values, `crc_stats`), fetched on the game's first `RequestStats`;
  - later, the open stats changes until they are stored.

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
  records for each file the **base**: SHA-1, size and timestamp of the
  version last downloaded from the cloud. These are the user's saves,
  not secrets. File names from the game and from Valve are checked like
  install paths (no `..`, no absolute paths, no root tokens, no symlinks
  followed) before use (section 5). Shadow runs use a separate shadow
  folder that no game writes to.

### Cloud

- **Two save lines.** Without uploads, saves a game writes in shim mode
  exist only in Reborn's folder. The official client, also on the same
  computer, never sees that folder; it plays from the cloud version. So
  one game can end up with two diverging saves: Reborn's local one and
  the cloud one the official client keeps using and uploading. The
  frontend says so before the first shim-mode launch of a game.
- **At launch** (extends ADR 0035), for each file, with L the local
  SHA-1, C the cloud's and B the base:
  - L = C: nothing to do;
  - no local file: download (as `restore_missing_saves` does);
  - L = B, C ≠ B: only the cloud changed (for example the official client
    played meanwhile): the cloud version is downloaded, SHA-1-checked,
    and replaces the local file, which is kept as a backup first; nothing
    the user made in Reborn is lost;
  - C = B, L ≠ B: only the local file changed: kept (it waits for
    uploads);
  - otherwise (both changed, or no base and they differ): a **true
    conflict**. Nothing is overwritten. In shim mode, the frontend asks
    the user before the game starts which version to play with; the
    other stays as a backup. This differs from ADR 0035, which starts
    with the local file and only reports.
- **During the run:** `FileRead`, `GetFileSize` and `FileWrite` work on
  the run's cloud folder only. A write goes to a temporary file first
  and replaces the old file only when complete. The size limit is the
  app's `ufs` `quota` from its app info (Cave Crawlers: 1 000 000), at
  most 16 MiB. Base64 makes data 4/3 as long, so a 16 MiB write is a
  line of about 21.4 MiB plus the JSON around it; the shim socket's line
  limit for a session is set from the app's quota (about 4/3 of it plus
  4 KiB, at most 22 MiB) instead of the fixed 64 KiB. A larger write
  gets `TooLarge` and stops the game. Splitting large files into chunks
  is the alternative if measurements show the long lines hurt.
- **Upload:** none, until the upload protocol is backed by an allowed
  source and a fixture (`docs/protocol/cloud.md`) and the maintainer
  accepts a separate ADR. When uploads come, the rule is fixed here
  already: before uploading, `rebornd` lists the cloud again; if a file
  differs from the base, nothing is uploaded and the user decides
  (section 5).

### Rate limits (red line 8)

- **Answer mode** (official client closed): ownership needs no request
  per call (one licence list per daemon session, as today); at most one
  `ClientGetUserStats` per app per run, a repeated `RequestCurrentStats`
  answered from the run's cache (the recorded games call it once); one
  cloud listing and the needed downloads at launch, nothing per call;
  stores one at a time, as above.
- **Shadow mode adds requests** on top of the official client's own
  traffic for the same account: `rebornd`'s own sign-in with the stored
  refresh token, one `ClientGetUserStats` per run, and one cloud listing
  plus a download of each file the proxy compares for `FileRead` into
  the shadow folder. These are bounded per run, and runs are bounded by
  ADR 0045 and 0052 (one minute between starts).
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
   with round-trip tests; in `reborn-cm` a typed public read of the
   stats response (an API change this ADR covers); the run's cloud
   folder, its base file and its path checks in `reborn-core`; daemon
   handlers tested against fixtures, including refusals for other apps
   and for writes on shadow sessions. The experiment comparing bytes
   12..20 of 0x44d with the account's own Steam ID (match or mismatch
   only) runs before the shim writes that field.
2. **Stage 1, shadow mode for reads (ADR 0046, as amended):** the
   observation proxy, with the official client running and `rebornd`
   signed in to the same account, asks `rebornd` for Reborn's answer
   next to the official one, for `BIsSubscribed`, position 7 (unowned
   DLC only), `GetFileSize`, `FileRead` (compared by size and SHA-1
   only), `RequestCurrentStats` (compared by 0x44d's size and a hash of
   its confirmed fields) and `GetAchievementDisplayAttribute`. The log
   says only `shadow match` or `shadow mismatch` (account data).
   `reborn-observe arm --app <id>` registers the process as a shadow
   session. **Two sign-ins at once** carry risks, each of which stops
   the stage:
   - Steam may end one of the two sessions or disturb the official
     client's game session; any logoff or kick of either client stops
     the stage;
   - the traffic for the account roughly doubles during the run (above);
   - `rebornd` signs in with the stored refresh token, where Steam Guard
     is not expected; if Steam asks for it, the agent stops (ADR 0051:
     stop at sign-ins) and the maintainer decides.
   Experiments have already signed in this way next to the official
   client (`reborn-cli library --all`, experiment 8), but only briefly.
3. **Stage 2, answer mode for reads, per game:** in shim mode (ADR
   0048, official client closed), never in the proxy; ADR 0046's ban
   for the proxy stays. Answer mode is **per game**, not per call: a game
   runs on the shim only when every position it calls is answered, so a
   call's answer can only be tried in games whose other calls are all
   covered. A game qualifies when shadow mode matched for each of its
   account-data calls and the maintainer gave the OK; the prediction is
   ADR 0046's (the game behaves as in a normal run). It may be that no
   game qualifies for a while; `docs/shim/gaps.md` shows which positions
   block each game.
4. **Stage 3, writes, without shadow mode:** this deviates from ADR
   0046, because a shadow write would be a second write the game did not
   ask Reborn for. Instead each write kind gets a planned answer-mode
   experiment with a stated success rule:
   - cloud `FileWrite`: the game behaves as in a normal run, the file in
     Reborn's folder is complete (size as passed), and at the next start
     the game reads back the same bytes (same SHA-1);
   - stats store (once its prerequisites exist): Valve's `EResult` is OK,
     0x44e arrives per call, and a later `ClientGetUserStats` shows the
     values the game set and nothing else changed.
   The three-game rule cannot be met for `StoreStats` today: only two
   recorded games call it (Cave Crawlers, Bongo Cat). Either a third
   game is recorded first, or the maintainer accepts two.
5. **Later, separate ADR:** cloud upload.

Games, from `docs/shim/m4-games.md`:

- Cave Crawlers (2143030): stats `…011`, `StoreStats`,
  `GetAchievementDisplayAttribute`, cloud files through the API, DLC at
  position 7;
- Bongo Cat (3419430): stats `…013`, `StoreStats` five times;
- Plant Therapy (2505120) or A Park Full of Cats (2610090): DLC checks
  at position 7; An Arcade Full of Cats (2368470), Dark Deception
  (332950) or A Park Full of Cats for `BIsSubscribed`.

All runs use the secondary test account (ADR 0044), which owns these
free games and no DLC.

### Open for the maintainer

1. Accept this ADR as a whole (it touches red lines 3, 4 and 8).
2. The amendments of ADR 0046 (shadow answers from `rebornd`; writes
   never shadowed) and ADR 0047 (shadow sessions for processes Reborn
   did not launch; "only for itself" includes the game's DLC).
3. `AppOwnership` strict (the game and its DLC, proposed) or for any app
   ID (lets a game see what else the user owns).
4. An empty `StoreStats`: stop the game (proposed) until an experiment
   records an empty store.
5. Store timing: one 0x44e per call, promptly, stores one at a time; no
   merging without an observation.
6. Real stats writes: whether and how a live store from the test
   account may be recorded as the fixture.
7. Cloud without upload: saves in shim mode stay in Reborn's folder; the
   official client on the same computer does not see them, so two save
   versions can result.
8. A true cloud conflict blocks the shim-mode launch until the user
   chooses (proposed), or the game starts with the local file as in ADR
   0035.
9. Shadow mode with `rebornd` signed in while the official client runs
   on the same account, with the risks above.
10. The games; `StoreStats` has only two recorded games.
11. The language of achievement names.

## Decided by the maintainer (2026-11-22)

Asked in four questions, the maintainer answered:

- **The ADR as a whole, with every proposal** (strict ownership: the game
  and its DLC; an empty `StoreStats` stops the game until an experiment
  records one; one 0x44e per `StoreStats` as soon as Valve answers, no
  merging; cloud saves in shim mode stay local, with a clear notice that
  the official client does not see them; a true cloud conflict asks the
  user before the game starts; achievement names in English until Reborn
  has a setting), including the explicit amendments of ADR 0046 and 0047:
  accepted.
- **Shadow mode with two sign-ins** (`rebornd` and the official client on
  the test account at once): allowed; any logoff or kick stops the stage.
- **A live stats write from the test account** to record the store
  exchange: allowed in principle; the agent plans that experiment and
  asks the maintainer once more right before the single run.
- **`StoreStats` with only two recorded games:** record a third game
  first; the three-game rule stays.

## Consequences

- The shim gets its first real account data without ever holding a
  token, and every answer can be traced to one Valve response or one
  checked file.
- Several confirmed meanings still stop the game: `…008` 6, every owned
  DLC at position 7, `StoreStats`, every stats reader and setter, `…012`
  10, `FileExists`-like calls, and missing files at `GetFileSize`. Games
  on the M4 list that use them keep stopping until each is confirmed.
  That is intended.
- Cloud saves written by a game in shim mode stay local until uploads
  are decided, and drift apart from what the official client sees; the
  base SHA-1 at least tells a harmless "only the cloud changed" from a
  true conflict.
- Stats writes need a store request that `reborn-cm` does not have, a
  flow nobody has sourced yet and a fixture nobody can record without a
  live write; they are the slowest part and the one with the most
  account risk (ADR 0012).
- `reborn-cm`, `reborn-shim-protocol` and possibly `reborn-ipc` (the
  conflict choice and the "saves stay local" notice) change their public
  API; each change is covered by this ADR or an amendment.
- Shadow mode makes Reborn's own requests in addition to the official
  client's; they are bounded per run and stated above.
- More moving parts in `rebornd`: a per-run state, shadow sessions, a
  cloud folder with base records and a stats queue, all with tests
  against fixtures.
