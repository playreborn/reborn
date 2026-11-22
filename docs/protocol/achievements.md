# Achievements

Reading the signed-in account's achievements for one game (ADR 0029).
Reborn never writes stats or achievements (red line 4).

### Get user stats

- **What:** The game's stats schema and the account's stats and unlock
  times.
- **Message:** `CMsgClientGetUserStats` (EMsg 818), sent as a job, with
  `game_id` (the app ID for a plain app), `schema_local_version` -1 and
  `crc_stats` 0 (no local copy, so Steam sends everything), and
  `steam_id_for_user` the own SteamID. Answer
  `CMsgClientGetUserStatsResponse` (EMsg 819) with `game_id`, `eresult`,
  `crc_stats`, `schema`, `stats` (`stat_id`, `stat_value`) and
  `achievement_blocks` (`achievement_id`, repeated `unlock_time`).
- **Checks:** `eresult` 1 (OK), `game_id` equals the request.
- **Source:** the messages and the binary KeyValues guess: SteamKit issue
  #531 (maintainer comment). Request values: Goldberg emulator
  `scripts/generate_emu_config.py`, `get_stats_schema` (LGPL, read only).
  EMsg numbers and fields: steam-vent-proto-steam `enums_clientserver.rs`,
  `steammessages_clientserver_userstats.rs`.
- **Verified:** yes (2026-11-20, first test account: app 3419430 sent
  EResult 1, a 24 KB schema, 10 stats and one achievement block; app 480
  also answered. Fixture `user_stats.bin`).

### Schema

- **What:** Binary KeyValues. The root holds one object named after the
  app ID with a `stats` object. Each child of `stats` is one stat, keyed
  by its stat ID. Stats with a `bits` object hold achievements: each child
  of `bits`, keyed by the bit number, is one achievement with `name` (the
  internal name) and `display` (`name`, `desc`, `hidden`, `icon`,
  `icon_gray`). `display/name` and `display/desc` are either a string or
  one string per language (`english`, `german`, ...).
- **Stat types** (`type`): 1 integer, 2 float, 3 average rate,
  4 achievements, 5 group achievements. Reborn does not rely on them; it
  takes every stat with `bits`.
- **Source:** Steam Achievement Manager `SAM.Game/Manager.cs`
  (`LoadUserGameStatsSchema`) and `SAM.API/Types/UserStatType.cs` (zlib,
  read only).
- **Verified:** yes (2026-11-20, `user_stats.bin`): the layout matches.
  Observed details: `type` is a string here (`ACHIEVEMENTS`); stat 1 has
  an empty `bits` object, stat 2 holds all 28 achievements; `display/name`
  and `display/desc` are objects with `english` and `token`; `hidden` is
  an int32.

### Unlock state

- **What:** For each achievement, the entry of `achievement_blocks` whose
  `achievement_id` is the stat ID; `unlock_time[bit]` is the unlock time in
  seconds since the Unix epoch, 0 while locked.
- **Source:** own recording (`user_stats.bin`): one block with
  `achievement_id` 2 and 32 unlock times; times at bits 0 to 3, the rest 0.
  The value of stat 2 is 15, so its bits mark the same four achievements.
- **Verified:** yes (2026-11-20): `reborn-cli achievements` showed 4 of 28
  unlocked with the same achievements and times as the official client
  for the first test account.

### Typed read for the shim (ADR 0055)

- **What:** `Session::user_stats(app_id)` sends the same request as
  "Get user stats" above and returns `UserStats`: `result` (the
  `eresult`), `crc_stats`, the raw `schema` bytes, `stats` (`stat_id`,
  raw `stat_value`) and `achievement_blocks` (`achievement_id`,
  `unlock_time`). `UserStats::achievements` builds the achievement list
  from it as described under "Schema" and "Unlock state".
  `user_stats_from_bytes` reads a recorded answer for replay tests.
- **Checks:** a missing `eresult` is an error. An OK answer must carry
  the asked `game_id`; an answer that is not OK is returned typed (it is
  Valve's real answer) and may lack `game_id`, but must not name another
  game. Entries without their ID, and stats without a value, are
  skipped; nothing is filled in.
- **Not interpreted:** what a stat value means. The schema's `type` says
  it (see "Stat types"), but only for achievement stats is the meaning
  observed (bit n set while achievement n is unlocked, "Unlock state").
  Float stats and how `crc_stats` is computed are not sourced; both are
  passed on as Valve sent them.
- **Source:** the request and fields as above (SteamKit issue #531,
  steam-vent-proto-steam `steammessages_clientserver_userstats.rs`).
- **Verified:** replay against `user_stats.bin` (app 3419430): EResult 1,
  `crc_stats` present, a 24 158-byte schema, 10 stat values (stat 2,
  the achievement stat, is 15), one achievement block for stat 2 with 32
  unlock times; the typed list equals the direct one (28 achievements).

### Stats for the shim (`rebornd`, ADR 0055 stage 0)

- **What:** the shim's `RequestStats` (shim protocol 0.2) is answered for
  the session's own app only: `Accepted`, then the event `StatsReceived`
  with Valve's `eresult` and, when it is OK, each achievement's internal
  name and English display name (`FALLBACK_LANGUAGE`). No stat values or
  unlock state go to the shim while the readers are candidates.
- **Rules:** a connection without a session, or a run not started in
  shim mode, gets `NotAllowed`; a daemon that is signed out for good
  (the user signed out, Steam ended the session, or signing in again was
  refused) gets `NotSignedIn`. A session only forgotten after an error
  counts as offline, not signed out. This is tracked in the daemon, not
  read from the keyring. At most one `ClientGetUserStats` per run;
  later requests are answered from the run's in-memory cache (dropped
  when the game exits). A request that has not reached Valve waits while
  `rebornd` reconnects (ADR 0025), also when the connection is already
  lost at the request: it is accepted and fetched once a session exists,
  woken as soon as one is there (at most the backoff below between
  checks). It never signs in itself, and never waits for the session
  lock that a running sign-in holds, so the connection's other requests
  (ownership) are not held up. Network failures back off from 1 s,
  doubling, at most 5 minutes, and give up after 5 tries. The fetch holds
  the run only weakly: once the game has exited, no further request goes
  to Valve. A request that gives up, or an answer Reborn cannot read,
  sends **no** event (no invented result); the run's stats are then
  failed, and later requests get `Unavailable` (`Offline`, or
  `NotSignedIn` after a sign-out) without asking Valve again. Which
  reason fits an unreadable answer is not decided by ADR 0055; `Offline`
  is used for now. A connection may wait on at most 16 accepted stats
  requests at once; a shim with more is closed.
- **Logs:** only the result and counts, never names or values; `Debug`
  of `UserStats` shows only counts.
- **Source:** ADR 0055 ("Protocol messages", "Rate limits", "Offline, not
  signed in, daemon gone"); the Valve side is the request above.
- **Verified:** unit tests in `apps/rebornd` with a fake Valve session
  answering with `user_stats.bin`, including a served in-memory
  connection (event after `Accepted`, clean close); no live run. The shim
  does not send `RequestStats` yet.
