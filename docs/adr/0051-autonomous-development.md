# 0051: Autonomous development of the shim

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22
- **Amends:** ADR 0042, ADR 0044, AGENTS.md sections 6 and 7

## Context

Since ADR 0044 an AI agent records games, plans and runs experiments and
writes code for the shim on the maintainer's computer. Every result
still waited for the maintainer: each confirmed meaning (ADR 0042, 0044),
each ADR, each merge of code that AGENTS.md section 7 marks for human
review. The maintainer cannot read the code, has reviewed none of it
line by line, and asked on 2026-11-22 for the work to run on its own,
around the clock, as other reverse-engineering projects do with agents
(for example AnyPS5, whose `AGENTS.md` makes agents run fixed tools
before starting a function and before pushing, and records every unknown
function in a gap list).

What slows the work is the waiting, not the red lines: the sources of
knowledge (recordings, Valve's public pages, planned experiments) stay
the same.

## Decision

The agent works autonomously within these rules. The red lines
(AGENTS.md section 2) and the shim's clean-room rules (section 4) do not
change.

### What the agent decides itself

- **Meanings.** A meaning is confirmed without the maintainer when its
  experiment plan was committed before the runs, its predictions are
  checked by a script in the repository, and all predictions matched.
  A mismatch keeps the candidate, as before.
- **ADRs.** The agent writes and accepts ADRs itself, except ADRs that
  touch a red line, credentials or tokens, or that read account data
  beyond ADR 0043 and 0050; those wait for the maintainer.
- **Merging.** The agent merges its own pull requests, including
  `unsafe` code and the shim, when: CI is green (fmt, clippy, tests,
  docs, deny), CodeQL reports nothing new, and a separate review pass
  (a fresh agent that did not write the change, reading only the diff and
  the rules) found no unresolved problem. This replaces the human review
  of AGENTS.md section 7 for the shim, the observation tools and
  `rebornd`'s shim parts. Changes to login, tokens, the keyring, or
  actions on the account (messages, friends, cloud uploads) still wait
  for the maintainer.

### How the work runs

- **One gap list.** `docs/shim/gaps.md` is generated from the recordings
  and `meanings.md`: every interface position seen, how many games call
  it, and its status. The loop takes the next task from it, most-called
  unknown position first.
- **Fixed tools.** Coverage, the gap list and runs of games come from
  scripts in `tools/shim/`, not from steps typed by hand.
- **A loop.** `docs/shim/LOOP.md` describes one iteration: pick a gap,
  find a candidate on a public page, commit an experiment plan, run it,
  evaluate it with a script, record the result, implement confirmed
  meanings in the shim (shadow mode first, ADR 0046), open a pull
  request, merge it under the rules above. A Claude Code session started
  by the maintainer can run this loop for hours or days.
- **Reporting.** `docs/shim/digest.md` gets one short entry per
  iteration (what was tried, what matched, what was merged), so the
  maintainer can read a day's work in a minute and stop anything.
- **Limits** of ADR 0044, 0045 and 0049 stay: at most 50 game starts a
  day, at least 2 minutes apart, no input for rewards, stop at EULAs,
  purchases, sign-ins and administrator prompts.

### The maintainer's role

- Can stop the loop at any time and veto anything after the fact; a
  vetoed change is reverted.
- Decides what is listed above as waiting for the maintainer.

## Consequences

- The shim advances without anyone waiting for approvals.
- Mistakes are caught by tests, CI, CodeQL and a second review pass
  instead of a person; some will get through and be fixed later. The
  digest and git history keep everything traceable and reversible.
- The rules about where knowledge comes from are unchanged, so every
  line of the shim can still be traced to a recording and a public page.
