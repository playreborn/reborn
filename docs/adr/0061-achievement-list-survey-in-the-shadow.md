# 0061: A set and index survey of the achievement-name shadow

- **Status:** Accepted (decided by the agent under the maintainer's
  delegation of 2026-11-23, ADR 0060)
- **Date:** 2026-11-23
- **Amends:** ADR 0059 decision 2 (what the achievement-name shadow may
  log), ADR 0055 ("What is cached, and where": achievement names are
  account data in logs, which say only counts)
- **Touches:** red line 3 only in that the data compared comes from the
  real account's session; nothing new is read from or written to the
  account.

## Context

ADR 0059 decision 2 lets the proxy compare, in a shadow session, the
official `GetNumAchievements` with the length of Reborn's achievement
list, and the official `GetAchievementName(i)` with Reborn's *i*-th
internal name, by SHA-256. Only "match" or "mismatch" is logged.

Experiment 35 (`docs/shim/meanings.md`, "First live runs of the
achievement-name shadow") ran it in Outcore and One's Own Exile. The
count matched in both. The name at index *i* differed from Reborn's
*i*-th name in 26 of 29 calls (Outcore) and 15 of 16 (One's Own Exile).
The cause is not known. The plan named four possibilities, none observed:
another order than Reborn's schema order, other contents of the list, a
stale schema on one side, or a different encoding of the same name.

"Match" or "mismatch" per index cannot tell these apart. Telling them
apart needs to compare the two lists without their order, and to say
where each official name sits in Reborn's list.

## Decision

Decided by the agent under the maintainer's delegation of 2026-11-23
(ADR 0060).

In a shadow session the proxy keeps, for each index the game asked for
(below a bound of 1024), the SHA-256 of the official text and the
SHA-256 of its **normalised form**: read as UTF-8, white space trimmed
at both ends, lowercased with Unicode's lowercase mapping. Reborn's
names get the same two digests. Once the game has asked for every index
below the official count, and Reborn's list is there, the proxy logs
three lines, once per process, after the call that completed the list:

- **(a) Set comparison:** the official count, how many distinct official
  digests there are and how many indices had no text, the length of
  Reborn's list and its distinct digests, and then three counts over the
  distinct digests: shared (an official digest that is also in Reborn's
  list), official-only and Reborn-only.
- **(b) Index mapping:** for each official index *i*, the first index *j*
  in Reborn's list with the same digest, or `none`, or `notext` when the
  official result pointed to no text.
- **(c) Normalised set comparison:** the same three counts as (a), over
  the normalised digests. It finds names that differ only in letter case
  or surrounding white space. It does not undo other encoding
  differences, such as a name in another code page.

The formats are fixed in `crates/shim-observe/src/daemon.rs`
(`AchievementSurvey`). No name and no digest is ever logged, sent to
`rebornd`, or written to `docs/shim/`. As before, the game always gets
the official client's answer.

A list longer than the proxy compares gets no survey: the proxy shadows
only logged calls (the first 50 of a position), so such a list never
completes. A count above 1024 gets one line that says so.

## Why this exposes no account data

- **What the lines hold:** counts, and pairs of indices into two lists
  of the app's achievement API names. No name, no digest, no stat value,
  no unlock state, no Steam ID.
- **The names are public schema data.** The schema is the same for every
  account that owns the game. Reborn gets it in `ClientGetUserStats`
  (`docs/protocol/achievements.md`), and the official client gets it from
  the same servers. ADR 0055 still treats the names as account data for
  logging, and this ADR keeps that rule: the names stay out of the log.
- **The indices are public positions.** An official index is the
  argument the game passed to `GetAchievementName`. A Reborn index is a
  position in the schema's own order (`reborn_cm::achievements`: every
  `bits` entry of every stat, in schema order). Both depend on the game's
  schema only, not on the account.
- **The counts** are the length of the schema's list and the sizes of
  its overlaps, the same for every account.
- **What the digests could reveal** if logged: a SHA-256 of a short API
  name can be reversed by guessing. That is why digests stay in memory
  only, as in ADR 0059 decision 2 and ADR 0056.

## Rules for using the result

- A survey describes, it does not decide. If the mapping shows "another
  order", the order is a **finding**. Reborn adopts an order hypothesis
  only with a source or an observation, never from memory (AGENTS.md
  section 4). A rule such as "sorted by name" or "by schema ID" is tested
  in a planned experiment before the shim uses it.
- Nothing in the shim, the proxy or `rebornd` is changed to make the
  comparison of ADR 0059 decision 2 match without a new plan.

## Consequences

- Experiment 36 can tell "order differs", "encoding or case differs" and
  "contents differ" apart (`docs/shim/meanings.md`).
- The proxy keeps at most 1024 pairs of digests (about 64 KiB) per
  process, and a second digest per Reborn name (at most 10,000, about
  320 KiB).
- No new message between the proxy and `rebornd`, no new Valve request
  (red line 8): the survey uses the list the comparison already fetched.
