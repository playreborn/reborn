# 0052: No daily cap on agent-run game starts

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22
- **Amends:** ADR 0045

## Context

ADR 0045 allows at most 50 game starts a day, at least 2 minutes apart.
With the autonomous loop (ADR 0051) a single experiment takes three to
six starts, and the cap is reached by the morning; the loop then has to
wait for the next day although nothing about a run changed.

## Decision

The daily cap of 50 game starts is removed. Everything else stays:

- at least **2 minutes** between two starts (the gap in `run.sh`);
- at most **20 free games added** to the test account a day;
- each run at most 10 minutes (30 for a named experiment), only for a
  game on the M4 list or one a planned experiment needs, never for play
  time, trading cards or other rewards (ADR 0044, 0049).

The gap alone bounds a day to at most 720 starts; in practice the loop's
plan, run, evaluate and review steps keep it far lower.

## Decided by the maintainer (2026-11-22)

The maintainer asked for the cap to be removed ("Ich will bitte den cap
entfernen von den 50 games").

## Consequences

- The loop no longer stops for the day after about ten experiments.
- More requests reach Valve's servers on busy days, still spread out by
  the 2-minute gap and tied to a planned purpose (red lines 5 and 8).
  If a run ever shows a rate limit or a warning from Valve, the agent
  stops starting games and asks the maintainer.
