"""Experiment 31 (docs/shim/meanings.md, loop iteration 15a):
SteamUtils009 position 16 (candidate SetWarningMessageHook) and
SteamGameServer012 position 0 (candidate InitGameServer) in Gravitas
(1067310, SteamClient018) and Dark Deception (332950, SteamClient019).
Candidates from https://partner.steamgames.com/doc/api/ISteamUtils and
https://partner.steamgames.com/doc/api/ISteamGameServer.

Needs READS (crates/shim-observe, ADR 0043/0053), added on the plan's
branch:
  ("SteamUtils009", 16, Read::Executable("rdx")): the proxy logs
    `after rdx: executable (outside steamclient64)` or `not executable`
    (only the page protection is asked for, no byte is read);
  ("SteamGameServer012", 0, Read::StackArguments): the proxy logs
    `stack arguments 5=0x.. 6=0x.. 7=0x..` (counting rcx as the first
    argument) and `N->"text"` for a slot that points to a text.
Without them U2 and G2 mismatch.

Assumptions (stated in the plan, as in experiments 23 and 24):
  A1  A bool, 8-, 16- or 32-bit argument or result arrives with the upper
      32 bits of its 64-bit register zero, a bool as 0 or 1 in the lowest
      byte (x64 zero-extends 32-bit register writes; an assumption about
      the compiled code, not from the pages).
  A2  A register the game did not set for the call does not point into
      executable memory outside steamclient64 at every call of every run.
  A3  Stack slots that are not arguments of the call do not hold the
      game's app ID in slot 6 and a pointer to a text in slot 7 at the
      first call of every run of both games.

Predictions, per game:
U1  SteamUtils009 position 16 is called at least once; at every call rdx
    has non-zero upper 32 bits and does not point to a text.
U2  At every call the proxy logged `after rdx: executable (outside
    steamclient64)`.
U3  Every call returned, with non-zero upper 32 bits of rax and a lowest
    byte other than 0 and 1.
G1  The first SteamGameServer012 call is position 0, and position 0 is
    called exactly once; its rdx has low 32 bits 0, r8 and r9 lie in
    1..=0xffff and differ.
G2  The stack line of that call has slot 6 with low 32 bits equal to the
    app ID run.sh started (starts.log) and slot 7 pointing to a text.
G3  Its rax is 0 or 1.

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the two games, one log per game, both
needed. Freshness (LOOP.md step 4): a log whose start (observelog.start_of)
is earlier than PLAN_COMMITTED is rejected. PLAN_COMMITTED is the committer
time (local, as run.sh writes starts.log) of the commit on loop/plan-15a
that holds the final plan text and this script; it is set in a follow-up
commit.

Prints only positions, counts and match/MISMATCH; no addresses, no texts,
no account data.

Usage: python tools/shim/experiments/031_hook_and_game_server.py <log> <log>
"""
import re
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {
    1067310: "Gravitas",
    332950: "Dark Deception",
}
UTILS, SERVER = "SteamUtils009", "SteamGameServer012"
PLAN_COMMITTED = datetime(2026, 11, 22, 20, 17, 38)  # commit a85feef on loop/plan-15a (final plan)
EXECUTABLE = "after rdx: executable (outside steamclient64)"
SLOT = re.compile(r"\b(\d)=(0x[0-9a-f]+|not readable)")
POINTS = re.compile(r'\b(\d)->"')


def verdict(ok):
    return "match" if ok else "MISMATCH"


def stack_line(call):
    """(slot -> value or None, slots that point to a text), or None."""
    for extra in call.extra:
        if extra.startswith("stack arguments"):
            values = {}
            for slot, value in SLOT.findall(extra):
                values[int(slot)] = None if value == "not readable" else int(value, 16)
            return values, {int(s) for s in POINTS.findall(extra)}
    return None


def check(path, app, label, verdicts):
    calls = observelog.calls(path)

    def say(point, ok, text):
        verdicts.append(ok)
        print(f"{label} {point}: {text} -> {verdict(ok)}")

    hooks = [c for c in calls if c.interface == UTILS and c.position == 16]
    bad1 = [c for c in hooks
            if c.reg("rdx") is None or c.reg("rdx") >> 32 == 0 or "rdx" in c.texts]
    say("U1", bool(hooks) and not bad1,
        f"{len(hooks)} calls of position 16, {len(bad1)} with a small, missing or text rdx")
    bad2 = [c for c in hooks if EXECUTABLE not in c.extra]
    say("U2", bool(hooks) and not bad2,
        f"{len(bad2)} calls without the line '{EXECUTABLE}'")
    bad3 = [c for c in hooks
            if c.rax is None or c.rax >> 32 == 0 or c.rax & 0xFF in (0, 1)]
    say("U3", bool(hooks) and not bad3,
        f"{len(bad3)} calls without a result of the leftover shape")

    server = [c for c in calls if c.interface == SERVER]
    zeros = [c for c in server if c.position == 0]
    init = zeros[0] if len(zeros) == 1 else None
    first_ok = bool(server) and server[0].position == 0
    regs = (init.reg("rdx"), init.reg("r8"), init.reg("r9")) if init else (None, None, None)
    ok_g1 = (init is not None and first_ok and None not in regs
             and observelog.low32(regs[0]) == 0
             and 0 < regs[1] <= 0xFFFF and 0 < regs[2] <= 0xFFFF and regs[1] != regs[2])
    say("G1", ok_g1, f"{len(server)} {SERVER} calls, first is position 0: {first_ok}, "
        f"{len(zeros)} calls of position 0")
    if init is None:
        say("G2-G3", False, "not exactly one call of position 0, nothing more to check")
        return
    stack = stack_line(init)
    if stack is None:
        say("G2", False, "no stack line logged")
    else:
        values, texts = stack
        six = values.get(6)
        ok_g2 = six is not None and observelog.low32(six) == app and 7 in texts
        say("G2", ok_g2, f"slot 6 is the app ID: {six is not None and observelog.low32(six) == app}, "
            f"slot 7 points to a text: {7 in texts}")
    say("G3", init.rax in (0, 1), f"result {init.rax if init.rax in (0, 1) else 'other'}")


paths = sys.argv[1:]
verdicts, seen = [], set()
if len({Path(p).resolve() for p in paths}) != len(paths):
    print("a log was given twice -> MISMATCH")
    verdicts.append(False)
for path in paths:
    name, app = Path(path).name, observelog.app_of(path)
    if app not in GAMES or app in seen:
        print(f"{name}: game {app} not expected or given twice -> MISMATCH")
        verdicts.append(False)
        continue
    started = observelog.start_of(path)
    if started is None or started < PLAN_COMMITTED:
        print(f"{name} ({GAMES[app]}): started {started}, before the plan commit {PLAN_COMMITTED} -> MISMATCH")
        verdicts.append(False)
        continue
    seen.add(app)
    check(path, app, f"{name} ({GAMES[app]})", verdicts)
print("experiment 31:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
