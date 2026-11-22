"""Experiment 23 (docs/shim/meanings.md, loop iteration 10a): callback
number 0x44e (1102), candidate UserStatsStored_t (ISteamUserStats page:
m_nGameID uint64, m_eResult EResult), and the positions that trigger it,
candidate StoreStats: STEAMUSERSTATS_INTERFACE_VERSION011 position 10 in
Cave Crawlers, STEAMUSERSTATS_INTERFACE_VERSION013 position 9 in Bongo Cat.

0x44e is already in PAYLOAD_LISTED (experiment 20).

Predictions, per game (its trigger position named in GAMES):
A  The trigger position is called at least once, and there are exactly as
   many 0x44e callbacks as trigger calls. Paired in order, the i-th
   callback comes after the i-th trigger call and at most 1000 ms later; no
   0x44e callback comes before the first trigger call.
B  Every 0x44e callback has size 16, payload bytes 0..4 the game's app ID,
   bytes 4..8 zero, bytes 8..12 equal to 1 (k_EResultOK).
C  Every trigger call returns exactly 1 (a bool true, not a 64-bit call
   handle), and no Steam_GetAPICallResult call lies between the i-th
   trigger call and the i-th 0x44e callback (the callback is not fetched
   as a call result).
D  No trigger call has a text argument (IndicateAchievementProgress, the
   other function the page says triggers UserStatsStored_t, takes an
   achievement name; StoreStats takes nothing).

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the two games, one log per game, both
needed. Freshness (LOOP.md step 4): a log whose start (observelog.start_of)
is earlier than PLAN_COMMITTED is rejected. PLAN_COMMITTED is the committer
time (local, as run.sh writes starts.log) of the commit on loop/plan-10a
that holds the final plan text; it is set in a follow-up commit.

Never prints payload bytes or IDs; only counts, times and match/MISMATCH.

Usage: python tools/shim/experiments/023_stats_stored.py <log> <log>
"""
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {
    2143030: ("Cave Crawlers", "STEAMUSERSTATS_INTERFACE_VERSION011", 10),
    3419430: ("Bongo Cat", "STEAMUSERSTATS_INTERFACE_VERSION013", 9),
}
STORED = 0x44E
WINDOW_MS = 1000
PLAN_COMMITTED = datetime(2026, 11, 22, 23, 59, 59)  # placeholder, set in the follow-up commit


def u32(data, at):
    return int.from_bytes(data[at:at + 4], "little")


def stored_callbacks(path):
    """(ms, size, payload bytes or None) for every 0x44e callback."""
    out = []
    for e in observelog.exports(path):
        if e.kind != "wrote":
            continue
        h = observelog.header(e)
        if h is not None and h.u32(4) == STORED:
            out.append((e.ms, h.payload_size, h.payload_first))
    return out


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
        print(f"{name} ({GAMES[app][0]}): started {started}, before the plan commit {PLAN_COMMITTED} -> MISMATCH")
        verdicts.append(False)
        continue
    seen.add(app)
    game, interface, position = GAMES[app]
    triggers = [c for c in observelog.calls(path) if c.interface == interface and c.position == position]
    cbs = stored_callbacks(path)
    label = f"{name} ({game})"

    gaps = [cb[0] - t.ms for t, cb in zip(triggers, cbs)]
    ok_a = (bool(triggers) and len(cbs) == len(triggers)
            and all(0 <= g <= WINDOW_MS for g in gaps)
            and not any(cb[0] < triggers[0].ms for cb in cbs))
    verdicts.append(ok_a)
    print(f"{label} A: {len(triggers)} calls of position {position}, {len(cbs)} callbacks 0x44e, "
          f"gaps {gaps} ms -> {verdict(ok_a)}")

    ok_b = bool(cbs) and all(
        size == 16 and data is not None and len(data) >= 16
        and u32(data, 0) == app and u32(data, 4) == 0 and u32(data, 8) == 1
        for _, size, data in cbs)
    verdicts.append(ok_b)
    print(f"{label} B: size, app ID, zero bytes 4..8 and result 1 in every callback -> {verdict(ok_b)}")

    gacr = [e.ms for e in observelog.exports(path) if e.kind == "call" and e.name == "Steam_GetAPICallResult"]
    between = sum(1 for t, cb in zip(triggers, cbs) for at in gacr if t.ms <= at <= cb[0])
    ok_c = bool(triggers) and all(t.rax == 1 for t in triggers) and between == 0
    verdicts.append(ok_c)
    print(f"{label} C: results {sorted({t.rax == 1 for t in triggers})} (True = exactly 1), "
          f"{between} GetAPICallResult calls in between -> {verdict(ok_c)}")

    ok_d = bool(triggers) and all(not t.texts for t in triggers)
    verdicts.append(ok_d)
    print(f"{label} D: {sum(1 for t in triggers if t.texts)} trigger calls with a text argument -> {verdict(ok_d)}")
print("experiment 23:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
