"""Experiment 25 (docs/shim/meanings.md, loop iteration 12a): first
recordings of three free games whose store pages list Stats and Steam
Achievements: VPet-Simulator (1920960), Pixel Puzzles Ultimate Jigsaw
Puzzles (351030) and Super Jigsaw Puzzle: Generations (1036950).

Two purposes:

S  Survey (data, no verdict): every interface and position the game
   called, with call counts (LOOP.md step 2 data for later rows).

StoreStats in whichever STEAMUSERSTATS_INTERFACE_VERSION0xx the game asks
for. The positions are not known in advance (the games were never
recorded), so the predictions are written over "any position of the
game's STEAMUSERSTATS_INTERFACE_VERSION0xx". A position T qualifies when
its calls pair one to one, in order, with the 0x44e callbacks of the log:
as many calls as callbacks, the i-th callback 0 to 1000 ms after the i-th
call, no 0x44e before the first call.

U  The game calls at least one position of a STEAMUSERSTATS_INTERFACE_
   VERSION0xx interface.
A  There is at least one 0x44e callback, and exactly one (interface,
   position) of the game's UserStats interfaces qualifies as T.
B  Every 0x44e callback has size 16, payload bytes 0..4 the game's app
   ID, bytes 4..8 zero, bytes 8..12 equal to 1 (k_EResultOK).
C  Every call of T returns a bool true (lowest byte 1, upper 32 bits
   zero), and no 703 callback between the i-th call of T and the i-th
   0x44e has 1102 in payload bytes 8..12; a 703 in between without logged
   payload bytes is a mismatch.
D  No call of T has a text argument (rules out IndicateAchievementProgress).
E  At least one call of T has a lowest rdx byte other than 0 and 1 (rules
   out ResetAllStats under experiment 23's assumption that a bool argument
   arrives as 0 or 1 in the lowest byte of rdx).
F  If T's interface is one with a confirmed StoreStats position
   (VERSION011 position 10, VERSION013 position 9, experiment 23), T is
   that position. For any other interface F only prints T (no verdict).

A run without a store (the game changed no stat) gives "nothing checked"
for A to E, which is a MISMATCH, not a match.

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the three games, one log per game, all
three needed for "experiment 25: match". The per-game verdict of A to F is
printed as well (the plan says what a per-game match is used for).
Freshness (LOOP.md step 4): a log whose start (observelog.start_of) is
earlier than PLAN_COMMITTED is rejected. PLAN_COMMITTED is the committer
time (local, as run.sh writes starts.log) of the commit on loop/plan-12a
that holds the final plan text; it is set in a follow-up commit.

Never prints payload bytes, IDs or register values; only interface names,
positions, counts, times and match/MISMATCH.

Usage: python tools/shim/experiments/025_new_stats_games.py <log> <log> <log>
"""
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {
    1920960: "VPet-Simulator",
    351030: "Pixel Puzzles Ultimate Jigsaw Puzzles",
    1036950: "Super Jigsaw Puzzle: Generations",
}
USERSTATS = "STEAMUSERSTATS_INTERFACE_VERSION"
CONFIRMED_STORE = {
    "STEAMUSERSTATS_INTERFACE_VERSION011": 10,
    "STEAMUSERSTATS_INTERFACE_VERSION013": 9,
}
STORED = 0x44E
COMPLETED = 703
WINDOW_MS = 1000
PLAN_COMMITTED = datetime(2026, 11, 22, 14, 50, 51)  # commit 09a72bb on loop/plan-12a (final plan)


def u32(data, at):
    return int.from_bytes(data[at:at + 4], "little")


def callbacks(path, number):
    """(ms, size, payload bytes or None) for every callback with `number`."""
    out = []
    for e in observelog.exports(path):
        if e.kind != "wrote":
            continue
        h = observelog.header(e)
        if h is not None and h.u32(4) == number:
            out.append((e.ms, h.payload_size, h.payload_first))
    return out


def is_true(rax):
    """A bool true: lowest byte 1, upper 32 bits zero (not a call handle)."""
    return rax is not None and rax & 0xFF == 1 and rax >> 32 == 0


def pairs(triggers, cbs):
    """Whether the calls pair one to one, in order, with the callbacks."""
    if not triggers or len(triggers) != len(cbs):
        return False
    if any(cb[0] < triggers[0].ms for cb in cbs):
        return False
    return all(0 <= cb[0] - t.ms <= WINDOW_MS for t, cb in zip(triggers, cbs))


def verdict(ok):
    return "match" if ok else "MISMATCH"


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
    all_calls = observelog.calls(path)
    game_verdicts = []

    # S: survey, no verdict.
    survey = Counter((c.interface, c.position) for c in all_calls)
    print(f"{label} S: {len(survey)} positions called")
    for (interface, position), count in sorted(survey.items()):
        print(f"    S: {interface} position {position}: {count} calls")

    stats = [c for c in all_calls if c.interface.startswith(USERSTATS)]
    ok_u = bool(stats)
    game_verdicts.append(ok_u)
    print(f"{label} U: {len(stats)} calls of {sorted({c.interface for c in stats})} -> {verdict(ok_u)}")

    cbs = callbacks(path, STORED)
    by_position = {}
    for c in stats:
        by_position.setdefault((c.interface, c.position), []).append(c)
    qualified = [k for k, ts in sorted(by_position.items()) if pairs(ts, cbs)]
    ok_a = bool(cbs) and len(qualified) == 1
    game_verdicts.append(ok_a)
    shown = [f"{i} position {p}" for i, p in qualified]
    print(f"{label} A: {len(cbs)} callbacks 0x44e, qualifying positions {shown} -> {verdict(ok_a)}")

    if ok_a:
        interface, position = qualified[0]
        triggers = by_position[qualified[0]]
    else:
        interface, position, triggers = None, None, []
    gaps = [cb[0] - t.ms for t, cb in zip(triggers, cbs)]
    if triggers:
        print(f"    A: {len(triggers)} calls of T, gaps {gaps} ms")

    ok_b = bool(triggers) and all(
        size == 16 and data is not None and len(data) >= 16
        and u32(data, 0) == app and u32(data, 4) == 0 and u32(data, 8) == 1
        for _, size, data in cbs)
    game_verdicts.append(ok_b)
    print(f"{label} B: size, app ID, zero bytes 4..8 and result 1 in every callback -> {verdict(ok_b)}")

    completed = callbacks(path, COMPLETED)
    inside = [c for t, cb in zip(triggers, cbs) for c in completed if t.ms <= c[0] <= cb[0]]
    unreadable = sum(1 for c in inside if c[2] is None or len(c[2]) < 12)
    announcing = sum(1 for c in inside if c[2] is not None and len(c[2]) >= 12 and u32(c[2], 8) == STORED)
    ok_c = (bool(triggers) and all(is_true(t.rax) for t in triggers)
            and unreadable == 0 and announcing == 0)
    game_verdicts.append(ok_c)
    print(f"{label} C: {sum(1 for t in triggers if is_true(t.rax))} of {len(triggers)} results a bool true, "
          f"{len(inside)} callbacks 703 in between ({unreadable} without payload bytes, "
          f"{announcing} announcing 1102) -> {verdict(ok_c)}")

    ok_d = bool(triggers) and all(not t.texts for t in triggers)
    game_verdicts.append(ok_d)
    print(f"{label} D: {sum(1 for t in triggers if t.texts)} calls of T with a text argument -> {verdict(ok_d)}")

    not_bool = sum(1 for t in triggers if t.reg("rdx") is not None and t.reg("rdx") & 0xFF not in (0, 1))
    ok_e = not_bool > 0
    game_verdicts.append(ok_e)
    print(f"{label} E: {not_bool} of {len(triggers)} calls of T with a lowest rdx byte other than 0 and 1 "
          f"-> {verdict(ok_e)}")

    if interface in CONFIRMED_STORE:
        ok_f = position == CONFIRMED_STORE[interface]
        game_verdicts.append(ok_f)
        print(f"{label} F: T is {interface} position {position}, confirmed StoreStats is position "
              f"{CONFIRMED_STORE[interface]} -> {verdict(ok_f)}")
    else:
        print(f"{label} F: T is {interface} position {position}, no confirmed StoreStats position there "
              f"(no verdict)")

    print(f"{label}: {verdict(all(game_verdicts))}")
    verdicts.extend(game_verdicts)
print("experiment 25:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
