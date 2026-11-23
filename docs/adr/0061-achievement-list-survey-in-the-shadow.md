# 0061: A set and index survey of the achievement-name shadow

- **Status:** Accepted (decided by the agent under the maintainer's
  delegation of 2026-11-23, ADR 0060)
- **Date:** 2026-11-23
- **Amends:** ADR 0059 decision 2 (what the achievement-name shadow may
  log) and ADR 0055 ("What is cached, and where": achievement names are
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
(below 50, the proxy's logging limit), the SHA-256 of the official text
and the SHA-256 of its **normalised form**: read as UTF-8, white space
trimmed at both ends, lowercased with Unicode's lowercase mapping.
Reborn's names get the same two digests. Once the game has asked for
every index below the official count, and Reborn's list is there, the
proxy logs three lines, once per process, after the call that completed
the list:

- **(a) Set comparison:** the official count, how many distinct official
  digests there are and how many indices had no text, the length of
  Reborn's list and its distinct digests, and then three counts over the
  distinct digests: shared (an official digest that is also in Reborn's
  list), official-only and Reborn-only.
- **(b) Normalised set comparison:** the same three counts as (a), over
  the normalised digests. It finds names that differ only in letter case
  or surrounding white space. It does not undo other encoding
  differences, such as a name in another code page. Two names that
  differ only in this way count as one.
- **(c) Index mapping:** for each official index *i*, the first index *j*
  in Reborn's list with the same digest, or `none`, or `notext` when the
  official result pointed to no text.

When no survey can be made, one line says why instead
(`shadow survey: not surveyed, <why>`): an official answer was skipped,
there is no Reborn list when the official list is complete, or the
official count is above 50 (only the first 50 calls of a position are
shadowed, so such a list can never be complete).

The formats are fixed in `crates/shim-observe/src/daemon.rs`
(`AchievementSurvey`). No name and no digest is ever logged, sent to
`rebornd`, or written to `docs/shim/`. As before, the game always gets
the official client's answer.

### Where each part may go

- **(a) and (b), the counts, and the "not surveyed" reasons** may go
  into `docs/shim/`, like the per-call match and mismatch counts.
- **(c), the index pairs *i*:*j*,** stay in the local log and in the
  output of the experiment script on the recorder's computer. They are
  **not** written into `docs/shim/`, a commit, a pull request or a
  prompt, until an experiment has shown that the official order does not
  depend on the account (next section). A summary of the mapping that
  holds only counts and yes/no answers may go into `docs/shim/`: the
  number of fixed points, whether it is the reversal (yes or no),
  whether it is a rotation (yes or no), and the number of cycles.

## What the parts may reveal

- **The counts** are lengths and overlap sizes of two lists of the app's
  achievement API names.
- **The official indices** *i* are the arguments the game passed to
  `GetAchievementName`: 0 to *N*-1, nothing more.
- **Reborn's indices** *j* are positions in Reborn's order
  (`reborn_cm::achievements`: every `bits` entry of every stat, in the
  order of the schema Valve sent to the test account).
- **The mapping** *i* → *j* says where the official client puts each of
  Reborn's names. The official order is exactly the open question. If it
  depended on the account, for example unlocked achievements first, or
  sorted by unlock time, then the mapping would encode the test
  account's unlock state. That is account data. Nothing recorded rules
  this out yet, which is why the pairs stay local.
- **Is the schema the same for every account?** This is assumed, not
  sourced: the schema in `ClientGetUserStats`
  (`docs/protocol/achievements.md`) describes the game's achievements,
  not the account's progress, but no recording compares two accounts. So
  the counts are treated as schema data only in this sense: they come
  from the app's list, and a count does not say which achievement is
  unlocked.
- **The names** stay out of every log, as ADR 0055 asks.
- **What the digests could reveal** if logged: a SHA-256 of a short API
  name can be reversed by guessing. That is why digests stay in memory
  only, as in ADR 0059 decision 2 and ADR 0056.

## Rules for using the result

- A survey describes, it does not decide. If the mapping shows "another
  order", the order is a **finding**. Reborn adopts an order hypothesis
  only with a source or an observation, never from memory (AGENTS.md
  section 4). A rule such as "sorted by name" or "by schema ID" is tested
  in a planned experiment before the shim uses it. Such an experiment can
  also show whether the order depends on the account, for example a rule
  that predicts the official order from the schema alone, from a source,
  and matches the local pairs. Never by unlocking or changing
  anything to see the order move (red line 4). Once one has,
  a new ADR may let the pairs into `docs/shim/`.
- Nothing in the shim, the proxy or `rebornd` is changed to make the
  comparison of ADR 0059 decision 2 match without a new plan.

## Consequences

- Experiment 36 can tell "order differs", "encoding or case differs" and
  "contents differ" apart (`docs/shim/meanings.md`). Its recorded result
  quotes the counts, the reading and the mapping summary, not the pairs.
- The proxy keeps at most 50 pairs of digests per process, and a second
  digest per Reborn name (at most 10,000, about 320 KiB).
- No new message between the proxy and `rebornd`, no new Valve request
  (red line 8): the survey uses the list the comparison already fetched.
