"""Experiment 22 (docs/shim/meanings.md, loop iteration 10a): callback
number 703 (0x2bf), candidate SteamAPICallCompleted_t (ISteamUtils page:
m_hAsyncCall SteamAPICall_t, m_iCallback int, m_cubParam uint32), and the
entry point Steam_GetAPICallResult, candidate GetAPICallResult.

Needs 703 in PAYLOAD_LISTED (crates/shim-observe/src/proxy.rs), so that the
first payload bytes of every 703 callback are logged.

A 703 window is the time between a header with number 703 (the `wrote`
note of a Steam_BGetCallback call) and the next Steam_FreeLastCallback call
on the same thread (experiment 19, D). The Steam_GetAPICallResult call of a
window is the first one on that thread inside it.

Predictions:
P  Every 703 callback has a size of 16 and its first payload bytes logged
   (at least 16).
F  For every window with a Steam_GetAPICallResult call: payload bytes 0..8
   equal its `rdx` (all 64 bits) and bytes 12..16 equal the low 32 bits of
   its `r9`. At least 10 windows checked across all logs; a log with 50 or
   more Steam_GetAPICallResult calls cannot be checked (the proxy logs only
   the first 50) and counts as a mismatch.
G  Every 703 callback whose bytes 0..8 equal a result of a
   STEAMUSERSTATS_INTERFACE_VERSION013 position 15 call made (started)
   before that 703 (candidate
   RequestUserStats; the ISteamUserStats page: "to be used with a
   UserStatsReceived_t call result") has bytes 8..12 = 1101 (0x44d,
   UserStatsReceived_t, experiment 21) and bytes 12..16 = 24 (its recorded
   size). Checked at least once in each of Bongo Cat, Cartapli and Linsips.
H  Windows whose Steam_GetAPICallResult `rdx` was returned earlier by the
   same interface position all carry the same number in bytes 8..12 (one
   function, one result type). Only the count of numbers per position is
   printed, never the number.

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the five games, one log per game, all five
needed. Freshness (LOOP.md step 4): a log whose start (observelog.start_of)
is earlier than PLAN_COMMITTED is rejected. PLAN_COMMITTED is the committer
time (local, as run.sh writes starts.log) of the commit on loop/plan-10a
that holds the final plan text; it is set in a follow-up commit.

Never prints payload bytes, handles or IDs; only counts and match/MISMATCH.

Usage: python tools/shim/experiments/022_api_call_completed.py <log>...
"""
import sys
from collections import defaultdict
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {
    3419430: "Bongo Cat",
    5191630: "Coin Maker Simulator Demo",
    588430: "Fallout Shelter",
    4314560: "Cartapli : Fold Quest",
    4301370: "Linsips",
}
G_GAMES = {3419430, 4314560, 4301370}
COMPLETED = 703
STATS_RECEIVED = 0x44D  # UserStatsReceived_t, experiment 21
STATS_RECEIVED_SIZE = 24
REQUEST_USER_STATS = ("STEAMUSERSTATS_INTERFACE_VERSION013", 15)
LOGGED = 50  # LOG_FIRST in crates/shim-observe/src/proxy.rs
F_MIN = 10
PLAN_COMMITTED = datetime(2026, 11, 22, 23, 59, 59)  # placeholder, set in the follow-up commit
low = observelog.low32


def u32(data, at):
    return int.from_bytes(data[at:at + 4], "little")


def u64(data, at):
    return int.from_bytes(data[at:at + 8], "little")


def windows(path):
    """703 windows: dicts with ms, size, data (payload bytes or None) and
    gacr (the Steam_GetAPICallResult call line or None). Also returns the
    number of Steam_GetAPICallResult calls in the log."""
    out, open_by_thread, n_gacr = [], {}, 0
    for e in observelog.exports(path):
        if e.kind == "wrote":
            h = observelog.header(e)
            if h is None:
                continue
            w = None
            if h.u32(4) == COMPLETED:
                w = {"ms": e.ms, "size": h.payload_size, "data": h.payload_first, "gacr": None}
                out.append(w)
            open_by_thread[e.thread] = w
        elif e.kind == "call" and e.name == "Steam_FreeLastCallback":
            open_by_thread.pop(e.thread, None)
        elif e.kind == "call" and e.name == "Steam_GetAPICallResult":
            n_gacr += 1
            w = open_by_thread.get(e.thread)
            if w is not None and w["gacr"] is None:
                w["gacr"] = e
    return out, n_gacr


def sources(path):
    """For every 64-bit result of a method call: [(ms, (interface, position))]."""
    out = defaultdict(list)
    for c in observelog.calls(path):
        if c.rax is not None and c.rax > 0xFFFFFFFF:
            out[c.rax].append((c.ms, (c.interface, c.position)))
    return out


def source_of(src, value, ms):
    earlier = [s for at, s in src.get(value, []) if at <= ms]
    return earlier[-1] if earlier else None


paths = sys.argv[1:]
verdicts, seen = [], set()
f_checked_all = 0
g_by_game = defaultdict(int)
numbers_by_source = defaultdict(set)
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
    ws, n_gacr = windows(path)
    src = sources(path)

    ok_p = bool(ws) and all(w["size"] == 16 and w["data"] is not None and len(w["data"]) >= 16 for w in ws)
    verdicts.append(ok_p)
    shown = sum(1 for w in ws if w["data"] is not None)
    print(f"{name} ({GAMES[app]}) P: {len(ws)} callbacks 703, {shown} with payload bytes -> "
          f"{'match' if ok_p else 'MISMATCH'}")

    f_checked = f_ok = 0
    for w in ws:
        g = w["gacr"]
        if g is None or w["data"] is None or len(w["data"]) < 16:
            continue
        f_checked += 1
        rdx, r9 = g.reg("rdx"), g.reg("r9")
        if rdx is not None and r9 is not None and u64(w["data"], 0) == rdx and u32(w["data"], 12) == low(r9):
            f_ok += 1
        s = source_of(src, rdx, g.ms) if rdx is not None else None
        if s is not None:
            numbers_by_source[s].add(u32(w["data"], 8))
    ok_f = f_checked == f_ok and n_gacr < LOGGED
    f_checked_all += f_checked
    verdicts.append(ok_f)
    print(f"{name} ({GAMES[app]}) F: {f_checked} windows with GetAPICallResult checked, {f_ok} equal, "
          f"{n_gacr} calls logged -> {'match' if ok_f else 'MISMATCH'}")

    handles = [(c.ms, c.rax) for c in observelog.calls(path)
               if (c.interface, c.position) == REQUEST_USER_STATS and c.rax is not None]
    g_checked = g_ok = 0
    for w in ws:
        if w["data"] is None or len(w["data"]) < 16:
            continue
        if not any(rax == u64(w["data"], 0) and at <= w["ms"] for at, rax in handles):
            continue
        g_checked += 1
        if u32(w["data"], 8) == STATS_RECEIVED and u32(w["data"], 12) == STATS_RECEIVED_SIZE:
            g_ok += 1
    ok_g = g_checked == g_ok and (g_checked > 0 or app not in G_GAMES)
    g_by_game[app] = g_checked
    verdicts.append(ok_g)
    print(f"{name} ({GAMES[app]}) G: {g_checked} callbacks 703 for a position-15 handle, "
          f"{g_ok} with number 1101 and size 24 -> {'match' if ok_g else 'MISMATCH'}")

ok_f_all = f_checked_all >= F_MIN
verdicts.append(ok_f_all)
print(f"F across all logs: {f_checked_all} windows checked (at least {F_MIN}) -> {'match' if ok_f_all else 'MISMATCH'}")
ok_h = all(len(numbers) == 1 for numbers in numbers_by_source.values()) and bool(numbers_by_source)
verdicts.append(ok_h)
for (interface, position), numbers in sorted(numbers_by_source.items()):
    print(f"    H: {interface} position {position}: {len(numbers)} number(s)")
print(f"H: {len(numbers_by_source)} source positions, one number each -> {'match' if ok_h else 'MISMATCH'}")
print("experiment 22:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
