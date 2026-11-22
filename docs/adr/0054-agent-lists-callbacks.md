# 0054: The agent lists callback numbers itself

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22
- **Amends:** ADR 0050

## Context

ADR 0050 lets the observation proxy log the first 64 bytes of a
callback's data only for callback numbers on a fixed list
(`PAYLOAD_LISTED`). An entry needs a candidate, an experiment plan in
`docs/shim/meanings.md` and the maintainer's OK; callbacks that carry
the test account's own Steam ID may be listed, with the value kept out of
`docs/shim/`. ADR 0053 already let the agent add proxy reads itself; the
first callback experiment (loop iteration 9) needs two numbers listed.

## Decision

The agent may add a number to `PAYLOAD_LISTED` itself, under the same
conditions as ADR 0053:

- the number is named in an experiment plan in `docs/shim/meanings.md`,
  committed before any run that uses it;
- no candidate for the callback could carry a ticket, a token or a
  password; a callback that carries the test account's own Steam ID may
  be listed, and its value never goes into `docs/shim/`, a commit or a
  prompt (only "match" or "mismatch");
- the review pass of ADR 0051 checks each new number against these two
  points, and the pull request lists the new numbers.

Raw logs stay on the recorder's computer, as before.

## Decided by the maintainer (2026-11-22)

Asked whether to approve the first two numbers only or to let the agent
decide with a review pass, the maintainer answered "Künftig immer
freigeben" (always approve from now on).

## Consequences

- Callback experiments no longer wait for the maintainer.
- Callbacks that could carry credentials still need their own ADR and
  the maintainer.
