# 0053: The agent adds proxy reads itself

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22
- **Amends:** ADR 0043, ADR 0051

## Context

ADR 0043 lets the observation proxy read a call's result text or the
first 16 bytes behind one argument, only for calls on a fixed list
(`READS`). Adding an entry needs a candidate and an experiment plan in
`docs/shim/meanings.md` and "the maintainer's OK in the pull request".
ADR 0051 lets the agent confirm meanings and merge its own pull requests,
but did not change that rule. In loop iteration 6 the agent added two
entries (`STEAMINVENTORY_INTERFACE_V003` positions 6 and 1) and used them
in runs before asking; the review pass found the missing OK.

## Decision

The agent may add a `READS` entry itself when all of these hold:

- the entry is named in an experiment plan in `docs/shim/meanings.md`,
  committed before any run that uses it;
- no candidate for the call could hand back a ticket, a token, a
  password or an account identifier (the rule of ADR 0043 is unchanged;
  such calls still need their own ADR and the maintainer);
- the review pass of ADR 0051 checks each new entry against these two
  points, and the pull request lists the new entries.

Everything else in ADR 0043 stays: 16 bytes or one text, only for logged
calls, raw logs stay local, account data is replaced before anything goes
into `docs/shim/`.

## Decided by the maintainer (2026-11-22)

Chosen from three options (decide alone with review, approve only this
pull request, or remove the entries). The two inventory entries of pull
request #16 are approved by this decision.

## Consequences

- An experiment that needs an output byte no longer waits for the
  maintainer.
- Reads that could touch account data still do.
