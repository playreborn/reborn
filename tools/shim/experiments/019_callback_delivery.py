"""Experiment 19 (docs/shim/meanings.md, loop iteration 9b): callback delivery.

Checks the 24-byte header the official client writes at `rdx` when
`Steam_BGetCallback` returns 1, and the order of the three callback entry
points. Reads only logs that have the `payload:` notes of step 4d
(ADR 0050).

Predictions:
A  Every header's bytes 0..4 equal U (low 32 bits of the first result of
   SteamClient position 2 in the same log). Across all logs, at least one
   header comes from a logged Steam_BGetCallback call whose `rcx` (low 32
   bits) differs from its bytes 0..4 (this rules out "bytes 0..4 are a copy
   of `rcx`").
A2 For a header whose Steam_BGetCallback call line is logged, the
   Steam_FreeLastCallback and Steam_GetAPICallResult calls that belong to it
   have the same `rcx` (low 32 bits) as that call.
B  Every header has 24 bytes and a `payload: size N sha256 ...` note with N
   equal to bytes 16..20; bytes 20..24 are 0.
C  The same number (bytes 4..8) always comes with the same size, within each
   log and across all logs given.
D  Per thread, each header is followed by exactly one Steam_FreeLastCallback
   call before that thread's next logged Steam_BGetCallback line, and no
   Steam_FreeLastCallback call comes without an unfreed callback. Every
   Steam_BGetCallback result's lowest byte is 0 or 1.
E  Every Steam_GetAPICallResult call lies between a header with the number
   703 and its Steam_FreeLastCallback, on the same thread; each such window
   holds exactly one. At least one log must have a 703 callback.
F  Only if the first payload bytes of 703 are logged (PAYLOAD_LISTED): bytes
   0..8 equal `rdx` of the Steam_GetAPICallResult call in the window, and
   bytes 12..16 equal its `r9`. Otherwise reported as not checked.

Each log must be a different file that run.sh recorded in starts.log
(observelog.app_of); at least 3 different games. Logs with more than 50
Steam_FreeLastCallback or Steam_GetAPICallResult calls cannot be checked for
D and E (the proxy logs only the first 50) and count as a mismatch.

Usage: python tools/shim/experiments/019_callback_delivery.py <log>...
"""
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

COMPLETED = 703  # the number after which Steam_GetAPICallResult was seen (entry-points.md)
LOGGED = 50  # LOG_FIRST in crates/shim-observe/src/proxy.rs
low = observelog.low32


def handles(path):
    """P and U: low 32 bits of the first SteamClient positions 0 and 2."""
    p = u = None
    for c in observelog.calls(path):
        if not c.interface.startswith("SteamClient") or c.rax is None:
            continue
        if c.position == 0 and p is None:
            p = low(c.rax)
        if c.position == 2 and u is None:
            u = low(c.rax)
    return p, u


def check(path, sizes_all):
    """Verdicts for one log: {prediction: (ok or None, note)}."""
    out = {}
    lines = observelog.exports(path)
    heads = [(e, observelog.header(e)) for e in lines if e.kind == "wrote"]
    p, u = handles(path)

    # A and B, and the sizes for C.
    first_words, a_ok, b_ok, sizes = set(), True, True, defaultdict(set)
    for _, h in heads:
        first_words.add(h.u32(0))
        a_ok &= u is not None and h.u32(0) == u
        b_ok &= (len(h.raw) == 24 and h.payload_size is not None
                 and h.payload_size == h.u32(16) and h.u32(20) == 0)
        sizes[h.u32(4)].add(h.u32(16))
        sizes_all[h.u32(4)].add(h.u32(16))

    # D, E, F: walk the entry point lines per thread.
    free_calls = [e for e in lines if e.name == "Steam_FreeLastCallback" and e.kind == "call"]
    api_calls = [e for e in lines if e.name == "Steam_GetAPICallResult" and e.kind == "call"]
    checkable = len(free_calls) <= LOGGED and len(api_calls) <= LOGGED
    open_cb = {}  # thread -> [header, frees, api calls, rcx of the call or None]
    last_call = {}  # thread -> (call number, low 32 bits of rcx)
    d_ok, e_ok, low_bytes = True, True, set()
    a2_ok, a2_checked, differs = True, 0, 0
    windows_703, f_checked, f_ok = 0, 0, True
    for e in lines:
        state = open_cb.get(e.thread)
        if e.name == "Steam_BGetCallback":
            if e.kind == "return" and e.rest.startswith("rax="):
                low_bytes.add(e.reg("rax") & 0xFF)
            if state is not None:  # next logged BGetCallback line: window ends
                d_ok &= state[1] == 1
                del open_cb[e.thread]
            if e.kind == "call":
                last_call[e.thread] = (e.number, low(e.reg("rcx")))
            if e.kind == "wrote":
                h = observelog.header(e)
                number, rcx = last_call.get(e.thread, (None, None))
                pipe = rcx if number == e.number else None
                differs += pipe is not None and pipe != h.u32(0)
                open_cb[e.thread] = [h, 0, [], pipe]
        elif e.name == "Steam_FreeLastCallback" and e.kind == "call":
            if state is None or state[1] >= 1:
                d_ok = False
                continue
            state[1] += 1
            if state[3] is not None:
                a2_checked += 1
                a2_ok &= low(e.reg("rcx")) == state[3]
            h, apis = state[0], state[2]
            if h.u32(4) == COMPLETED:
                windows_703 += 1
                e_ok &= len(apis) == 1
                if h.payload_first is not None and len(h.payload_first) >= 16 and len(apis) == 1:
                    f_checked += 1
                    first = h.payload_first
                    f_ok &= (int.from_bytes(first[0:8], "little") == apis[0].reg("rdx")
                             and int.from_bytes(first[12:16], "little") == apis[0].reg("r9"))
            else:
                e_ok &= not apis
        elif e.name == "Steam_GetAPICallResult" and e.kind == "call":
            if state is None or state[1] >= 1:
                e_ok = False
                continue
            state[2].append(e)
            if state[3] is not None:
                a2_checked += 1
                a2_ok &= low(e.reg("rcx")) == state[3]
    for state in open_cb.values():  # a callback still open at the end
        d_ok &= state[1] == 1
    d_ok &= low_bytes <= {0, 1}

    if not heads:
        a_ok = b_ok = False
    out["A"] = (a_ok, f"{len(heads)} headers, bytes 0..4 {sorted(first_words)}, P={p} U={u}, "
                f"{differs} from a call whose rcx differs")
    out["A2"] = (a2_ok, f"{a2_checked} frees and GetAPICallResult calls with a logged poll")
    out["B"] = (b_ok, "size note equals bytes 16..20, bytes 20..24 zero")
    out["C"] = (all(len(s) == 1 for s in sizes.values()),
                f"{len(sizes)} numbers, {sum(len(s) > 1 for s in sizes.values())} with more than one size")
    out["D"] = (d_ok and checkable,
                f"{len(free_calls)} frees for {len(heads)} headers, result lowest bytes {sorted(low_bytes)}"
                + ("" if checkable else ", more than 50 calls: not checkable"))
    out["E"] = (e_ok and checkable, f"{len(api_calls)} GetAPICallResult calls, {windows_703} callbacks 703")
    out["F"] = (f_ok if f_checked else None,
                f"{f_checked} of {windows_703} 703 callbacks with payload bytes")
    return out, windows_703, differs


paths = sys.argv[1:]
verdicts, games, any_703, f_any, sizes_all = [], set(), False, False, defaultdict(set)
any_differs = 0
if len({Path(p).resolve() for p in paths}) != len(paths):
    print("a log was given twice -> MISMATCH")
    verdicts.append(False)
for path in paths:
    name, app = Path(path).name, observelog.app_of(path)
    if app is None:
        print(f"{name}: not recorded by run.sh in starts.log -> MISMATCH")
        verdicts.append(False)
        continue
    if not any(e.kind == "wrote" and "payload:" in e.rest for e in observelog.exports(path)):
        print(f"{name} (app {app}): no payload notes (step 4d) -> MISMATCH")
        verdicts.append(False)
        continue
    games.add(app)
    result, n703, differs = check(path, sizes_all)
    any_703 |= n703 > 0
    any_differs += differs
    for key, (ok, note) in result.items():
        if ok is None:
            print(f"{name} (app {app}) {key}: {note} -> not checked")
            continue
        f_any |= key == "F"
        verdicts.append(ok)
        print(f"{name} (app {app}) {key}: {note} -> {'match' if ok else 'MISMATCH'}")
across = all(len(s) == 1 for s in sizes_all.values())
verdicts.append(across)
print(f"C across all logs: {len(sizes_all)} numbers -> {'match' if across else 'MISMATCH'}")
for number in sorted(sizes_all):
    print(f"    number {number} (0x{number:x}): size {sorted(sizes_all[number])}")
if len(games) < 3:
    print(f"only {len(games)} different games, 3 needed -> MISMATCH")
    verdicts.append(False)
if not any_differs:
    print("no header from a call whose rcx differs from bytes 0..4 (A needs one) -> MISMATCH")
    verdicts.append(False)
if not any_703:
    print("no log with a 703 callback (E needs one) -> MISMATCH")
    verdicts.append(False)
if not f_any:
    print("F not checked (703 payload bytes not logged)")
print("experiment 19:", "match" if verdicts and all(verdicts) else "MISMATCH")
