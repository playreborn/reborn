"""Experiment 17 (docs/shim/meanings.md, loop iteration 9a): SteamFriends017.

Candidates: position 43 `SetRichPresence`, position 3 `GetFriendCount`,
position 4 `GetFriendByIndex`, position 36 `GetLargeFriendAvatar`.

Predictions:
A (Cave Crawlers): position 43 is called at least once; every logged call
   has a text in rdx and returns true (lowest byte 1), and at least one
   call also has a text in r8 (two texts: the only such function on the
   page; r8 of a call can hold an empty value, which the proxy does not
   log as text).
B (Please, Touch The Artwork 2; Fallout Shelter): position 3 is called at
   least once, always with 4 in the low 32 bits of rdx
   (k_EFriendFlagImmediate). Position 4 is called at least once; every
   logged call comes after a position 3 call, returns the address it got
   in rdx (a value returned through the caller's address), has in r9 the
   rdx of the latest position 3 call, and has in r8 an index from 0 to
   below the latest position 3 result.
C (Please, Touch The Artwork 2): position 36 is called at least once;
   every result, as a signed 32-bit number, is -1, 0 or positive; at least
   one is positive, and every positive result h is later given in rdx to
   SteamUtils010 position 5 (GetImageSize) and to SteamUtils010 position 6
   (GetImageRGBA) with a buffer size in r9 larger than 64*64*4 bytes.

Account data stays out of the output: Steam IDs, friend counts, indices,
call counts and avatar results of parts B and C are compared inside the
script and only match or mismatch is printed. Each log must be a different
file recorded by run.sh in starts.log for one of these games, started
(its `start` line) at or after PLAN, the time of the plan commit; one log
per game; all three games are needed. Anything else is a mismatch.

Usage: python tools/shim/experiments/017_friends.py <log>...
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

CC, PT, FS = 2143030, 2600140, 588430
GAMES = {CC: "Cave Crawlers", PT: "Please, Touch The Artwork 2", FS: "Fallout Shelter"}
FRIENDS = "SteamFriends017"
low = observelog.low32
PLAN = "2026-11-22 12:42:38"  # local time of the plan commit (starts.log format)


def signed32(value):
    v = value & 0xFFFFFFFF
    return v - (1 << 32) if v & 0x80000000 else v


def verdict(ok):
    return "match" if ok else "MISMATCH"


def part_a(calls):
    pos = [c for c in calls if c.interface == FRIENDS and c.position == 43]
    ok = (bool(pos) and all("rdx" in c.texts and c.rax is not None and c.rax & 0xFF == 1
                            for c in pos)
          and any("r8" in c.texts for c in pos))
    two = sum("r8" in c.texts for c in pos)
    return ok, f"{len(pos)} position-43 calls, {two} with two texts"


def part_b(calls):
    three = four = bad = 0
    latest = None
    for c in calls:
        if c.interface != FRIENDS or c.position not in (3, 4):
            continue
        if c.position == 3:
            three += 1
            if low(c.reg("rdx")) != 4 or c.rax is None:
                bad += 1
                latest = None
            else:
                latest = (low(c.reg("rdx")), signed32(c.rax))
            continue
        four += 1
        r8 = low(c.reg("r8"))
        if (latest is None or c.rax is None or c.rax != c.reg("rdx")
                or low(c.reg("r9")) != latest[0] or r8 is None or not 0 <= r8 < latest[1]):
            bad += 1
    ok = three > 0 and four > 0 and bad == 0
    return ok, "positions 3 and 4 checked"


def part_c(calls):
    avatars = [c for c in calls if c.interface == FRIENDS and c.position == 36]
    results = [None if c.rax is None else signed32(c.rax) for c in avatars]
    shapes_ok = bool(avatars) and all(r is not None and r >= -1 for r in results)
    checked = bad = 0
    for c, r in zip(avatars, results):
        if r is None or r <= 0:
            continue
        checked += 1
        later = [u for u in calls if u.ms >= c.ms and u.interface == "SteamUtils010"
                 and low(u.reg("rdx")) == r]
        size = any(u.position == 5 for u in later)
        rgba = any(u.position == 6 and (u.reg("r9") or 0) > 64 * 64 * 4 for u in later)
        if not (size and rgba):
            bad += 1
    ok = shapes_ok and checked > 0 and bad == 0
    return ok, "position 36 checked"


PARTS = {CC: [("A", part_a)], PT: [("B", part_b), ("C", part_c)], FS: [("B", part_b)]}

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
    start = observelog.started_at(path)
    if start is None or start < PLAN:
        print(f"{name}: started {start}, before the plan commit {PLAN} -> MISMATCH")
        verdicts.append(False)
        continue
    seen.add(app)
    calls = observelog.calls(path)
    for part, check in PARTS[app]:
        ok, text = check(calls)
        verdicts.append(ok)
        print(f"{name} ({GAMES[app]}) {part}: {text} -> {verdict(ok)}")
print("experiment 17:", "match" if seen == set(GAMES) and verdicts and all(verdicts) else "MISMATCH")
