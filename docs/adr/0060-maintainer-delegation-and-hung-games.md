# 0060: The maintainer's standing delegation, and games that do not close

- **Status:** Accepted (by the maintainer, 2026-11-23)
- **Date:** 2026-11-23
- **Amends:** ADR 0044, ADR 0051

## Context

The shim loop (ADR 0051) stopped many times to ask the maintainer: ADR
decisions, account-data questions, and games that did not close on the
close request (ADR 0044: "never a forced kill"; stop and report). Each
stop left the loop idle until the maintainer came back.

On 2026-11-23 the maintainer said, in substance: stop asking questions;
decide everything on my behalf to the best of your judgement, also about
account data; it is only a test account. When a game needs a click, the
agent clicks. When a game hangs, the agent ends it.

## Decision

1. **Standing delegation.** Where an ADR, `LOOP.md` or `AGENTS.md`
   section 7 asks for the maintainer's decision or OK, the agent decides
   itself. It writes the decision and its reasons into the ADR or plan
   as "decided by the agent under the maintainer's delegation of
   2026-11-23 (ADR 0060)". The maintainer can read and reverse any such
   decision at any time.
2. **What the delegation does not cover.** The red lines of `AGENTS.md`
   section 2 stay absolute; no delegation can lift them. The agent also
   still never buys anything, never accepts an agreement in the
   maintainer's name, never types a password or Steam Guard code, and
   never touches an account other than the test account. Where a red
   line is close, it takes the conservative option.
3. **Input in games.** ADR 0049 already lets the agent send input to a
   game during a run. It uses this to get past launchers, title screens
   and dialogs, and never to play in a way that unlocks achievements or
   changes stats (red line 4).
4. **Games that do not close.** If a game is still running two minutes
   after the normal close request, the agent may end it with a forced
   kill (`taskkill /F` on the game's process only, never on the official
   client or `rebornd`). The run notes it. This replaces ADR 0044's
   "never a forced kill" for agent runs. A forced kill can lose an
   unsaved local save of the test account; that is accepted.

## Consequences

- The loop no longer waits for the maintainer, except for actions that
  physically need them (a Steam Guard prompt, a purchase or an
  agreement), which stop the run as before.
- Decisions that used to be the maintainer's are now recorded with the
  agent as decider, so the record stays honest.
