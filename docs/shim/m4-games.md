# M4 game list (draft)

M4 is done when "a list of at least 20 widespread single-player games,
fixed in advance, reaches level 3 on a system without the official
client installed" (`docs/plan.md`, M4; levels in § 8.5: level 3 = starts
without the official client, single-player playable, plus achievements
and cloud). This file is that list while it is being fixed. It is a
**draft**: the maintainer fixes the final list.

## Criteria

A game goes on the list only if all of these hold:

1. **Single-player** is listed on its store page.
2. **No anti-cheat** is named on its store page. Games with anti-cheat
   are never run with the observation proxy: a foreign DLL in such a
   process could put the account at risk.
3. **Small enough** for agent runs: at most 10 GB (ADR 0044).
4a. **64-bit on Windows.** M4 covers 64-bit Windows and Linux; 32-bit
   Windows comes with M5 (`docs/plan.md`). Whether a game is 32-bit
   shows only after installing it (`steam_api.dll` without `64`).
4. **Owned by the test account**, or free so that it can be added
   (ADR 0044, at most 3 a day).
5. Ideally **Steam Achievements and Steam Cloud** are listed, so that
   level 3 means something for it.
6. A game that does not start with the proxy, or later with the shim,
   stays unsupported and is replaced. DRM is never looked at or worked
   around (red line 1); whether a game has DRM is not checked, only
   whether it starts.

The store page facts below were read on 2026-11-21, one page at a time.
"EULA" means the page says the game needs agreement to a third-party
EULA; under ADR 0044 the maintainer accepts it, not the agent.

## Candidates

| App | Name | Owned | Achievements | Cloud | Min. disk | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| 2143030 | Cave Crawlers | yes | yes | yes | 1 GB | recorded (stages 3, 4) |
| 4301370 | Linsips | yes | yes | yes | 4 MB | recorded (stage 3, agent) |
| 3419430 | Bongo Cat | yes | yes | not listed | 100 MB | recorded (stage 3) |
| 5191630 | Coin Maker Simulator Demo | yes | not listed | yes | 3 GB | recorded (stage 3) |
| 1854430 | How Fish Is Made | yes | not listed | not listed | 750 MB | loads Steam only after a language screen; a run needs a person |
| 1712830 | Baldi's Basics Classic Remastered | added | yes | yes | 600 MB | recorded (stage 3, agent) |
| 332950 | Dark Deception | added | yes | yes | 1 GB (7.9 GB download) | first start asks for administrator rights (Windows dialog for the official client's install step); left for the maintainer |
| 897330 | Coloring Pixels | added | yes | yes | 500 MB | shows its own settings window with a Play button before loading Steam; a run needs a person |
| 588430 | Fallout Shelter | added | yes | yes | 2 GB | EULA accepted by the maintainer; recorded (stage 3, agent) |
| 319630 | Life is Strange - Episode 1 | added | yes | yes | 5 GB | EULA accepted by the maintainer; 32-bit, so for M5 |
| 2420510 | HoloCure - Save the Fans! | added | yes | yes | 250 MB | EULA accepted by the maintainer; recorded (stage 3, agent) |
| 2220360 | Paper Lily - Chapter 1 | added | yes | yes | 350 MB | recorded (stage 3, agent) |
| 2600140 | Please, Touch The Artwork 2 | added | yes | yes | 3 GB | recorded (stage 3, agent) |
| 2368470 | An Arcade Full of Cats | added | yes | yes | 350 MB | recorded (stage 3, agent) |
| 310360 | Higurashi When They Cry Hou - Ch.1 | added | yes | yes | 500 MB | asks for a language before loading Steam; a run needs a person |
| 1067310 | Gravitas | added | yes | yes | 8 GB (6.9 GB download) | recorded (stage 3, agent); a launcher process starts the game process, and the game printed no `[S_API]` line |
| 1275670 | Outcore: Desktop Adventure | added | yes | yes | 2 GB | recorded (stage 3, agent) |
| 2505120 | Plant Therapy | added | yes | yes | 900 MB | recorded (stage 3, agent) |
| 4051150 | Chronigma | added | yes | yes | 2 GB | recorded (stage 3, agent) |
| 2233770 | The Thief, the Witch, the Toad, and the Mushroom. | added | yes | yes | 350 MB | recorded (stage 3, agent) |
| 3086720 | One's Own Exile | added | yes | yes | 150 MB | recorded (stage 3, agent) |

Left out: AION 2 (3393110, owned; online game) and Quatro! (2158270,
owned; online PvP, no single-player listed). Added and installed, but
32-bit, so for M5: Katawa Shoujo (3068300), SCP – Containment Breach
(2178380), Travellin Cats in Paris (2313250), Cube Escape: Paradox
(909080).

With these, 21 candidates are listed: 16 recorded by now, four that need a person for a run, and Life is Strange, which is 32-bit (M5). The maintainer picks the final 20. Free games are a practical start for agent
runs; the plan asks for widespread games, so the maintainer may want
paid single-player games on the final list too.

## Order of work

1. Stage 3 runs (start to menu and quit) of every candidate, by the
   agent, to see which interface versions and positions each one uses.
2. From that, the positions most games share go first into
   `meanings.md` (candidates, experiments) and `coverage.md`.
3. Runs that need playing (achievement or cloud experiments) are
   collected for the maintainer.
