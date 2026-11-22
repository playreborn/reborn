"""Experiment 13 (docs/shim/meanings.md): experiment 12 again, with each
prediction checked only in the games it names.

Predictions:
A (An Arcade Full of Cats, One's Own Exile, A Park Full of Cats):
   STEAMUSERSTATS_INTERFACE_VERSION012 position 0 (RequestCurrentStats)
   is called once, within the first 3 seconds, returning true; position 10
   (StoreStats) is called once, at least 20 seconds after position 0, at
   close, returning true.
B (Baldi's Basics Classic Remastered, Chronigma, HoloCure): SteamInput006
   position 0 (Init) is called once, before any other SteamInput006
   position, returning true.

Each log is given with the part it is about: A:<log> or B:<log>. The
experiment matches only if every part has exactly its three logs and all
six match; a log without the interface is a mismatch, not "neither way".

Usage: python tools/shim/experiments/013_stats_and_input_init_scoped.py A:<log> ... B:<log> ...
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

STATS = "STEAMUSERSTATS_INTERFACE_VERSION012"
INPUT = "SteamInput006"


def true(c):
    return c.rax is not None and c.rax & 0xFF == 1


def check_a(calls):
    stats = [c for c in calls if c.interface == STATS]
    p0 = [c for c in stats if c.position == 0]
    p10 = [c for c in stats if c.position == 10]
    start = calls[0].ms if calls else 0
    ok = (len(p0) == 1 and p0[0].ms - start <= 3000 and true(p0[0])
          and len(p10) == 1 and p10[0].ms - p0[0].ms >= 20000 and true(p10[0]))
    return ok, f"position 0 at {[c.ms - start for c in p0]} ms, position 10 at {[c.ms - start for c in p10]} ms"


def check_b(calls):
    inp = [c for c in calls if c.interface == INPUT]
    p0 = [c for c in inp if c.position == 0]
    ok = bool(inp) and len(p0) == 1 and inp[0] is p0[0] and true(p0[0])
    first = inp[0].position if inp else None
    return ok, f"first {INPUT} call is position {first}, {len(p0)} position-0 calls"


verdicts = {"A": [], "B": []}
for arg in sys.argv[1:]:
    part, _, path = arg.partition(":")
    if part not in verdicts or not path:
        sys.exit(f"argument {arg!r}: expected A:<log> or B:<log>")
    ok, text = (check_a if part == "A" else check_b)(observelog.calls(path))
    verdicts[part].append(ok)
    print(f"{Path(path).name} {part}: {text} -> {'match' if ok else 'MISMATCH'}")
complete = all(len(v) == 3 for v in verdicts.values())
for part, v in verdicts.items():
    print(f"part {part}: {len(v)} logs, {'match' if len(v) == 3 and all(v) else 'MISMATCH'}")
print("experiment 13:", "match" if complete and all(all(v) for v in verdicts.values()) else "MISMATCH")
