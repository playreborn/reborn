# 0043: Reading result texts and output bytes for planned experiments (step 4c)

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

ADR 0042 gives recorded calls a meaning in two steps: a **candidate**
from Valve's public documentation, and **confirmed** after an
experiment planned in advance matched its prediction. The first
candidates for `STEAMUSERSTATS_INTERFACE_VERSION011` are in
`docs/shim/meanings.md`. With what the proxy logs today, none of them can
be confirmed:

- Position 12 got an achievement name and the text `name`, and returned
  an address. The candidate returns text. To check it, the text at the
  returned address has to be seen.
- Position 6 got a name and an address on the game's stack, and returned
  1. Several candidates fit; they differ in what they write to that
  address (one byte, four bytes). To tell them apart, the first bytes
  there have to be seen after the call returns.

What the proxy may read is fixed:

- ADR 0038: arguments as raw values, an argument as text only if it
  points to readable memory with printable ASCII; buffers by length and
  SHA-256 only.
- ADR 0040: pointer values at a returned object's first field (step 3a).
- ADR 0042: the first 16 bytes at `rdx` and `r8` after
  `Steam_BGetCallback` returns 1, and nothing else. "If the 16 bytes
  show that more is needed, a new ADR decides."

Other calls write to addresses too. `SteamUser020` position 2 (in
`docs/shim/objects.md`) got an address on the game's stack and returned
it. Something like that could hold account data, so reading behind
every address is not harmless.

## Problem

How does the proxy see the result text or the output bytes of a call,
so that a planned experiment can confirm or reject a candidate, without
reading behind every address of every call?

## Alternatives considered

1. **For every call through a stand-in:** log the result as text when it
   points to ASCII, and the first 16 bytes behind every argument that
   pointed to the game's stack. Covers every experiment at once, but
   reads where no experiment asked for it, including calls that may
   hand out account data.
2. **Only for listed calls:** a short list in the proxy's source names
   the interface version text, the position and what is read: the
   result as text, or the first 16 bytes at the address one named
   argument register held. Each entry belongs to a planned experiment in
   `docs/shim/meanings.md`.
3. **Use a debugger for each experiment:** no proxy change, but a manual
   setup per experiment and per game, and harder to repeat.

## Decision

Option 2.

- The proxy has a fixed list in its source code, empty except for the
  entries below. Each entry names an interface version text, a position
  and one of two kinds of read:
  - **result text:** after the call returns, if `rax` points to
    readable memory with 2 to 64 printable ASCII characters and a NUL,
    that text is logged, with the same rules as argument texts
    (ADR 0038, `VirtualQuery` first);
  - **output bytes:** after the call returns, the first 16 bytes at the
    address the named argument register (`rdx`, `r8` or `r9`) held when
    the call started, as hex, only if `VirtualQuery` shows all 16 bytes
    readable (the same rules as ADR 0042, 4b).
- Nothing else is read: no further bytes, no addresses found in those
  bytes are followed, and calls not on the list are logged as before.
- Reads happen only for calls that are logged anyway (the first 50 of
  each kind, then every 10 000th).
- **Adding an entry** needs a candidate and an experiment plan in
  `docs/shim/meanings.md` that names the entry, and the maintainer's OK
  in the pull request. An entry is never added for a call whose
  candidates could hand back a ticket, a token, a password or an
  account identifier; such calls need their own ADR.
- First entries, from `docs/shim/meanings.md`:
  - `STEAMUSERSTATS_INTERFACE_VERSION011` position 12: result text.
  - `STEAMUSERSTATS_INTERFACE_VERSION011` position 6: output bytes at
    `r8`.
- Raw logs stay with the recorder. Before anything goes into
  `docs/shim/`, every value that may be account data is replaced, as
  before. Achievement names and display names are the game's public
  data; whether the test account has unlocked an achievement is noted
  only as a match or mismatch with the prediction.

## Consequences

- The candidates for positions 12 and 6 can be checked in one run of
  Cave Crawlers, without changing anything on the account.
- Each further experiment that needs a read adds one reviewed list entry
  and a new proxy build; there is no switch at run time.
- A small amount of read-only `unsafe` code more, with human review
  (AGENTS.md section 7).
- Everything else as in ADR 0038, 0040, 0041 and 0042: nothing of
  Valve's is changed, the game gets the same results as without the
  proxy, DRM is never treated specially, and the game stays unsupported
  if it misbehaves.
