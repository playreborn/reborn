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
| 4301370 | Linsips | yes | yes | yes | 4 MB | |
| 3419430 | Bongo Cat | yes | yes | not listed | 100 MB | recorded (stage 3) |
| 5191630 | Coin Maker Simulator Demo | yes | not listed | yes | 3 GB | recorded (stage 3) |
| 1854430 | How Fish Is Made | yes | not listed | not listed | 750 MB | level 3 trivially; keep as a start check |
| 1712830 | Baldi's Basics Classic Remastered | free | yes | yes | 600 MB | |
| 332950 | Dark Deception | free | yes | yes | 1 GB | |
| 897330 | Coloring Pixels | free | yes | yes | 500 MB | |
| 588430 | Fallout Shelter | free | yes | yes | 2 GB | EULA |
| 319630 | Life is Strange - Episode 1 | free | yes | yes | 5 GB | EULA |
| 2420510 | HoloCure - Save the Fans! | free | yes | yes | 250 MB | EULA |

Left out: AION 2 (3393110, owned; online game) and Quatro! (2158270,
owned; online PvP, no single-player listed).

Nine places are still open. Free games are a practical start for agent
runs; the plan asks for widespread games, so the maintainer may want
paid single-player games on the final list too.

## Order of work

1. Stage 3 runs (start to menu and quit) of every candidate, by the
   agent, to see which interface versions and positions each one uses.
2. From that, the positions most games share go first into
   `meanings.md` (candidates, experiments) and `coverage.md`.
3. Runs that need playing (achievement or cloud experiments) are
   collected for the maintainer.
