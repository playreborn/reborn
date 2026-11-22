# Recording candidates

These are free games to **record** with the observation proxy, picked
because they will probably call interfaces that only 1 or 2 of the 20
games in `coverage.md` call today. They are **not** proposed for the
fixed M4 list (`m4-games.md`). Recording one follows ADR 0044, 0045,
0049 and 0052 like any other run. This file is only a proposal: no
license was added, nothing was installed or started, and Steam and the
registry were not touched.

## Method

- Store pages read on 2026-11-22, one page at a time (19 store fetches
  in total: two filtered store searches for free single-player games
  with Steam Workshop or stats, then 17 app pages, plus two
  web searches). No sign-in, no scraping loop, no SDK file.
- Evidence comes only from the store page: the feature list (Steam
  Workshop, Steam Cloud, Steam Leaderboards, Stats, Steam Achievements),
  the DLC list, the language list, a "View In-Game Items" link, and the
  description. Steam Trading Cards were not counted as evidence. A
  feature on the store page makes a call **likely**, not certain. Which
  interface *version* a game asks for, and which positions it calls,
  only shows in a recording.
- Hard limits applied: free (free to play or free game; paid DLC is
  fine as long as the game is free), single-player listed, no anti-cheat
  named, at most 10 GB, Windows, not in `tools/shim/games.json`.
- Whether a game is 64-bit shows only after installing it
  (`steam_api64.dll`, `m4-games.md`). A game turning out 32-bit is
  noted for M5, as before.
- "EULA" or "3rd-party account" means the store page says so. Under
  ADR 0044 and 0049 the maintainer accepts EULAs and decides on
  third-party accounts, not the agent.

Left out after reading their pages: Alien Swarm: Reactive Drop
(563560; Steam inventory items, Workshop, leaderboards, but VAC is
listed), Lynium: Play the Rhythm (1415010; needs a third-party Lynium
account and a EULA), Iron Snout (424280; leaderboards and stats, but a
Windows XP era game that is likely 32-bit), Moonring (2373630; only
Steam Cloud and one DLC). Idle and item-drop games (Banana and similar)
were left out on purpose: their purpose is item drops, close to red
line 5, and Bongo Cat already covers `STEAMINVENTORY_INTERFACE_V003`.
Games from the search with VAC, MMO features or online-only play were
skipped without reading their pages.

## Candidates, by expected value

| # | App | Name | Store page evidence | Min. disk | EULA / account / anti-cheat | Gap rows it may help with |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 1920960 | VPet-Simulator | Workshop (items, themes, plugins), Steam Cloud, Steam Leaderboards, Stats, Achievements, 3 DLC (2 free), Online Co-op | 1 GB | none listed | `STEAMUGC_INTERFACE_VERSION016` 67, `STEAMREMOTESTORAGE_*`, `STEAMUSERSTATS_*` (leaderboards), `STEAMAPPS_INTERFACE_VERSION008` 6, `SteamFriends*` |
| 2 | 476460 | Picross Touch | Workshop ("full in-game workshop support"), Steam Cloud, Steam Leaderboards, Achievements, 5 paid DLC | 100 MB | EULA (3rd-party) | UGC 67, RemoteStorage 014/016, UserStats leaderboard rows, Apps 6 |
| 3 | 434520 | Simutrans | Workshop, Steam Cloud, Achievements, 7 free DLC (pak sets), 19 interface languages | 500 MB | EULA (3rd-party) | UGC 67, RemoteStorage, Apps 4 (language) and 6 (DLC) |
| 4 | 444930 | Zaccaria Pinball | Steam Leaderboards (per table and mode), Steam Cloud, Achievements, 129 DLC, 8 languages, "challenge your friends" | 3 GB, 64-bit | none listed; has online PvP, record single-player only | UserStats leaderboard rows, Apps 4 and 6, `SteamFriends017` 36/37 (avatars, user info on boards) |
| 5 | 351030 | Pixel Puzzles Ultimate Jigsaw Puzzles | Steam Leaderboards, Stats, Steam Cloud, Achievements, 174 DLC (5 free), 6 languages | 300 MB | none listed | Apps 6 (DLC checks), UserStats, RemoteStorage |
| 6 | 742500 | Nonogram - Master's Legacy | Workshop ("share works with friends"), Steam Cloud, Achievements, 3 paid DLC, 13 languages | not stated | none listed | UGC 67, RemoteStorage, Apps 4 and 6 |
| 7 | 1036950 | Super Jigsaw Puzzle: Generations | Steam Leaderboards, Stats, Steam Cloud, Achievements, 197 DLC (3 free), 12 languages | 650 MB | none listed | Apps 4 and 6, UserStats, RemoteStorage |
| 8 | 607260 | McOsu | Workshop (skins), 3 paid DLC (donations) | 75 MB | none listed; no osu! account needed | UGC 67, Apps 6 |
| 9 | 2398450 | Classic Marathon | Workshop, Steam Cloud, Achievements | 100 MB, 64-bit | none listed; has online co-op/PvP, record single-player only | UGC 67, RemoteStorage, UserStats 013 rows |
| 10 | 2093940 | vivid/stasis | Steam Leaderboards, Steam Cloud, Achievements | 4 GB, 64-bit | none listed | UserStats leaderboard rows, RemoteStorage, Friends (leaderboard names) |
| 11 | 1653970 | Jigsaw Puzzle Dreams | Workshop (custom puzzles), Steam Cloud, 6 paid DLC, Online Co-op | 4 GB, 64-bit | none listed | UGC 67, RemoteStorage, Apps 6 |
| 12 | 599390 | Battle for Wesnoth | Steam Cloud, 16 languages | 800 MB, 64-bit | none listed; has online PvP, record single-player only | RemoteStorage 014/016, Apps 4 |
| 13 | 417860 | Emily is Away | 4 in-game items ("View In-Game Items"), Achievements | not stated | none listed; Windows XP era, may be 32-bit (then M5) | `STEAMINVENTORY_INTERFACE_V003` rows (2, 5, 11, 13, 20, 21, 26) if it uses the inventory |
| 14 | 980610 | Quaver | Workshop (maps, skins), Achievements, online leaderboards | 150 MB, 64-bit | **3rd-party account** (Quaver, links to the Steam account): the maintainer decides; record only up to a screen reachable without it | UGC 67, Friends |
| 15 | 1902490 | Aperture Desk Job | 28 languages, Source 2 engine | 4 GB, 64-bit | none listed | Apps 4 (language), a different engine's `SteamClient` use |

## Notes

- **Expected value** weighs how many open rows a game may touch, how
  strong the store evidence is, and how cheap the run is (size, no
  EULA). VPet-Simulator, Picross Touch and Simutrans each list Workshop
  and Cloud together, the two interfaces with the fewest games today.
- **ISteamScreenshots** (`STEAMSCREENSHOTS_INTERFACE_VERSION003` 3): no
  free candidate with store-page evidence was found. Store pages do not
  list screenshot integration; one web search found no free
  single-player game known to hand photos to Steam.
- **ISteamInventory**: only Emily is Away has evidence (an item
  store), and whether it uses the inventory interface or another way to
  sell items only shows in a recording.
- **ISteamUser voice** (`SteamUser020` 12): no store page names voice
  chat in a single-player game; none proposed for it.
- **Many DLC** (Pixel Puzzles, Super Jigsaw Puzzle, Zaccaria Pinball)
  makes DLC queries (`STEAMAPPS_INTERFACE_VERSION008`) likely at start.
  The test account owns none of the paid DLC; ownership is never faked
  (red line 3), so the recording shows the "not owned" answers.
- **Open-source games** (Simutrans, Classic Marathon, Battle for
  Wesnoth, McOsu, Quaver): their Steam integration source is **not** a
  source for the shim and must not be read; such repositories may
  include or vendor Steamworks SDK files (red line 6, ADR 0012).
  Meanings still come only from recordings and Valve's public
  documentation (`docs/shim/`).
- **Multiplayer features** on a page (Classic Marathon, Zaccaria
  Pinball, Wesnoth, VPet-Simulator, Jigsaw Puzzle Dreams) do not rule a
  game out, but runs stay single-player (ADR 0049: no online play).
- Leaderboard uploads happen only when the game itself records play
  (red line 4); a start-to-menu run (stage 3) should not upload
  anything. Runs that need playing to reach leaderboard or Workshop
  calls are planned as experiments first.

Sources: the Steam store pages of the apps above
(`https://store.steampowered.com/app/<app id>/`), read 2026-11-22.
