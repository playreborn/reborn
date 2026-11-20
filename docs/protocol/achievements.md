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
