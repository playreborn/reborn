# 0056: Call results, the persona name, and recordings of other games

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22
- **Amends:** ADR 0043, ADR 0053, ADR 0055

## Context

Planning experiments 26 and 27 (loop iteration 13a) left three questions
for the maintainer:

1. `SteamUtils010` position 13 (candidate `GetAPICallResult`) fills the
   game's buffer with the result of an asynchronous call. Some call
   results can name a user, so a `READS` entry for it touches account
   data, which ADR 0053 leaves to the maintainer.
2. `SteamFriends017` position 0 is narrowed to `GetPersonaName` or
   `ClearRichPresence`; only the returned text, the account's persona
   name, tells them apart. ADR 0043 does not let the proxy read it.
3. `docs/shim/coverage.md` and `gaps.md` count only the 20 games of the
   M4 list. Recordings of other free games (experiment 25 and later)
   call positions the M4 games do not.

## Decision

1. The proxy may read the bytes a call writes into the game's buffer for
   call results (`SteamUtils010` 13 and later the same for other
   versions), under ADR 0053's rules otherwise: the entry is named in an
   experiment plan committed before the runs, raw logs stay on the
   recorder's computer, and values that may be account data never go
   into `docs/shim/` (only match or mismatch).
2. The persona name may be compared: `rebornd` fetches the account's
   persona name (from its own session) and a shadow comparison tells
   whether the text the official client returns at `SteamFriends017`
   position 0 is the same. Only "match" or "mismatch" is logged; the
   name itself is never written to a log, a file in the repository or a
   prompt.
3. Recordings of games outside the M4 list count in the coverage and gap
   lists. `tools/shim/games.json` gets them as extra entries marked as
   not on the M4 list; M4's own goal (the 20 games of `m4-games.md`)
   does not change.

Tickets, tokens and passwords are still never read (ADR 0043).

## Decided by the maintainer (2026-11-22)

Asked about the three questions, the maintainer answered: "Ja die Proxy
darf mitlesen. Ja er darf den Anzeigenamen vergleichen. Ja einfließen."

## Consequences

- `GetAPICallResult` and `GetPersonaName` can be confirmed by experiment.
- The gap list grows with the extra games; percentages in
  `docs/shim/PROGRESS.md` say which set they count.
