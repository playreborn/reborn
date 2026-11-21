# 0041: Stand-ins for objects returned through stand-ins (step 3c)

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

ADR 0040 leaves step 3c ("objects returned by those calls") to a new ADR
once 3b has results. They are in `docs/shim/objects.md`:

- The game calls through the stand-in for `SteamClient020`, always with
  the object in `rcx`.
- At position 12 it passed a text in `r9` 25 times (21 different texts
  such as `SteamUser020`, `SteamUtils009`,
  `STEAMREMOTESTORAGE_INTERFACE_VERSION014`) and got a non-zero address
  back each time; the same text always gave the same address. Position 5
  did the same for `SteamUser020`.
- What those addresses are is not observed; the texts suggest objects
  like the ones `CreateInterface` returns, but that is not a source.

## Problem

How the proxy observes what the game does with those addresses, without
mistaking a result for an object that is none, and without the real
code ever getting a Reborn object.

## Alternatives considered

1. **Stand-in for every result with the shape of step 3a:** some results
   may only look like that by chance; replacing them would break the game.
2. **Stand-in only when the call passed a text and the result has the
   shape of step 3a:** matches what was observed at positions 12 and 5;
   other results stay as they are.
3. **Stop at `SteamClient020`:** the shim would know nothing about the
   objects the game uses most.

## Decision

Option 2.

- A call through a stand-in whose arguments include a text (an integer
  register pointing to printable ASCII, as logged now) and whose result
  has the shape of step 3a (first field points into
  `steamclient64.dll`, followed by 1 to 112 addresses in its executable
  memory) gets a stand-in for its result, named after that text (for
  example `SteamUser020#3` in the log). The shape line is logged for
  every such result, also when no stand-in is made.
- The same real object always gets the same stand-in, wherever it comes
  from.
- Room for 32 stand-ins (was 4), still 128 positions each. When all are
  used, or a table has more counted entries, the result stays as it is
  and the log says why.
- If any argument register (`rcx`, `rdx`, `r8`, `r9`) of a call through a
  stand-in, or of a call to one of the five exported entry points, holds
  a stand-in's address, the proxy puts the real object there and logs
  it. The real code then never gets a Reborn object in a register.
  Arguments on the stack are not looked at; if the game passes a
  stand-in there and misbehaves, that is the observation, and
  `--read-only` goes back to step 3a.
- Everything else as in ADR 0040: the real tables are only read, nothing
  of Valve's is changed, raw logs stay with the recorder and are redacted
  before anything goes into `docs/shim/`.

## Consequences

- One more level of observation; deeper levels follow the same rule
  without a new ADR, as long as each still needs a text argument and the
  step 3a shape.
- More generated stubs (32 × 128) and more `unsafe` code, with human
  review.
- If the game misbehaves, `reborn-observe arm --read-only` turns all
  stand-ins off.
