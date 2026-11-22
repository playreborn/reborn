"""Experiment 12 (docs/shim/meanings.md).

Predictions:
A. STEAMUSERSTATS_INTERFACE_VERSION012: position 0 (RequestCurrentStats) is
   called once, within the first 3 seconds, returning true; position 10
   (StoreStats) is called once, at least 20 seconds after position 0
   (when the game closes), returning true.
B. SteamInput006: position 0 (Init) is called once, before any other
   SteamInput006 position, returning true.
A log without the interface counts neither way for that part.

Usage: python tools/shim/experiments/012_stats_and_input_init.py <log>...
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

def true(c):
    return c.rax is not None and c.rax & 0xFF == 1

verdicts = []
for path in sys.argv[1:]:
    calls = observelog.calls(path)
    name = Path(path).name
    stats = [c for c in calls if c.interface == "STEAMUSERSTATS_INTERFACE_VERSION012"]
    if stats:
        p0 = [c for c in stats if c.position == 0]
        p10 = [c for c in stats if c.position == 10]
        start = calls[0].ms
        ok = (len(p0) == 1 and p0[0].ms - start <= 3000 and true(p0[0])
              and len(p10) == 1 and p10[0].ms - p0[0].ms >= 20000 and true(p10[0]))
        verdicts.append(ok)
        print(f"{name} A: position 0 at {[c.ms for c in p0]} ms, position 10 at {[c.ms for c in p10]} ms -> {'match' if ok else 'MISMATCH'}")
    inp = [c for c in calls if c.interface == "SteamInput006"]
    if inp:
        p0 = [c for c in inp if c.position == 0]
        ok = len(p0) == 1 and inp[0] is p0[0] and true(p0[0])
        verdicts.append(ok)
        print(f"{name} B: first SteamInput006 call is position {inp[0].position}, {len(p0)} position-0 calls -> {'match' if ok else 'MISMATCH'}")
    if not stats and not inp:
        print(f"{name}: neither interface (counts neither way)")
print("experiment 12:", "match" if verdicts and all(verdicts) else ("nothing checked" if not verdicts else "MISMATCH"))
