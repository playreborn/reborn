# 0058: An executable check and a narrow stack read for the proxy

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22
- **Amends:** ADR 0043, ADR 0053

## Context

ADR 0043 lets the observation proxy read, for listed calls only, the text
`rax` points to or the first 16 bytes behind one argument register. ADR
0053 lets the agent add such entries itself. Experiment 31 (loop
iteration 15a, `docs/shim/meanings.md`) needs two facts neither kind
gives:

1. `SteamUtils009` position 16, candidate `SetWarningMessageHook`: whether
   the address in `rdx` is a function, that is, executable memory.
2. `SteamGameServer012` position 0, candidate `InitGameServer`: the page
   gives it six parameters, so its sixth and seventh arguments (counting
   the object in `rcx`) are the app ID and a version text, which lie on
   the stack, where the proxy does not look.

The first plan read the fifth to seventh stack slots (24 bytes) and
followed every address found there to a text. The review pass of ADR
0051 rejected that: if the meaning is not the one predicted, those slots
are leftover stack memory, and the bytes or the text behind them could be
a server token, a password or a persona name.

## Decision

Two more read kinds for `READS`, under ADR 0053's rules otherwise (named
in an experiment plan committed before the runs, checked by the review
pass, listed in the pull request; raw logs stay on the recorder's
computer):

1. **`Read::Executable(register)`** logs only `after <register>:
   executable` or `not executable`, and for executable memory whether the
   address lies inside `steamclient64` or outside it. It asks Windows for
   the protection of the page (`VirtualQuery`); no byte at the address is
   read or logged. A yes or no about page protection, and the place
   relative to one module, cannot carry account data.
2. **`Read::StackAppIdAndText`** logs only, from the stack as the call
   started:
   - the low 32 bits of the sixth argument's slot, as a number;
   - for the seventh argument's slot, `yes` or `no`: whether it points to
     a readable, NUL-terminated printable text (the proxy's text rule:
     `VirtualQuery`, never past the end of the region, at most the usual
     text length);
   - `not readable` for a slot outside readable memory.

   It never logs the fifth slot, the seventh slot's value or the text
   itself. Every address is computed with `checked_add` and every read is
   bounded by `VirtualQuery` and the region's end, as for the other
   reads. Addresses found on the stack are never followed for logging;
   the seventh slot is looked at only to answer yes or no.

   Why no account data can come out even if the slots are leftover
   memory: a yes or no reveals no content, and 32 bits are too few for a
   Steam ID (64 bits), a ticket, a token or a password. They could hold
   the low half of a 64-bit Steam ID, the account ID of the test account.
   That is why the experiment script compares the value only with the app
   ID `run.sh` started and prints match or mismatch, and `docs/shim/`
   quotes no value (ADR 0050's rule for account values).

Anything wider (more slots, full values, text content, following
addresses) still needs its own ADR and the maintainer.

## Decided by the maintainer (2026-11-22)

After the review result, the maintainer chose this narrowed option over
dropping the stack read.

## Consequences

- Experiment 31 can confirm `SetWarningMessageHook` (under its
  assumptions A1 and A2) and `InitGameServer` (under A3).
- Later plans may use both kinds for other calls under ADR 0053, within
  exactly the limits above.
