# 0046: Shadow and answer mode, the first step from proxy to shim (stage 5)

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

Stage 4 (ADR 0042, 0043) gives recorded positions a meaning: a candidate
from Valve's public documentation, confirmed by an experiment planned in
advance. Five meanings are confirmed (`docs/shim/meanings.md`), among
them `GetAppID` at position 9 of `SteamUtils010`, which all 14 recorded
games call (`docs/shim/coverage.md`).

Two things are open:

1. **Functions without a result.** Positions such as `SteamController`
   2 and `SteamInput006` 3 fit a documented function that returns
   nothing and is called every frame (`RunFrame`). Observing calls shows
   arguments and results; a function without a result leaves almost
   nothing to predict, so ADR 0042's experiments cannot confirm it.
2. **When the shim starts.** The shim (ADR 0012) answers games without
   the official client. Waiting until every position a game uses is
   confirmed before writing any shim code means a long time without
   testing a single answer in a real game.

## Problem

How can Reborn's own answers be tested in real games one call at a time,
long before a complete shim exists, without risking the account and
without letting the shim run on unconfirmed guesses?

## Alternatives considered

1. **Wait for the full shim.** Every answer is untested until a whole
   game runs on Reborn alone; a wrong answer is hard to find among
   hundreds.
2. **Shadow mode, then answer mode, inside the observation proxy.** For
   a short list of confirmed calls the proxy first computes Reborn's
   answer next to the real one and compares (shadow); after enough
   matches it gives the game Reborn's answer instead (answer). Every
   other call still goes to the official client.
3. **Answer mode right away.** Faster, but the first sign of a wrong
   answer would be a game misbehaving.

## Decision

Option 2.

### Shadow mode

- A fixed list in the proxy's source (like `READS` of ADR 0043) names
  calls by interface version text and position, each with Reborn's own
  implementation of its confirmed meaning.
- For a listed call, the proxy forwards the call to the official client
  as before, and the game gets the official client's result unchanged.
  After the return it computes Reborn's answer and logs `shadow match`
  or `shadow mismatch` with both values, unless a value may be account
  data (then only match or mismatch).
- Reborn's answers come only from Reborn's own data. For an observation
  run, values Reborn would know from its own launch of the game (such
  as the app ID) are given to the proxy by `reborn-observe arm`
  (for example `--app <id>`), never read from the official client.

### Answer mode

- A listed call may switch to answer mode when shadow mode matched in at
  least three different games and the maintainer gave the OK. In answer
  mode the proxy does not forward that call; the game gets Reborn's
  answer.
- The run is then an experiment in the sense of ADR 0042: the prediction
  is that the game behaves as in a normal run (it reaches the same
  screen, closes normally, and the other recorded calls look the same).

### Functions without a result

- A position whose candidate returns nothing is **confirmed** when, in
  addition to the fit from the documentation page:
  1. the recordings show the result register holding only leftovers
     (values that differ between games or stay the same address
     whatever the arguments), and
  2. a planned answer-mode experiment, in which Reborn's implementation
     of the documented behaviour runs instead of the official client's,
     matches its prediction in at least three games; where the page
     describes an effect the recorder can check (for example input
     arriving), the experiment checks it.
- Until then such a position stays a candidate.

### Limits

- Only confirmed meanings are listed (ADR 0042). Shadow mode never
  changes what the game gets; answer mode changes it only for listed
  calls.
- Never listed for answer mode: anything about ownership, licenses, DLC
  or tickets (red line 3), achievements, stats or cloud writes (red line
  4), or anything that acts on the account. Those stay with Valve's
  servers through Reborn's own client code (ADR 0012), never invented
  in the proxy.
- DRM is never treated specially (red line 1).
- `--read-only` (ADR 0040) still turns all of this off.
- The code is read-only towards Valve's objects and needs human review
  (AGENTS.md section 7).

### First entries

- Shadow mode: `SteamUtils010` position 9 (`GetAppID`), answered from
  the app ID given to `reborn-observe arm`.
- No answer-mode entries until shadow mode has matched in three games
  and the maintainer gave the OK.

## Consequences

- Reborn's own answers meet real games one call at a time, with the
  official client's answer next to them for comparison.
- The shim's first code can be written and tested before the shim as a
  whole exists; when it does, its answers have already been compared.
- Functions without a result get a way to be confirmed through
  behaviour, which is weaker than a recorded result; the three-game rule
  and the maintainer's OK make up for part of that.
- More code in the proxy, all of it behind fixed lists.
