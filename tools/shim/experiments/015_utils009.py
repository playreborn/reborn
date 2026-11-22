"""Experiment 15 (docs/shim/meanings.md): SteamUtils009 positions 9 and 14.

Predictions:
A (Cave Crawlers, Gravitas, Dark Deception, UNBEATABLE [white label]):
   SteamUtils009 position 9 (GetAppID) is called at least once, and every
   call's result, in its low 32 bits, is the game's app ID.
B (Cave Crawlers, Dark Deception, UNBEATABLE [white label]): position 14
   (GetCurrentBatteryPower) is called at least once, and every call
   returns 255 in its lowest byte (a desktop on AC power).

Each log must be a different file recorded by run.sh in starts.log for
one of these games, one log per game; all four games are needed.
Anything else is a mismatch.

Usage: python tools/shim/experiments/015_utils009.py <log>...
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {2143030: "Cave Crawlers", 1067310: "Gravitas", 332950: "Dark Deception",
         1290490: "UNBEATABLE [white label]"}
PART_B = {2143030, 332950, 1290490}
low = observelog.low32

paths = sys.argv[1:]
verdicts, seen = [], set()
if len({Path(p).resolve() for p in paths}) != len(paths):
    print("a log was given twice -> MISMATCH")
    verdicts.append(False)
for path in paths:
    app, name = observelog.app_of(path), Path(path).name
    if app not in GAMES or app in seen:
        print(f"{name}: game {app} not expected or given twice -> MISMATCH")
        verdicts.append(False)
        continue
    seen.add(app)
    calls = [c for c in observelog.calls(path) if c.interface == "SteamUtils009"]
    nine = [c for c in calls if c.position == 9]
    ok = bool(nine) and all(c.rax is not None and low(c.rax) == app for c in nine)
    verdicts.append(ok)
    print(f"{name} ({GAMES[app]}) A: {len(nine)} calls, results {sorted({low(c.rax) for c in nine if c.rax is not None})} -> {'match' if ok else 'MISMATCH'}")
    if app in PART_B:
        fourteen = [c for c in calls if c.position == 14]
        ok = bool(fourteen) and all(c.rax is not None and c.rax & 0xFF == 255 for c in fourteen)
        verdicts.append(ok)
        print(f"{name} ({GAMES[app]}) B: {len(fourteen)} calls, lowest bytes {sorted({c.rax & 0xFF for c in fourteen if c.rax is not None})} -> {'match' if ok else 'MISMATCH'}")
print("experiment 15:", "match" if seen == set(GAMES) and verdicts and all(verdicts) else "MISMATCH")
