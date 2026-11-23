# 0059: Achievement names and leaderboards in the shim

- **Status:** Accepted (decided by the agent under the maintainer's
  delegation of 2026-11-23, ADR 0060)
- **Date:** 2026-11-23
- **Amends:** ADR 0055 (when the achievement list is fetched; new shim
  protocol messages; new shadow comparisons)
- **Touches:** red lines 3, 4 and 8, the public API of
  `reborn-shim-protocol` and `reborn-cm`.

## Context

Experiment 32 (`docs/shim/meanings.md`, "Achievement names and
leaderboards (loop iteration 16a)") confirmed nine positions of the
`ISteamUserStats` interfaces:

| Interface and position | Meaning | Recorded in |
| --- | --- | --- |
| `STEAMUSERSTATS_INTERFACE_VERSION011` 14, `…012` 14 | `GetNumAchievements` (under A1 and A3) | Outcore, One's Own Exile |
| `…011` 15, `…012` 15 | `GetAchievementName` | Outcore, One's Own Exile |
| `…012` 23, `…013` 22 | `FindLeaderboard` | Fallout Shelter; Pixel Puzzles |
| `…012` 29 | `DownloadLeaderboardEntriesForUsers` (under A2) | Fallout Shelter |
| `…013` 30 | `UploadLeaderboardScore` (under A2) | Pixel Puzzles |
| `…013` 23 | `GetLeaderboardName` | Pixel Puzzles |

LOOP step 7 asks for Reborn's answer in the shim, with a shadow entry
where Reborn can compute it. This ADR records why none of the nine could
be answered within ADR 0055 as accepted, lays out the options, and
decides them.

### What the shim has today

`crates/shim/src/objects.rs` hands out objects for `SteamClient018` to
`023`, `SteamUtils009`/`010` and `STEAMAPPS_INTERFACE_VERSION008` only.
**There is no `STEAMUSERSTATS` object at all**: a game that asks for any
`STEAMUSERSTATS_INTERFACE_VERSION…` gets no interface and stops
(`interface_or_stop`). Missing, before any of the nine positions can be
answered:

1. a stats object per version (`…011`, `…012`, `…013`), every slot a
   stop stub except confirmed ones;
2. `RequestCurrentStats` (position 0) sending `RequestStats`
   (`crates/shim/src/session.rs` can send it and receive events, but no
   game call sends it yet);
3. turning the event `StatsReceived` into callback 0x44d in the queue of
   `crates/shim/src/callbacks.rs` (ADR 0055 describes it; not built);
4. for leaderboards: call handles, callback 703 (`SteamAPICallCompleted_t`,
   experiment 22) and `GetAPICallResult` (`SteamUtils010` 13 and
   `Steam_GetAPICallResult`, experiments 22 and 26) filling the game's
   buffer with a call result whose byte layout is not confirmed.

`rebornd` already answers `RequestStats` for the run's own app with
`StatsReceived { result, achievement_names }` (`apps/rebornd/src/shim_stats.rs`),
the names built by `reborn_cm::achievements::UserStats::achievements`:
every entry under `bits` of every stat, stats and bits each in the order
they appear in the schema (`docs/protocol/achievements.md`).

### A timing observation that decides the achievement design

From the local logs of experiment 32 and its earlier checks (times in ms
after the proxy loaded; no names, values or handles quoted). Outcore
calls `RequestCurrentStats` twice; the column gives the first call.

| Log | Game | first `RequestCurrentStats` (0) | first `GetNumAchievements` (14) | last `GetAchievementName` (15) returns | first 0x44d delivered | `Steam_BGetCallback` calls |
| --- | --- | --- | --- | --- | --- | --- |
| `observe-49168` | Outcore | 93 | 109 | 116 | 425 | 50 |
| `observe-47724` | Outcore | 90 | 104 | 109 | 405 | 50 |
| `observe-64880` | Outcore | 95 | 112 | 120 | 410 | 50 |
| `observe-51644` | One's Own Exile | 66 | 66 | 70 | none | 0 |
| `observe-612` | One's Own Exile | 63 | 64 | 67 | none | 0 |
| `observe-56900` | One's Own Exile | 66 | 66 | 69 | none | 0 |

The official client answers the first `GetNumAchievements` 0 to 17 ms
after the first `RequestCurrentStats`, and the last `GetAchievementName`
returns up to about 25 ms after it in Outcore and 3 to 5 ms after it in
One's Own Exile: all within about 25 ms, roughly 300 ms **before** it
delivers `UserStatsReceived_t` (Outcore), or in a game that never polls
`Steam_BGetCallback` at all (One's Own Exile).

Where the official client takes these answers from is **not recorded**.
Two possible causes, neither observed: a cache on disk from earlier runs,
or a fetch the official client starts itself when the game launches.

The 300 ms is the official client's delay for 0x44d (250 to 310 ms in
loop iteration 9; 265 to 309 ms recorded in experiments 20 and 21).
Reborn's own `ClientGetUserStats` round-trip time is **not recorded**.
`rebornd` fetches the stats (and with them the schema) only on the game's
first `RequestStats` (ADR 0055, "What is cached, and where"). Unless that
round trip is much shorter than 25 ms, under ADR 0055 as accepted the
shim has nothing to answer with when these calls arrive, and its rule
"honest failure" stops the game.

What the official client answers on a computer where it has never run
the game is **not recorded**.

## The nine positions in three groups

### Group 1: game data (public, from the app's stats schema or from the game)

`GetNumAchievements`, `GetAchievementName`, `GetLeaderboardName`.

- **Where Reborn gets it:**
  - Count and API names: the schema in `CMsgClientGetUserStatsResponse`
    (EMsg 819), already fetched by `Session::user_stats` with
    `schema_local_version` -1 ("no local copy, so Steam sends
    everything"; `docs/protocol/achievements.md`, sources SteamKit issue
    #531, Goldberg `generate_emu_config.py`, Steam Achievement Manager
    `SAM.Game/Manager.cs`; fixture `user_stats.bin`).
  - **Unknown, needs a source:** that index *i* of `GetAchievementName`
    is the *i*-th achievement in Reborn's order (above). The public
    `ISteamUserStats` page says only "an index between 0 and
    `GetNumAchievements`". Also unknown: whether the official count
    equals Reborn's (every `bits` entry of every stat, group achievements
    included). Both are **checked** by a shadow comparison (decision 2),
    not assumed.
  - `GetLeaderboardName(handle)`: experiment 32 (T1) recorded the result
    text as equal to the text the game passed to `FindLeaderboard` for
    that handle. Valve's find answer also carries a `leaderboard_name`.
    Which of the two the official client returns when they differ (for
    example only in letter case) is **not recorded**; in that case the
    shim stops. The shim answers from its own handle table without asking
    anyone, but only once it hands out handles, which needs
    `FindLeaderboard`.
- **Shim protocol:** `StatsReceived` already carries `achievement_names`
  (internal `name` and English `display_name`) in Reborn's order; count
  and API names need nothing new. The missing piece is **when** the shim
  has them (decision 1). `GetLeaderboardName` needs nothing from the
  daemon.
- **ADR 0055's table:** lists only `RequestCurrentStats` and
  `GetAchievementDisplayAttribute` for stats; the event's names were meant
  for the second, delivered after the game's request. It says nothing
  about `GetNumAchievements`, `GetAchievementName` or leaderboards, and
  its stage-1 list of shadow comparisons does not include them.

### Group 2: account and network data (read)

`FindLeaderboard`, `DownloadLeaderboardEntriesForUsers`.

- **Where Reborn gets it:**
  - `FindLeaderboard`: `CMsgClientLBSFindOrCreateLB` (EMsg 5416) with
    `app_id`, `leaderboard_name` and `create_if_not_found` false; answer
    `CMsgClientLBSFindOrCreateLBResponse` (EMsg 5417: `eresult`,
    `leaderboard_id` int32, `leaderboard_entry_count`, sort method,
    display type, name). Sources: SteamKit2
    `SteamKit2/SteamKit2/Steam/Handlers/SteamUserStats/SteamUserStats.cs`
    `FindLeaderboard` (commit `00b87042`, lines 53 to 63) and ValvePython/steam
    `steam/client/builtins/leaderboards.py` `get_leaderboard` (commit
    `32752256`, lines 16 to 33); message types in
    `steam-vent-proto-steam` 0.5.2 `steammessages_clientserver_lbs.proto`.
    Never `create_if_not_found` true: the confirmed meaning is the plain
    find (experiment 32 ruled out `FindOrCreateLeaderboard`). `app_id` is
    always the session's own app, never a value from the game.
  - `DownloadLeaderboardEntriesForUsers`: `CMsgClientLBSGetLBEntries`
    (EMsg 5418) with `app_id`, `leaderboard_id`, data request "users" and
    `steamids`; answer EMsg 5419 with entries (`steam_id_user`,
    `global_rank`, `score`, `details`, `ugc_id`). Sources: SteamKit2, same
    file, `DownloadLeaderboardEntriesForUsers` (lines 137 to 151);
    ValvePython/steam, same file, `get_entries` with `steam_ids` (lines 103
    to 126).
  - **Unknown, needs a source:** how Valve's int32 `leaderboard_id` maps
    to the game's 64-bit `SteamLeaderboard_t`; the byte layout of
    `LeaderboardFindResult_t` and `LeaderboardScoresDownloaded_t` (only
    the 703 payload sizes, 16 and 24, are recorded; that these are the
    call results' sizes rests on assumption A2 of experiment 32); what the
    "entries handle" in `LeaderboardScoresDownloaded_t` is and how a game
    reads the entries (`GetDownloadedLeaderboardEntry` is not confirmed,
    so a download alone is useless to a game). No fixture of LBS traffic
    exists; recording one needs a live read from the test account
    (ADR 0044).
- **Account data:** the Steam IDs a game passes to
  `DownloadLeaderboardEntriesForUsers` (Fallout Shelter passes 4 per
  call, 13 calls at start; the array is not read, ADR 0043) are other
  people's or the user's own; the answer holds their scores. Fallout
  Shelter makes 13 finds and 13 downloads at every start: 26 Valve
  requests per launch. In answer mode (official client closed) these
  replace the official client's own requests; only shadow mode, with
  both clients signed in, doubles them.
- **Shim protocol:** new requests, for example
  `FindLeaderboard { name }` → `LeaderboardFound { found, id,
  entry_count }` and `LeaderboardEntriesForUsers { id, users }` →
  `LeaderboardEntries { entries }`, plus a way to deliver call results
  (703 and its buffer). None exists; the exact names are settled in the
  implementation PR.
- **ADR 0055's table:** has **no leaderboard row** and no "answer for
  another user" rule; its "only for itself" rule covers ownership only.

### Group 3: writes

`UploadLeaderboardScore`.

- **Where Reborn gets it:** the message type `CMsgClientLBSSetScore`
  (EMsg 5414: `app_id`, `leaderboard_id`, `score`, `details`,
  `upload_score_method`) and its response exist in
  `steam-vent-proto-steam` 0.5.2. **Unknown, needs a source:** a GitHub
  code search on 2026-11-23 for `ClientLBSSetScore` found only EMsg lists
  and `.proto` files in SteamKit2, node-steam-user and ValvePython/steam;
  none of them sends it (SteamKit2's `SteamUserStats.cs` and
  ValvePython's `leaderboards.py` have find and get-entries only). A
  field list is not a flow. Stopped here for this item, as AGENTS.md
  section 4 rule 4 requires.
- **Red line 4:** the upload happens only because the game called
  `UploadLeaderboardScore` in this run, with the game's own score and
  method, and only to the signed-in account. Pixel Puzzles uploads at
  start without any input (experiment 32 and its two earlier runs); that
  is still the game's own request.
- **Shim protocol:** a write request such as `UploadLeaderboardScore {
  id, method, score, details }` → `Accepted`, later an event with the
  result; refused on shadow sessions (`Request::writes`, ADR 0055).
- **ADR 0055's table:** no row. Its stage 3 rule applies by analogy:
  writes are never shadowed, each write kind gets a planned answer-mode
  experiment with a success rule.

## Options for group 1 (achievement count and names)

- **(a) Stop the game (status quo).** Outcore and One's Own Exile keep
  stopping at position 14. No new risk; no progress.
- **(b) Fetch at shim-mode launch.** `rebornd` starts its one
  `ClientGetUserStats` for the run as part of the launch, alongside the
  ownership data it already prepares for the run then, and hands the
  public part (count, internal names, English display names) to the shim
  as soon as it arrives. One Valve request per run for games that ask for
  stats (the same as today), one extra for games that never do. The game
  still gets callback 0x44d only after its own `RequestCurrentStats`.
  - This brings back part of ADR 0055's rejected alternative 2,
    "Snapshot at launch". Its reasons do not apply here: it was rejected
    because it put a lot of account data into the game process up front
    and could not carry writes. Here only the public schema part reaches
    the shim (stat values and unlock state stay in `rebornd` as today),
    writes are untouched, and the extra cost is bounded at one request
    per launch.
- **(c) Wait inside the call.** The shim, at the game's first call that
  needs the list, blocks the call until the list arrives, for a bounded
  time, then stops. No extra request, but the game's thread stalls for
  Reborn's round trip, a pause the official client never shows; whether
  games tolerate it is unrecorded.
- **(d) Keep the public part on disk.** The count and names of a schema
  fetched in an earlier run, under Reborn's data folder; the first start
  still needs (b) or (c). ADR 0055 keeps all of the stats response in
  memory only, and its logging note treats achievement names as account
  data (logs say only counts); a file of names would need its own rule.
  A stale schema (the developer added achievements) would need a rule as
  well: Reborn sends `schema_local_version` -1 (sourced, above), but what
  the field does with other values is not sourced.
- **Checked and dropped: start the fetch when the game asks for the
  stats interface.** One's Own Exile gets the interface at 64 ms and
  calls 14 at 66 ms (`observe-56900`); Outcore gets it at 94 ms and calls
  14 at 112 ms (`observe-64880`). That gains at most about 20 ms over
  fetching at `RequestCurrentStats`, too little for a Valve round trip.

## Options for groups 2 and 3 (leaderboards)

1. **Leave them stopping** until the unknowns above have sources:
   handle mapping, call-result layouts, `GetDownloadedLeaderboardEntry`,
   a flow for `SetScore`.
2. **Read-only leaderboards first** (find, then name): plan an experiment
   that reads the 16 bytes of `LeaderboardFindResult_t` from
   `GetAPICallResult`'s buffer (ADR 0056 allows call-result reads) to
   confirm its layout; record a fixture of one live find from the test
   account; then implement find and name.
3. **Uploads** only after a flow source exists and with a separate
   planned experiment.

For `DownloadLeaderboardEntriesForUsers` the IDs could be forwarded as the
game passes them, or filtered. Filtering has no source and would make
Reborn's answers differ from Valve's (the game would get fewer entries
than it asked for without knowing why).

## Decision

Decided by the agent under the maintainer's delegation of 2026-11-23
(ADR 0060):

1. **Achievement list: option (b) with the bounded fallback of (c).**
   `rebornd` fetches the run's stats at each shim-mode launch, alongside
   the launch, and only the public schema part (count, internal names,
   English display names) goes to the shim. If the list has not arrived
   when the game first calls a position that needs it, that call waits
   at most **2 s**, then the game stops with a log line, as today. For
   the user: on a normal connection the game starts as it does with the
   official client; on a slow one it may pause up to 2 s at start; with
   no answer it stops as it does today. Stat values, unlock state and the
   rules for 0x44d stay as ADR 0055 decided.
2. **Shadow comparison: yes.** In a shadow session the proxy compares the
   official `GetNumAchievements` value with the length of Reborn's list,
   and the official text of `GetAchievementName(i)` with Reborn's *i*-th
   internal name for each index (the proxy reads the result text in
   memory, hashes it with SHA-256 and compares digests; the text is never
   logged, and position 15 is no longer a `READS` result text). Only
   "match" or "mismatch" is logged, never a name. This adds to ADR 0055's stage-1 list.
3. **Three games.** A third game that calls these positions is recorded
   before answer mode; the three-game rule of LOOP step 7 stays.
4. **Leaderboard reads** (`FindLeaderboard`, `GetLeaderboardName`):
   allowed in principle, always for the session's own app ID, at most one
   find per name per run (later finds of the same name are answered from
   the run's cache). Built only after an experiment has confirmed the
   layout of the reply data (`LeaderboardFindResult_t`) and the handle
   mapping. One live find from the test account may be recorded as the
   fixture.
5. **Other players' scores** (`DownloadLeaderboardEntriesForUsers`): stay
   stopped for now. When built, the IDs the game passes are forwarded
   unchanged, because Valve decides what it returns.
6. **Score uploads** (`UploadLeaderboardScore`): stay stopped until a
   public, allowed source sends the message. The first live upload is a
   separate experiment the agent decides on right before it runs
   (ADR 0060), and happens only because a game asked (red line 4).

## Consequences

- Outcore and One's Own Exile get a way past position 14 once the stats
  object, `RequestCurrentStats`, 0x44d and the launch fetch exist in the
  shim, and the shadow comparison has matched in three games.
- One extra `ClientGetUserStats` per shim-mode launch for games that never
  ask for stats (red line 8: bounded, one per launch).
- Leaderboard games (Fallout Shelter, Pixel Puzzles, Coin Maker
  Simulator) keep stopping at their first find until decision 4's
  experiment and fixture exist.
- Nothing in this ADR adds a write to the account.
