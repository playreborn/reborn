# 0050: Reading what a callback carries (step 4d)

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22

## Context

Step 4b (ADR 0042) logs the first 16 bytes at the addresses `rdx` and
`r8` held when `Steam_BGetCallback` returns 1. Across the recorded games
those bytes always look the same way (`docs/shim/entry-points.md`):

- 4 bytes `1` (the same value as the handles of experiment 7),
- 4 bytes a number (for example `0x2bf` = 703, `0x0fdeac`, `0x504`), seen
  more than a hundred times for some numbers,
- 8 bytes that look like an address in the game's heap.

ADR 0042 ends with: "If the 16 bytes show that more is needed (for
example an address to the payload), a new ADR decides." This is that
case. Valve's public pages do not describe the layout: the overview page
(<https://partner.steamgames.com/doc/sdk/api>) only says that each
callback is a struct with a unique identifier and some data, and points
to `steam_api.h` for manual dispatch, which ships with the Steamworks SDK
and is excluded (red line 6, ADR 0012). The meaning of each callback has
to come from observation plus the public interface pages, which list
callback structs with their fields in words.

What a callback carries can be account data: Steam IDs, names, stats.

## Problem

How does the proxy observe a callback's data well enough to give
callback numbers a meaning (and later let the shim produce them), without
logging account data by default?

## Alternatives considered

1. **Keep 16 bytes.** Numbers can be counted and timed, but nothing is
   learned about what a callback carries.
2. **Size and hash for every callback, bytes only for listed numbers.**
   Read 8 more bytes of the header (enough to see whether a size follows
   the address), and for the data behind the address log only its length
   and SHA-256, as ADR 0038 does for buffers; for callback numbers on a
   reviewed list, also the first 64 bytes.
3. **Log every payload.** Fast, but puts account data into every log.

## Decision

Option 2.

- After a `Steam_BGetCallback` call with lowest result byte 1, the proxy
  reads 24 bytes at `rdx` instead of 16 (same `VirtualQuery` rules).
- If bytes 8 to 16 hold an address and bytes 16 to 20 a size of at most
  4096 that make a readable range, the proxy logs the size and the
  SHA-256 of that range, never its content.
- For callback numbers in a fixed list in the proxy's source, it also
  logs the first 64 bytes of that range as hex. An entry is added only
  with a candidate and an experiment plan in `docs/shim/meanings.md` that
  names the number, and the maintainer's OK. Numbers whose candidate
  carries a Steam ID, a name or another account identifier are only
  listed with a separate decision, and their values never go into
  `docs/shim/` (only matches and mismatches).
- Raw logs stay on the recorder's computer, as before.

## Decided by the maintainer (2026-11-22)

Accepted as written. Callbacks that carry the test account's own Steam
ID may be listed, with the value kept out of `docs/shim/` (only matches
and mismatches).

## Consequences

- Callback numbers can get meanings one at a time, which every game
  needs before it can run on the shim.
- By default, logs gain only sizes and hashes of callback data.
