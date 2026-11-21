# 0042: How recorded calls get a meaning, and observing callbacks (stage 4)

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

Stage 3 (ADR 0040, ADR 0041) is recorded for Cave Crawlers on Windows
(`docs/shim/objects.md`): which objects the game gets, at which table
positions it calls them, and with which raw arguments and results.
Every entry ends with "Meaning: not known". A shim cannot answer a call
whose meaning is unknown.

What is allowed as a source for meaning is already set:

- ADR 0012 and `docs/shim/README.md`: observation recordings, and
  Valve's public documentation on its website for what something means
  (linked, not copied).
- `docs/legal/shim-assessment.md` § 2: learning from the public
  documentation is fine, copying its text is not. What a call does, the
  size and meaning of arguments and the result codes are technical
  facts that may be reimplemented.
- AGENTS.md § 4: memory is never a source, also not an AI model's,
  whose training data may contain SDK headers.

Some recorded calls hint at their purpose: one position of
`STEAMREMOTESTORAGE_INTERFACE_VERSION014` got the game's save file name,
an address and a number; one of `STEAMUSERSTATS_INTERFACE_VERSION011`
got stat names. A hint is not a source.

Callbacks are open too (`docs/shim/entry-points.md`):
`Steam_BGetCallback` is called about every 3 ms with `rcx` = 1 and two
stack addresses in `rdx` and `r8`; its lowest result byte was 1 once, and
`Steam_FreeLastCallback` followed right after. What the client wrote to
those addresses was not read: ADR 0038 logs buffers only by length and
SHA-256.

## Problem

1. When does a recorded position count as having a meaning, with a
   source that holds up, and without an AI model filling gaps from
   memory?
2. How does the proxy observe what `Steam_BGetCallback` hands the game,
   without logging more than needed?

## Alternatives considered

For meanings:

1. **Documentation alone:** a position gets the meaning of a documented
   function whose parameters fit the recorded arguments. Fast, but
   several functions can fit the same pattern, and a wrong match would
   go straight into the shim.
2. **Documentation plus an experiment:** a fitting documented function
   is only a candidate; it counts once an experiment planned in advance
   confirms a prediction made from the documentation.
3. **Experiments only:** Reborn names behaviour itself, without the
   documentation. Clean, but slow, and result codes and argument sizes
   would often stay unknown.

For callbacks:

1. **Keep logging only length and hash:** nothing is learned.
2. **Log the first bytes the client wrote:** bounded, enough to see
   whether the memory holds small numbers or addresses.
3. **Log everything the memory holds:** could put whole payloads,
   including account data, into the log.

## Decision

Meanings: option 2. Callbacks: option 2.

### 4a, meanings (documentation work, no code)

- Each meaning in `docs/shim/` has a status: **candidate** or
  **confirmed**. Without either, it stays "not known".
- **Candidate:** a page of Valve's public documentation that opens
  without signing in and without accepting an agreement describes a
  function whose parameters and result fit what was recorded: how many
  argument registers carry values, which hold text, numbers, addresses,
  or an address with a size next to it, and what comes back. The entry
  links the page and describes the fit in Reborn's own words. Text,
  parameter names and signatures from the page are not copied.
- **Positions come only from recordings.** The order of functions on a
  documentation page, and any number on it, is never used to decide a
  position or an argument register.
- **Confirmed:** the entry first describes an experiment and the
  prediction it makes from the linked page: an action of the recorder
  in the game (for example saving, or changing a setting) or a change of
  account state on the test account, and which position fires, which
  argument changes or which result comes back. Then the recorder runs it
  with the proxy. If the recording matches, the meaning is confirmed; if
  not, it stays a candidate and the mismatch is noted. An experiment
  never manipulates achievements or stats (red line 4) and never does
  anything on the account the game would not do by itself.
- **Versions:** a meaning is confirmed only for the interface version
  text it was recorded with. The documentation may describe a newer
  version; the entry says which one the recording saw.
- **AI agents** write candidates only from entries in `docs/shim/` and
  documentation pages fetched in the same session and linked in the
  entry. They never name a function, a parameter list, a position or a
  result code from memory. If no fetched page fits, the meaning stays
  "not known". The maintainer reviews every candidate and every
  confirmation.
- Pages are read one at a time, as a person would; no crawling
  (red lines 5 and 8).
- **The shim implements only confirmed meanings.** Candidates may guide
  experiments, never code.

### 4b, callbacks (proxy change)

- After a `Steam_BGetCallback` call whose result's lowest byte is 1, the
  proxy reads the first 16 bytes at the address `rdx` held when the call
  started, and the first 16 bytes at the address `r8` held, each only
  after `VirtualQuery` shows the memory is readable. It logs them as hex.
- Nothing else is read: no further bytes, no addresses found in those
  bytes are followed, no other entry point or method gets this. This is
  the one exception to "buffers by length and SHA-256 only" of ADR 0038.
- Raw logs stay with the recorder. Before anything goes into
  `docs/shim/`, every value that may be account data (a SteamID or
  anything else not needed for the fact) is replaced, as before.
- If the 16 bytes show that more is needed (for example an address to
  the payload), a new ADR decides.

## Decided by the maintainer (2026-11-21)

Reading the first 16 bytes the official client wrote for the game at
`rdx` and `r8` after `Steam_BGetCallback` returns with lowest byte 1 is
allowed, as described in 4b. It is data for the game, not code bytes,
and the raw log stays on the recorder's computer.

## Consequences

- Meanings arrive slowly, one experiment at a time, and need the
  recorder's time; some positions may never be confirmed.
- Every line of the shim can be traced to a recording and, for its
  meaning, to a linked public page and a confirming experiment.
- 4b adds a small amount of read-only `unsafe` code, with human review
  (AGENTS.md section 7).
- Everything else as in ADR 0038, 0040 and 0041: nothing of Valve's is
  changed, DRM is never treated specially, and the game stays
  unsupported if it misbehaves.
