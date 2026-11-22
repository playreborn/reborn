"""Experiment 16 (docs/shim/meanings.md, loop iteration 9a): RunFrame.

Candidates: `SteamController007`/`SteamController008` position 2 and
`SteamInput001` position 2 / `SteamInput006` position 3 are `RunFrame`, which
both public pages describe as "performed automatically by
SteamAPI_RunCallbacks".

Predictions, for each named game (C = the SteamController call, I = the
SteamInput call of that game's versions):
A (results): C and I are each called at least 20 times (logged), and for
   each of them at least one call returns a lowest byte other than 0 and 1
   (a function returning bool sets it to 0 or 1 in every call).
B (pump): every logged I call is followed directly, on the same thread, by
   a C call, and at least 10 of these pairs are followed directly by a
   Steam_BGetCallback call (the callback pump).
C (one leftover): within a game, every logged C and I call returns one and
   the same register value.
D (nothing else): within a game, no other position of the game's
   SteamController and SteamInput versions is called (no Init, no action
   set, handle, LED, vibration or callback function).

Games: Cave Crawlers, Dark Deception, UNBEATABLE [white label]
(SteamController007 + SteamInput001); Bongo Cat, Linsips, Cartapli : Fold
Quest (SteamController008 + SteamInput006). Each log must be a different
file recorded by run.sh in starts.log for one of these games, started (its
`start` line) at or after PLAN, the time of the plan commit; one log per
game; all six games are needed. Anything else is a mismatch.

Usage: python tools/shim/experiments/016_run_frame.py <log>...
"""
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

OLD = (("SteamController007", 2), ("SteamInput001", 2))
NEW = (("SteamController008", 2), ("SteamInput006", 3))
GAMES = {2143030: ("Cave Crawlers", OLD), 332950: ("Dark Deception", OLD),
         1290490: ("UNBEATABLE [white label]", OLD), 3419430: ("Bongo Cat", NEW),
         4301370: ("Linsips", NEW), 4314560: ("Cartapli : Fold Quest", NEW)}
PLAN = "2026-11-22 12:42:38"  # local time of the plan commit (starts.log format)
CALL = re.compile(r"^(\d+) ms ThreadId\((\d+)\) (\S+) call ")


def sequence(path):
    """Every call line of the log as (thread, what), in order."""
    out = []
    for raw in open(path, encoding="utf-8", errors="replace"):
        m = CALL.match(raw)
        if m:
            out.append((m.group(2), m.group(3)))
    return out


def next_on_thread(seq, i):
    thread = seq[i][0]
    return next((what for th, what in seq[i + 1:] if th == thread), None)


def verdict(ok):
    return "match" if ok else "MISMATCH"


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
    game, ((c_if, c_pos), (i_if, i_pos)) = GAMES[app]
    calls = observelog.calls(path)
    c_calls = [c for c in calls if c.interface == c_if and c.position == c_pos]
    i_calls = [c for c in calls if c.interface == i_if and c.position == i_pos]

    def odd(cs):
        return any(c.rax is not None and c.rax & 0xFF not in (0, 1) for c in cs)

    ok = len(c_calls) >= 20 and len(i_calls) >= 20 and odd(c_calls) and odd(i_calls)
    verdicts.append(ok)
    print(f"{name} ({game}) A: {len(c_calls)} {c_if}#{c_pos} and {len(i_calls)} {i_if}#{i_pos} calls, "
          f"a result byte other than 0/1: {odd(c_calls)}/{odd(i_calls)} -> {verdict(ok)}")

    seq = sequence(path)
    c_name, i_name = f"{c_if}#{c_pos}", f"{i_if}#{i_pos}"
    pairs = unpaired = pumped = 0
    for i, (_, what) in enumerate(seq):
        if what != i_name:
            continue
        if next_on_thread(seq, i) != c_name:
            unpaired += 1
            continue
        pairs += 1
        j = next(k for k in range(i + 1, len(seq)) if seq[k][0] == seq[i][0])
        if next_on_thread(seq, j) == "Steam_BGetCallback":
            pumped += 1
    ok = pairs > 0 and unpaired == 0 and pumped >= 10
    verdicts.append(ok)
    print(f"{name} ({game}) B: {pairs} pairs, {unpaired} unpaired, {pumped} followed by "
          f"Steam_BGetCallback -> {verdict(ok)}")

    results = {c.rax for c in c_calls + i_calls}
    ok = len(results) == 1 and None not in results
    verdicts.append(ok)
    print(f"{name} ({game}) C: {len(results)} distinct result values -> {verdict(ok)}")

    others = sorted({f"{c.interface}#{c.position}" for c in calls
                     if c.interface in (c_if, i_if)
                     and (c.interface, c.position) not in ((c_if, c_pos), (i_if, i_pos))})
    ok = not others
    verdicts.append(ok)
    print(f"{name} ({game}) D: other positions called: {', '.join(others) or 'none'} -> {verdict(ok)}")
print("experiment 16:", "match" if seen == set(GAMES) and verdicts and all(verdicts) else "MISMATCH")
