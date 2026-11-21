# 0045: Higher limits for agent-run observation runs

- **Status:** Accepted
- **Date:** 2026-11-21
- **Amends:** ADR 0044

## Context

ADR 0044 lets an AI agent carry out observation runs with at most 10
game starts a day, at least 5 minutes apart, and at most 3 free games
added to the test account a day. On the first day the agent reached
both limits within about an hour: most runs are short (a game starts,
reaches its first screen and is closed within two minutes), and many
candidate games turned out to need a person anyway, so finding the
ones that do not takes many starts.

## Decision

The limits in ADR 0044, section "Limits" and "Before a run", change to:

- at most **50 game starts a day**, at least **2 minutes apart**;
- at most **20 free games added** to the test account a day.

Everything else in ADR 0044 stays: each run at most 10 minutes, never
for play time, trading cards or other rewards, no input to games, only
for games on the M4 list or needed for a planned experiment, and the
agent stops at any unexpected dialog.

## Decided by the maintainer (2026-11-21)

The maintainer chose these numbers out of three proposals (30/3/10,
50/2/20, or only more starts).

## Consequences

- More games can be checked in a day; the rate stays far below anything
  that looks like idling or farming (red line 5): short runs, a purpose
  for each, and no rewards sought.
- More requests reach Valve's servers per day than before, still spread
  out by the 2-minute gap (red line 8).
