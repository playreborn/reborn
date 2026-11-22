"""Experiment 11 (docs/shim/meanings.md): the calls a game makes through
SteamClient021 or SteamClient023 when it closes.

Predictions, on the low 32 bits: the pipe handle P is the result of
position 0, the user handle U the result of position 2. At close the game
calls, in this order, position 4 with rdx = P and r8 = U (ReleaseUser),
position 1 with rdx = P and a lowest result byte of 1 (BReleaseSteamPipe),
then position 23 (021) or 22 (023) with a lowest result byte of 1
(BShutdownIfAllPipesClosed). A log without close calls counts neither way.

Usage: python tools/shim/experiments/011_close_calls.py <log>...
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

SHUTDOWN = {"SteamClient021": 23, "SteamClient023": 22}
low = observelog.low32
results, checked = [], 0
for path in sys.argv[1:]:
    calls = [c for c in observelog.calls(path) if c.interface in SHUTDOWN]
    if not calls:
        print(f"{Path(path).name}: no SteamClient021/023 calls")
        continue
    version = calls[0].interface
    first = {}
    for c in calls:
        first.setdefault(c.position, c)
    P, U = low(first[0].rax) if 0 in first else None, low(first[2].rax) if 2 in first else None
    shutdown = SHUTDOWN[version]
    # The close calls are the last calls at positions 4, 1 and the shutdown.
    close = [c for c in calls if c.position in (4, 1, shutdown) and c.ms > calls[0].ms + 5000]
    if not close:
        print(f"{Path(path).name} {version}: no close calls (counts neither way)")
        continue
    checked += 1
    seq = [c.position for c in close]
    ok = seq[-3:] == [4, 1, shutdown]
    by = {c.position: c for c in close}
    ok &= low(by[4].reg("rdx")) == P and low(by[4].reg("r8")) == U if 4 in by else False
    ok &= low(by[1].reg("rdx")) == P and (by[1].rax or 0) & 0xFF == 1 if 1 in by else False
    ok &= (by[shutdown].rax or 0) & 0xFF == 1 if shutdown in by else False
    results.append(ok)
    print(f"{Path(path).name} {version}: P={P} U={U} close calls {seq} -> {'match' if ok else 'MISMATCH'}")
print("experiment 11:", "match" if results and all(results) else ("no close calls in any log" if not results else "MISMATCH"))
