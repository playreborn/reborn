# 0059: Achievement names and leaderboards in the shim

- **Status:** Proposed
- **Date:** 2026-11-23
- **Amends (if accepted):** ADR 0055 (when stats are fetched; new shim
  protocol messages; new shadow comparisons)
- **Touches:** red lines 3, 4 and 8, the public API of
  `reborn-shim-protocol` and `reborn-cm`. Under ADR 0051 the agent cannot
  accept it; the maintainer decides.

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
where Reborn can compute it. This ADR records why none of the nine can be
answered within the accepted ADRs today, and lays out the options.

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
the names built by `reborn_cm::achievements::UserStats::achievements`
(every entry under `bits` of every stat, in schema order,
`docs/protocol/achievements.md`).

### A timing observation that decides the achievement design

From the local logs of experiment 32 and its earlier checks (times in ms
after the proxy loaded; no names, values or handles quoted):

| Log | Game | `RequestCurrentStats` (0) | first `GetNumAchievements` (14) | first 0x44d delivered | `Steam_BGetCallback` calls |
| --- | --- | --- | --- | --- | --- |
| `observe-49168` | Outcore | 93 | 109 | 425 | 50 |
| `observe-47724` | Outcore | 90 | 104 | 405 | 50 |
| `observe-64880` | Outcore | 95 | 112 | 410 | 50 |
| `observe-51644` | One's Own Exile | 66 | 66 | none | 0 |
| `observe-612` | One's Own Exile | 63 | 64 | none | 0 |
| `observe-56900` | One's Own Exile | 66 | 66 | none | 0 |

The official client answers `GetNumAchievements` and every
`GetAchievementName` 0 to 16 ms after `RequestCurrentStats`, about 300 ms
**before** it delivers `UserStatsReceived_t` (Outcore), or in a game
that never polls callbacks at all (One's Own Exile). It therefore answers
from data it already has locally. Reborn's `rebornd` fetches the stats
(and with them the schema) only on the game's first `RequestStats` (ADR
0055, "What is cached, and where"), and the official 0x44d took 250 to
310 ms after the request (experiments 20, 21). So under ADR 0055 as
accepted, the shim has nothing to answer with when these calls arrive,
and its rule "honest failure" means it stops the game.

What the official client answers on a computer where it has no local
schema yet (first start of a game) is **not recorded**.

## The nine positions in three groups

### Group 1: game data (public, from the app's stats schema or from the game)

`GetNumAchievements`, `GetAchievementName`, `GetLeaderboardName`.

- **Where Reborn gets it:**
  - Count and API names: the schema in `CMsgClientGetUserStatsResponse`
    (EMsg 819), already fetched by `Session::user_stats`
    (`docs/protocol/achievements.md`, source SteamKit issue #531, Steam
    Achievement Manager `SAM.Game/Manager.cs`; fixture `user_stats.bin`).
  - **Unknown, needs a source:** that index *i* of `GetAchievementName`
    is the *i*-th achievement in Reborn's schema order (stats in schema
    order, bits in key order). The public `ISteamUserStats` page says only
    "an index between 0 and `GetNumAchievements`". Also unknown: whether
    the official count equals Reborn's (every `bits` entry of every stat,
    group achievements included). Both can be **checked** by a shadow
    comparison (below), not assumed.
  - `GetLeaderboardName(handle)`: experiment 32 (T1) recorded the result
    as the text the game passed to `FindLeaderboard` for that handle. The
    shim could answer it from its own handle table without asking anyone,
    but only once it hands out handles, which needs group 2's
    `FindLeaderboard`.
- **Shim protocol:** `StatsReceived` already carries `achievement_names`
  (internal `name` and English `display_name`) in schema order; count and
  API names need nothing new. The missing piece is **when** the shim has
  them (decision 1). `GetLeaderboardName` needs nothing from the daemon.
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
    find (experiment 32 ruled out `FindOrCreateLeaderboard`).
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
    their sizes, 16 and 24, are recorded); what the "entries handle" in
    `LeaderboardScoresDownloaded_t` is and how a game reads the entries
    (`GetDownloadedLeaderboardEntry` is not confirmed, so a download alone
    is useless to a game). No fixture of LBS traffic exists; recording one
    needs a live read from the test account (ADR 0044).
- **Account data:** the Steam IDs a game passes to
  `DownloadLeaderboardEntriesForUsers` (Fallout Shelter passes 4 per
  call, 13 calls at start; the array is not read, ADR 0043) are other
  people's or the user's own; the answer holds their scores. Fallout
  Shelter's 13 finds and 13 downloads at every start would be 26 Valve
  requests per launch.
- **Shim protocol:** new requests, for example
  `FindLeaderboard { name }` → `LeaderboardFound { found, id,
  entry_count }` and `LeaderboardEntriesForUsers { id, users }` →
  `LeaderboardEntries { entries }`, plus a way to deliver call results
  (703 and its buffer). None exists.
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
- **Red line 4:** the upload must happen only because the game called
  `UploadLeaderboardScore` in this run, with the game's own score and
  method, and only to the signed-in account. Pixel Puzzles uploads at
  start without any input (experiment 32, recorded in three runs); that is
  still the game's own request.
- **Shim protocol:** a write request such as `UploadLeaderboardScore {
  id, method, score, details }` → `Accepted`, later an event with the
  result; refused on shadow sessions (`Request::writes`, ADR 0055).
- **ADR 0055's table:** no row. Its stage 3 rule applies by analogy:
  writes are never shadowed, each write kind gets a planned answer-mode
  experiment with a success rule, and a live write from the test account
  needs the maintainer's OK right before the run.

## Options for group 1 (achievement count and names)

1. **Stop the game (status quo).** Outcore and One's Own Exile keep
   stopping at position 14. No new risk; no progress.
2. **Fetch stats at shim-mode launch.** `rebornd` sends its one
   `ClientGetUserStats` for the run while it prepares the launch (as it
   already loads the licence list), and the shim gets the achievement
   list in the session (`Hello` answer) or as an event before the game
   starts. Same number of Valve requests for games that ask for stats
   (one per run), one extra for games that never do. Deviates from ADR
   0055's "fetched on the game's first `RequestStats`"; the game still
   gets callback 0x44d only after its own `RequestCurrentStats`.
3. **Wait inside the call.** The shim, at the first `GetNumAchievements`
   with a stats request open, blocks the game's call until `StatsReceived`
   arrives (at most a few seconds, then stop). No extra request, but the
   game's thread stalls about 300 ms or more, a timing the official
   client never shows; whether games tolerate it is unrecorded.
4. **Keep the schema on disk.** Only the public part (count, API names,
   display names) of a schema fetched in an earlier run, under Reborn's
   data folder; the first start still needs option 2 or 3. ADR 0055 keeps
   all of the stats response in memory only, and a stale schema (the
   developer added achievements) would need a rule (the request has
   `schema_local_version`, whose meaning is not sourced).

**Proposed:** option 2, with option 3 as the fallback when the launch
fetch has not finished (bounded wait, then stop). Plus a **shadow
comparison before any answer mode**: in a shadow session the proxy
compares the official count with the length of Reborn's list, and the
official text of each `GetAchievementName(i)` with Reborn's *i*-th
internal name (the proxy already reads the result text, experiment 32's
`READS`), logging only "match" or "mismatch". Three games matching
(LOOP step 7) confirms the order; today two games call these positions
(Outcore, One's Own Exile), so a third must be recorded.

## Options for groups 2 and 3 (leaderboards)

1. **Leave them stopping** until the unknowns above have sources:
   handle mapping, call-result layouts, `GetDownloadedLeaderboardEntry`,
   a flow for `SetScore`.
2. **Read-only leaderboards first** (find, then name): plan an experiment
   that reads the 16 bytes of `LeaderboardFindResult_t` from
   `GetAPICallResult`'s buffer (ADR 0056 allows call-result reads) to
   confirm its layout; record a fixture of one live find from the test
   account; then implement find and name. Downloads only after
   `GetDownloadedLeaderboardEntry` is confirmed and the Steam-ID question
   below is decided.
3. **Uploads** only after a flow source exists and with a separate
   planned experiment (one live write, maintainer's OK before the run).

**Proposed:** option 1 now, option 2 as the next loop work, option 3 only
with a source.

## Questions for the maintainer

1. **When to fetch achievement data:** at every shim-mode launch (one
   extra Valve request for games without stats), or only when the game
   asks and let the game wait, or keep the public part of the schema on
   disk between runs, or keep stopping these games?
2. **Shadow comparison of achievement names:** may the proxy compare the
   official `GetAchievementName` texts and count with Reborn's (match or
   mismatch only, names never logged), as an addition to ADR 0055's
   stage-1 list?
3. **The third game:** only two recorded games call positions 14 and 15;
   record a third first (as for `StoreStats`), or accept two?
4. **Leaderboard reads:** may `rebornd` ask Valve for leaderboards on
   the game's behalf (one find per name and run, cached for the run;
   Fallout Shelter would cause 13 finds per start)? And may one live find
   from the test account be recorded as the fixture?
5. **Other people's scores:** `DownloadLeaderboardEntriesForUsers` sends
   Steam IDs the game chose and returns those users' scores. Forward any
   IDs the game passes (the page takes "a maximum of 100 users"), or only
   the user's own and friends', or none until decided?
6. **Score uploads:** agree that `UploadLeaderboardScore` stays stopped
   until a flow source exists, and that the first live upload is a
   separate experiment you approve right before the run?

## Consequences

- If accepted as proposed: Outcore and One's Own Exile get a way past
  position 14 once the stats object, `RequestCurrentStats` and 0x44d
  exist in the shim and the shadow comparison has matched in three games.
- Leaderboard games (Fallout Shelter, Pixel Puzzles, Coin Maker
  Simulator) keep stopping at their first find for now.
- Nothing in this ADR adds a write to the account.
