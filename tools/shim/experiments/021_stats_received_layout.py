"""Experiment 21 (docs/shim/meanings.md): experiment 20 with the layout its result suggested: callback number 0x44d (1101),
candidate UserStatsReceived_t, and STEAMUSERSTATS_INTERFACE_VERSION011
position 0, candidate RequestCurrentStats.

Predictions, for Cave Crawlers, Gravitas and Dark Deception:
A. Exactly one callback with number 0x44d; its data has size 24; bytes
   0..4 of the data are the game's app ID; bytes 8..12 are 1 (k_EResultOK)
   or 2 (k_EResultFail, "if the user has no stats").
B. The 8 bytes at 12..20 of the data are the same in all three logs (the
   same user), and its upper four bytes are not zero. Only match or
   mismatch is printed, never the value.
C. STEAMUSERSTATS_INTERFACE_VERSION011 position 0 is called before the
   0x44d callback, and the callback comes at most 5 seconds after the
   first such call.

Each log must be a different file recorded by run.sh in starts.log for
one of these games, one per game, all three needed.

Usage: python tools/shim/experiments/021_stats_received_layout.py <log> <log> <log>
"""
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {2143030: "Cave Crawlers", 1067310: "Gravitas", 332950: "Dark Deception"}
CALLBACK = re.compile(
    r"^(\d+) ms .*Steam_BGetCallback return \d+ wrote rdx->\[[0-9a-f]{8} ([0-9a-f]{8})"
    r".* payload: size (\d+) sha256 [0-9a-f]+(?: first \[([0-9a-f ]+)\])?")


def callbacks(path, number):
    """(ms, size, data bytes or None) for every callback with `number`."""
    out = []
    for raw in open(path, encoding="utf-8", errors="replace"):
        m = CALLBACK.match(raw.strip())
        if m and int.from_bytes(bytes.fromhex(m.group(2)), "little") == number:
            data = bytes.fromhex(m.group(4).replace(" ", "")) if m.group(4) else None
            out.append((int(m.group(1)), int(m.group(3)), data))
    return out


u32 = lambda b, at: int.from_bytes(b[at:at + 4], "little")  # noqa: E731
paths = sys.argv[1:]
verdicts, seen, users = [], set(), []
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
    cbs = callbacks(path, 0x44D)
    ok_a = (len(cbs) == 1 and cbs[0][1] == 24 and cbs[0][2] is not None
            and len(cbs[0][2]) == 24 and u32(cbs[0][2], 0) == app and u32(cbs[0][2], 8) in (1, 2))
    if ok_a:
        users.append(cbs[0][2][12:20])
    verdicts.append(ok_a)
    result = u32(cbs[0][2], 8) if cbs and cbs[0][2] is not None and len(cbs[0][2]) >= 12 else None
    print(f"{name} ({GAMES[app]}) A: {len(cbs)} callbacks 0x44d, size {[c[1] for c in cbs]}, "
          f"app ID {'matches' if ok_a else 'checked'}, result {result} -> {'match' if ok_a else 'MISMATCH'}")
    starts = [c.ms for c in observelog.calls(path)
              if c.interface == "STEAMUSERSTATS_INTERFACE_VERSION011" and c.position == 0]
    ok_c = bool(cbs) and bool(starts) and starts[0] < cbs[0][0] <= starts[0] + 5000
    verdicts.append(ok_c)
    gap = cbs[0][0] - starts[0] if cbs and starts else None
    print(f"{name} ({GAMES[app]}) C: callback {gap} ms after the first position-0 call -> {'match' if ok_c else 'MISMATCH'}")
ok_b = len(users) == 3 and len(set(users)) == 1 and users[0][4:8] != bytes(4)
verdicts.append(ok_b)
print(f"B: bytes 12..20 the same in {len(users)} logs -> {'match' if ok_b else 'MISMATCH'}")
print("experiment 21:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
