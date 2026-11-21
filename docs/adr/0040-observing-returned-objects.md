# 0040: Observing calls on returned objects (stage 3)

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

ADR 0038 leaves stage 3 ("whether and how calls on returned objects are
observed") to its own ADR, once stages 0 to 2 have results. They have
(`docs/shim/`):

- The game asks `CreateInterface` for `SteamClient017` and
  `SteamClient020` (text in `rcx`) and gets a non-zero address back for
  each.
- Nothing else is known about these addresses. A common pattern on
  Windows is an object whose first eight bytes point to a table of
  function addresses, called with the object's address in `rcx`. That is
  a guess, not a source (AGENTS.md § 4); each part of it has to be
  observed before anything relies on it.

The proxy already forwards and logs the five exported entry points
without changing them (ADR 0039).

## Problem

How Reborn learns what the game does with the returned addresses,
without the SDK, without decompiling or disassembling, without changing
Valve's files or memory, and without guessing.

## Alternatives considered

1. **Change the table in the real object** so its entries point to
   Reborn's stubs: it writes into the official client's memory. Rejected.
2. **Give the game a stand-in object of Reborn's** that forwards to the
   real one: nothing of Valve's is changed; works only if the observed
   shape (first field points to a table, `rcx` holds the object) holds.
3. **Stop at the exported entry points:** safe, but the shim would know
   nothing about what lies behind `CreateInterface`.

## Decision

Option 2, in steps, each recorded in `docs/shim/` before the next starts:

- **3a, shape (read only):** for each address `CreateInterface` returns,
  the proxy reads and logs, after checking readability with
  `VirtualQuery`:
  - the first eight bytes, and whether they hold an address inside
    `steamclient64.dll`'s own memory;
  - if so, how many consecutive eight-byte values from there are
    addresses inside a part of `steamclient64.dll` that Windows marks as
    executable (counted, at most 1024; the values themselves are logged
    only as offsets from the module start).

  Only pointer values in data are read; no code bytes, no disassembly.
- **3b, stand-in objects:** only if 3a shows that shape. The proxy hands
  the game, instead of the returned address, a stand-in object of its
  own whose first field points to a table of Reborn stubs, as many as 3a
  counted. Each stub knows its position in the table and:
  - if `rcx` holds the stand-in's address, puts the real object's
    address there; otherwise leaves `rcx` alone and logs that;
  - logs the position and the raw registers as the proxy does now,
    follows the result through the return stub, and jumps to the entry
    at the same position in the real object's table.

  Whether the game uses `rcx` for the object is thereby observed, not
  assumed.
- **3c, objects returned by those calls:** a new ADR once 3b has results.
- Stop rules: if the game misbehaves at any step, the step is reverted,
  the game stays unsupported for that step, and nothing is worked around
  (red line 1). The same logging rules as ADR 0038 apply; raw register
  logs stay on the recorder's computer and are redacted before anything
  goes into `docs/shim/` (they can hold account data).

## Decided by the maintainer (2026-11-21)

Reading pointer values from the official client's memory while the game
runs, as step 3a describes (the first field of the returned object and
the table it points to), counts as observing the running program
(§ 69d(3) UrhG, ADR 0012). Code bytes stay off limits: they are never
read, logged or disassembled.

## Consequences

- 3a changes nothing the game sees; 3b changes only what the game gets
  back from `CreateInterface`, never Valve's code, data or files.
- More `unsafe` code and assembly, with human review (AGENTS.md
  section 7).
- If the shape is not what 3a looks for, stage 3 stops there and a new
  ADR decides how to go on.
