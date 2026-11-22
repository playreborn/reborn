# M4 game list

M4 is done when "a list of at least 20 widespread single-player games,
fixed in advance, reaches level 3 on a system without the official
client installed" (`docs/plan.md`, M4; levels in § 8.5: level 3 = starts
without the official client, single-player playable, plus achievements
and cloud). The list was fixed by the maintainer on 2026-11-22 (below).

## The fixed list (decided by the maintainer, 2026-11-22)

These 20 games are M4's list. All are 64-bit Windows games owned by the
test account, recorded with the observation proxy (`coverage.md`), with
single-player listed on their store pages and no anti-cheat named.

1. Cave Crawlers (2143030)
2. Bongo Cat (3419430)
3. Coin Maker Simulator Demo (5191630)
4. Linsips (4301370)
5. Baldi's Basics Classic Remastered (1712830)
6. Paper Lily - Chapter 1 (2220360)
7. Please, Touch The Artwork 2 (2600140)
8. An Arcade Full of Cats (2368470)
9. Gravitas (1067310)
10. Outcore: Desktop Adventure (1275670)
11. Plant Therapy (2505120)
12. Chronigma (4051150)
13. The Thief, the Witch, the Toad, and the Mushroom. (2233770)
14. One's Own Exile (3086720)
15. Fallout Shelter (588430)
16. HoloCure - Save the Fans! (2420510)
17. Dark Deception (332950)
18. UNBEATABLE [white label] (1290490)
19. A Park Full of Cats (2610090)
20. Cartapli : Fold Quest (4314560)

Not on it: How Fish Is Made (uses no Steam calls up to its menu, so it
does not test the shim), and the 32-bit games, which belong to M5. All
are free games; widely played paid games could join later through
Steam Families, which the maintainer has not set up.

The candidate table below stays as the record of how the list was
found.

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
| 1854430 | How Fish Is Made | yes | not listed | not listed | 750 MB | reached its main menu with the proxy armed but loaded no Steam library at all (agent run, 2026-11-22); whether it uses Steam later needs playing |
| 1712830 | Baldi's Basics Classic Remastered | added | yes | yes | 600 MB | recorded (stage 3, agent) |
| 332950 | Dark Deception | added | yes | yes | 1 GB (7.9 GB download) | recorded (stage 3, agent; `SteamClient019`) after the maintainer answered the first-start Windows prompt |
| 1290490 | UNBEATABLE [white label] | added | yes | yes | 2 GB | recorded (stage 3, agent) |
| 2610090 | A Park Full of Cats | added | yes | yes | 600 MB | recorded (stage 3, agent); its very first start after installing ended by itself after about 0.1 s of Steam calls, three later runs (one without, two with `GetAppID` answered by Reborn) ran normally; cause not known |
| 4314560 | Cartapli : Fold Quest | added | yes | yes | 1 GB | recorded (stage 3, agent) |
| 897330 | Coloring Pixels | added | yes | yes | 500 MB | 32-bit (`steam_api.dll`), so for M5; its settings window was clicked through by the agent (ADR 0049) |
| 588430 | Fallout Shelter | added | yes | yes | 2 GB | EULA accepted by the maintainer; recorded (stage 3, agent) |
| 319630 | Life is Strange - Episode 1 | added | yes | yes | 5 GB | EULA accepted by the maintainer; 32-bit, so for M5 |
| 2420510 | HoloCure - Save the Fans! | added | yes | yes | 250 MB | EULA accepted by the maintainer; recorded (stage 3, agent) |
| 2220360 | Paper Lily - Chapter 1 | added | yes | yes | 350 MB | recorded (stage 3, agent) |
| 2600140 | Please, Touch The Artwork 2 | added | yes | yes | 3 GB | recorded (stage 3, agent) |
| 2368470 | An Arcade Full of Cats | added | yes | yes | 350 MB | recorded (stage 3, agent) |
| 310360 | Higurashi When They Cry Hou - Ch.1 | added | yes | yes | 500 MB | 32-bit (`steam_api.dll`), so for M5 |
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

With these, 21 candidates are listed: 20 recorded by now; How Fish Is Made uses no Steam calls up to its menu; Coloring Pixels, Life is Strange and Higurashi are 32-bit (M5). The maintainer picks the final 20. Free games are a practical start for agent
runs; the plan asks for widespread games, so the maintainer may want
paid single-player games on the final list too.

## Order of work

1. Stage 3 runs (start to menu and quit) of every candidate, by the
   agent, to see which interface versions and positions each one uses.
2. From that, the positions most games share go first into
   `meanings.md` (candidates, experiments) and `coverage.md`.
3. Runs that need playing (achievement or cloud experiments) are
   collected for the maintainer.
