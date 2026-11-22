"""Experiment 27 (docs/shim/meanings.md, loop iteration 13a):
STEAMSCREENSHOTS_INTERFACE_VERSION003 position 3, called once at start by
HoloCure - Save the Fans! (2420510, M4 reference run) and Pixel Puzzles
Ultimate Jigsaw Puzzles (351030, experiment 25). Candidates from
https://partner.steamgames.com/doc/api/ISteamScreenshots (read 2026-11-22,
nine functions).

Predictions, per game (X = STEAMSCREENSHOTS_INTERFACE_VERSION003):
A  X position 3 is called exactly once, and no other position of X is
   called.
B  Its rdx has lowest byte 1 (true for HookScreenshots' bool).
C  Its result has a lowest byte other than 0 and 1 (not a bool: rules out
   IsScreenshotsHooked, SetLocation, TagPublishedFile and TagUser) and
   non-zero upper 32 bits (not a 32-bit ScreenshotHandle, under the
   assumption that a 32-bit result clears them: rules out
   AddScreenshotToLibrary, AddVRScreenshotToLibrary and WriteScreenshot).
D  Every callback number delivered (Steam_BGetCallback `wrote` lines) from
   the time of the call line of position 3 to 10 000 ms after it also
   occurs in at least one reference log of tools/shim/games.json (M4 games
   and extra entries) whose game never calls any X position
   (TriggerScreenshot "Triggers a ScreenshotReady_t callback";
   such a number would be new). The numbers outside that set are printed
   (callback numbers are not account data).

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the two games, one log per game, both
needed for "experiment 27: match". Freshness (LOOP.md step 4): a log whose
start (observelog.start_of) is earlier than PLAN_COMMITTED is rejected.
PLAN_COMMITTED is the committer time (local, as run.sh writes starts.log)
of the commit on loop/plan-13a that holds the final plan text; it is set
in a follow-up commit.

Never prints register values; only positions, counts, callback numbers and
match/MISMATCH.

Usage: python tools/shim/experiments/027_screenshots_hook.py <log> <log>
"""
import json
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {
    2420510: "HoloCure - Save the Fans!",
    351030: "Pixel Puzzles Ultimate Jigsaw Puzzles",
}
X = "STEAMSCREENSHOTS_INTERFACE_VERSION003"
WINDOW_MS = 10_000
PLAN_COMMITTED = datetime(2026, 11, 22, 19, 14, 46)  # commit 212f413 on loop/plan-13a (final plan; a4fb521 before the rebase, same time)


def numbers(path, start=None, end=None):
    """Callback numbers of the `wrote` lines of a log, optionally in a window."""
    out = set()
    for e in observelog.exports(path):
        if e.kind != "wrote" or (start is not None and not start <= e.ms <= end):
            continue
        h = observelog.header(e)
        if h is not None and h.u32(4) is not None:
            out.add(h.u32(4))
    return out


def baseline():
    """Callback numbers of the reference logs whose game never calls X."""
    data = json.loads((Path(__file__).resolve().parents[1] / "games.json").read_text(encoding="utf-8"))
    games = data["games"] + data.get("extra", [])
    out, used = set(), 0
    for game in games:
        path = observelog.log_dir() / game[3]
        if not path.exists() or any(c.interface == X for c in observelog.calls(path)):
            continue
        out |= numbers(path)
        used += 1
    return out, used


def verdict(ok):
    return "match" if ok else "MISMATCH"


known, used = baseline()
print(f"baseline: {used} reference logs without {X}, {len(known)} callback numbers")
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
    label = f"{name} ({GAMES[app]})"
    xs = [c for c in observelog.calls(path) if c.interface == X]
    threes = [c for c in xs if c.position == 3]
    others = sorted({c.position for c in xs if c.position != 3})
    ok_a = len(threes) == 1 and not others
    print(f"{label} A: {len(threes)} calls of position 3, other positions {others} -> {verdict(ok_a)}")
    call = threes[0] if threes else None
    rdx = call.reg("rdx") if call else None
    ok_b = rdx is not None and rdx & 0xFF == 1
    print(f"{label} B: lowest rdx byte 1 -> {verdict(ok_b)}")
    rax = call.rax if call else None
    ok_c = rax is not None and rax & 0xFF not in (0, 1) and rax >> 32 != 0
    print(f"{label} C: result neither a bool nor a 32-bit value -> {verdict(ok_c)}")
    ok_d = False
    if call is not None and rax is not None:
        new = sorted(numbers(path, call.ms, call.ms + WINDOW_MS) - known)
        ok_d = used > 0 and not new
        print(f"{label} D: callback numbers in the {WINDOW_MS} ms after the call not in the baseline: {new} "
              f"-> {verdict(ok_d)}")
    else:
        print(f"{label} D: no call -> MISMATCH")
    game_ok = ok_a and ok_b and ok_c and ok_d
    print(f"{label}: {verdict(game_ok)}")
    verdicts.append(game_ok)
print("experiment 27:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
