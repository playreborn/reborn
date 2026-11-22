"""Experiment 18 (docs/shim/meanings.md, loop iteration 9a): SteamUser020
position 12.

Candidate: `GetVoiceOptimalSampleRate` (no arguments, returns the native
sample rate of the Steam voice decoder; the page gives 11025 to 48000 as the
decoder's range).

Prediction (Cave Crawlers): position 12 of SteamUser020 is called at least
once; every logged call returns, in its low 32 bits, one and the same value
from 11025 to 48000 with the upper 32 bits zero, and that value differs
from the low 32 bits of every SteamClient020 position 2 result
(ConnectToGlobalUser, the user handle that GetHSteamUser would return).

A match only narrows the position (see meanings.md: GetPlayerSteamLevel is
ruled out by judgement, not by the page). The result value is not printed,
only whether it is in range: it could be the account's Steam level.

The log must be recorded by run.sh in starts.log for Cave Crawlers, started
(its `start` line) at or after PLAN, the time of the plan commit; one log.
Anything else is a mismatch.

Usage: python tools/shim/experiments/018_voice_sample_rate.py <log>
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {2143030: "Cave Crawlers"}
low = observelog.low32
PLAN = "2026-11-22 12:42:38"  # local time of the plan commit (starts.log format)

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
    start = observelog.started_at(path)
    if start is None or start < PLAN:
        print(f"{name}: started {start}, before the plan commit {PLAN} -> MISMATCH")
        verdicts.append(False)
        continue
    calls = observelog.calls(path)
    twelve = [c for c in calls if c.interface == "SteamUser020" and c.position == 12]
    users = {low(c.rax) for c in calls
             if c.interface == "SteamClient020" and c.position == 2 and c.rax is not None}
    values = {c.rax for c in twelve}
    ok = (bool(twelve) and len(values) == 1 and None not in values
          and next(iter(values)) >> 32 == 0 and 11025 <= next(iter(values)) <= 48000
          and not values & users)
    verdicts.append(ok)
    print(f"{name} ({GAMES[app]}): one value from 11025 to 48000, not a user handle "
          f"-> {'match' if ok else 'MISMATCH'}")
print("experiment 18:", "match" if seen == set(GAMES) and verdicts and all(verdicts) else "MISMATCH")
