"""Experiment 14 (docs/shim/meanings.md): STEAMINVENTORY_INTERFACE_V003.

Predictions:
A (Cave Crawlers, Bongo Cat): position 6 (GetAllItems) is called at least
   once; every logged position 6 call returns true and the proxy read the
   four bytes at rdx after it returned; each such value H is below 0x10000
   and is the low 32 bits of rdx in at least one later position 1 call.
B (Bongo Cat): position 3 (GetResultTimestamp) is called at least once;
   for every call, the low 32 bits of the result are a Unix time within
   one day of the log file's last write, and the low 32 bits of rdx equal
   the low 32 bits of rdx in some position 1 call of the same log.

Position 1 (candidate GetResultItems) is recorded, not predicted.

Each log must be a different file and must be recorded by run.sh in
starts.log for its game (Cave Crawlers 2143030, Bongo Cat 3419430);
exactly one log per game. Anything else is a mismatch.

Usage: python tools/shim/experiments/014_inventory_handles.py <log> <log>
"""
import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

INV = "STEAMINVENTORY_INTERFACE_V003"
GAMES = {2143030: "Cave Crawlers", 3419430: "Bongo Cat"}
low = observelog.low32


def part_a(calls):
    six = [c for c in calls if c.position == 6]
    ok = bool(six)
    handles = []
    for c in six:
        h = observelog.after_u32(c, "rdx")
        if c.rax is None or c.rax & 0xFF != 1 or h is None or h >= 0x10000:
            ok = False
            continue
        later = [x for x in calls if x.position == 1 and x.ms >= c.ms and low(x.reg("rdx")) == h]
        ok &= bool(later)
        handles.append(h)
    return ok, f"{len(six)} position-6 calls, handles {handles}"


def part_b(calls, path):
    three = [c for c in calls if c.position == 3]
    used = {low(c.reg("rdx")) for c in calls if c.position == 1}
    written = os.path.getmtime(path)
    ok = bool(three)
    for c in three:
        if c.rax is None or abs(low(c.rax) - written) > 86400 or low(c.reg("rdx")) not in used:
            ok = False
    offsets = [round(low(c.rax) - written) if c.rax is not None else None for c in three]
    return ok, f"{len(three)} position-3 calls, result minus log time {offsets} s"


paths = sys.argv[1:]
verdicts, seen = [], {}
if len(set(Path(p).resolve() for p in paths)) != len(paths):
    print("a log was given twice -> MISMATCH")
    verdicts.append(False)
for path in paths:
    app = observelog.app_of(path)
    name = Path(path).name
    if app not in GAMES or app in seen:
        print(f"{name}: game {app} not expected or given twice -> MISMATCH")
        verdicts.append(False)
        continue
    seen[app] = path
    calls = [c for c in observelog.calls(path) if c.interface == INV]
    ok, text = part_a(calls)
    verdicts.append(ok)
    print(f"{name} ({GAMES[app]}) A: {text} -> {'match' if ok else 'MISMATCH'}")
    if app == 3419430:
        ok, text = part_b(calls, path)
        verdicts.append(ok)
        print(f"{name} ({GAMES[app]}) B: {text} -> {'match' if ok else 'MISMATCH'}")
complete = set(seen) == set(GAMES)
print("experiment 14:", "match" if complete and verdicts and all(verdicts) else "MISMATCH")
