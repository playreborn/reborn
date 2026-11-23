"""Experiment 34 (docs/shim/meanings.md, loop iteration 17a): the
inventory result callbacks 4700 and 4701 and
STEAMINVENTORY_INTERFACE_V003 positions 6, 21 and 26. Candidates from
https://partner.steamgames.com/doc/api/ISteamInventory.

Games (tools/shim/games.json):
  Cave Crawlers (2143030)  positions 6, 21; callbacks 4700, 4701, 4702  (group CC)
  Bongo Cat (3419430)      positions 6, 26; callbacks 4700, 4701, 703   (group BC)

Needs PAYLOAD_LISTED (crates/shim-observe, ADR 0050 and 0054), added on
the plan's branch: 4700 and 4701. Their candidates carry a result handle
and an EResult (SteamInventoryResultReady_t) or a result handle
(SteamInventoryFullUpdate_t); no ticket, token or password. If either
number is another callback, its 8 bytes could hold a Steam ID (possibly
another user's); the bytes stay in the local log and this script prints
only match or mismatch (decided by the agent under the maintainer's
delegation, ADR 0060).
703 is already listed (experiment 22); 4702 is checked by its size only.
The handle position 6 writes is read since experiment 14 (READS, output
bytes at rdx). No READS entry is added.

Assumptions (stated in the plan):
  A1  A bool, 8-, 16- or 32-bit argument or result (an enum or a
      SteamInventoryResult_t included) arrives with the upper 32 bits of
      its 64-bit register zero (experiments 23, 24, 31, 32).
  A2  The size in a 703 payload (bytes 12..16) is the size of the call
      result struct, and a struct's size lies between the sum of its
      fields and its size under natural alignment (experiment 32).
  A3  A pointer a function writes through is NULL or an address of at
      least 0x10000; a call given a non-zero value below 0x10000 there,
      after which the game runs on, is not that function.
  A4  Every inventory result a call creates gets a 4700 callback while
      the game runs (the page: ResultReady "is fired whenever an inventory
      result transitions from k_EResultPending to any other completed
      state"), so a call that creates a result shows a 4700 with a handle
      no other call wrote.
  A5  A call handle in a 703 (bytes 0..8) is the SteamAPICall_t of a
      call that created it; a value returned by a void function (a
      leftover in rax) or an update handle (StartUpdateProperties) is not
      one, at least not when no other logged call returned it.
  A6  Callback 4701 fires only after GetAllItems, as the page says of
      SteamInventoryFullUpdate_t ("Triggered when GetAllItems successfully
      returns a result which is newer / fresher than the last known
      result").
  A7  4700 and 4701 are inventory callbacks: 4700
      SteamInventoryResultReady_t, 4701 SteamInventoryFullUpdate_t. No
      page gives callback numbers; only the sizes and the handle match
      tie them to the inventory, and a small handle could match by
      chance. Bongo Cat (6's handle out of three live ones) is the
      stronger evidence.

Predictions, both groups (S = STEAMINVENTORY_INTERFACE_V003):
H1  S position 6 is called at least once; every call returns 1 and has
    an `after rdx` note whose first four bytes are a handle below 0x10000.
H2  At least one 4701 header is logged; every 4701 has payload size 4 and
    payload bytes, its bytes 0..4 equal a handle a call of position 6
    logged before it wrote, and the next callback header after it is a
    4700 whose bytes 0..4 are the same handle (the page: ResultReady
    "will still be triggered immediately afterwards").
H3  Every 4700 has payload size 8 and payload bytes; its bytes 0..4 are a
    handle the game passes afterwards in the low 32 bits of rdx to S
    position 1 or 3 (confirmed GetResultTimestamp, experiment 14).

Group CC only:
C1  Every 4700's bytes 0..4 equal a handle written by position 6 (so no
    other call created a result, A4: positions 20 and 21 do not).
C2  At least one 4702 header, each with payload size 1 (the page's
    SteamInventoryDefinitionUpdate_t has no fields).
C3  S position 21 is called at least once before the first 4702 and at
    least once after it; every call before returns exactly 0, every call
    after returns exactly 1. After the first 4702 there is a call with
    rdx 0 directly followed (next S call) by a call of 21 with a non-zero
    rdx and the same r8.
C4  Every non-zero rdx of position 21 has non-zero upper 32 bits (not a
    32-bit value under A1); at least one call of 21 has non-zero upper 32
    bits in r9 (not GetItemsWithPrices' uint32 length, A1); at least one
    call has a non-zero r9 below 0x10000 (no pointer to write through, A3).

Group BC only:
B1  S position 26 is called at least once; every result has non-zero
    upper 32 bits and is returned by no other logged call.
B2  For every result of 26 exactly one 703 payload has bytes 0..8 equal
    to it; all of these have size 8 (bytes 12..16) and one number (bytes
    8..12), which is not 1101 (UserStatsReceived_t, experiment 21).

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the two games, one log per game. A group
is decided by its own game's log; the last line says whether both
matched. Freshness (LOOP.md step 4): a log whose start
(observelog.start_of) is earlier than PLAN_COMMITTED is rejected.
PLAN_COMMITTED is the committer time (local, as run.sh writes starts.log)
of the commit on the plan branch that holds the final plan text and this
script; it is set in a follow-up commit.

Prints only positions, counts and match/MISMATCH; no addresses, handles,
callback numbers, item data or account data.

Usage: python tools/shim/experiments/034_inventory_results.py <log>...
"""
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

INVENTORY = "STEAMINVENTORY_INTERFACE_V003"
GAMES = {
    2143030: ("Cave Crawlers", "CC"),
    3419430: ("Bongo Cat", "BC"),
}
COMPLETED = 703
STATS_RECEIVED = 0x44D  # UserStatsReceived_t, experiments 21 and 22
READY, FULL, DEFINITIONS = 4700, 4701, 4702
PLAN_COMMITTED = datetime(2026, 11, 23, 2, 24, 12)  # commit 161034c on loop/plan-17a


def verdict(ok):
    return "match" if ok else "MISMATCH"


def high(value):
    return value is not None and value >> 32 != 0


def callbacks(path):
    """[(ms, number, size, first payload bytes or None)] of every header."""
    out = []
    for e in observelog.exports(path):
        if e.kind != "wrote":
            continue
        h = observelog.header(e)
        if h is None:
            continue
        out.append((e.ms, h.u32(4), h.payload_size, h.payload_first))
    return out


def u32_at(data, at):
    if data is None or len(data) < at + 4:
        return None
    return int.from_bytes(data[at:at + 4], "little")


def check_common(calls, cbs, say):
    """H1 to H3. Returns [(ms, handle)] of the handles position 6 wrote."""
    gets = [c for c in calls if c.position == 6]
    written = [(c.ms, observelog.after_u32(c, "rdx")) for c in gets]
    ok_h1 = bool(gets) and all(c.rax == 1 for c in gets) and all(
        h is not None and h < 0x10000 for _, h in written)
    say("H1", ok_h1, f"{len(gets)} calls of 6, all true with a small handle written: {ok_h1}")

    fulls = [i for i, cb in enumerate(cbs) if cb[1] == FULL]
    good = 0
    for i in fulls:
        ms, _, size, first = cbs[i]
        handle = u32_at(first, 0)
        before = {h for wms, h in written if wms <= ms}
        nxt = cbs[i + 1] if i + 1 < len(cbs) else None
        if (size == 4 and handle is not None and handle in before and nxt is not None
                and nxt[1] == READY and u32_at(nxt[3], 0) == handle):
            good += 1
    say("H2", bool(fulls) and good == len(fulls),
        f"{good} of {len(fulls)} callbacks 4701 with size 4, a handle of 6 and a 4700 for it next")

    readers = [c for c in calls if c.position in (1, 3)]
    readies = [cb for cb in cbs if cb[1] == READY]
    good = 0
    for ms, _, size, first in readies:
        handle = u32_at(first, 0)
        later = {observelog.low32(c.reg("rdx")) for c in readers if c.ms >= ms}
        if size == 8 and handle is not None and handle in later:
            good += 1
    say("H3", bool(readies) and good == len(readies),
        f"{good} of {len(readies)} callbacks 4700 with size 8 and a handle read later by 1 or 3")
    return written, readies


def check_cc(calls, cbs, written, readies, say):
    handles = {h for _, h in written if h is not None}
    own = [cb for cb in readies if u32_at(cb[3], 0) in handles]
    say("C1", bool(readies) and len(own) == len(readies),
        f"{len(own)} of {len(readies)} callbacks 4700 for a handle of 6")

    defs = [cb for cb in cbs if cb[1] == DEFINITIONS]
    say("C2", bool(defs) and all(cb[2] == 1 for cb in defs),
        f"{len(defs)} callbacks 4702, all size 1: {all(cb[2] == 1 for cb in defs)}")
    if not defs:
        say("C3-C4", False, "no 4702, nothing more to check")
        return
    first_def = defs[0][0]
    ids = [c for c in calls if c.position == 21]
    before = [c for c in ids if c.ms < first_def]
    after = [c for c in ids if c.ms >= first_def]
    pair = False
    for at, c in enumerate(calls):
        if c.position != 21 or c.ms < first_def:
            continue
        nxt = calls[at + 1] if at + 1 < len(calls) else None
        if (c.reg("rdx") == 0 and nxt is not None and nxt.position == 21
                and nxt.reg("rdx") not in (None, 0) and nxt.reg("r8") == c.reg("r8")):
            pair = True
    ok_c3 = (bool(before) and bool(after) and all(c.rax == 0 for c in before)
             and all(c.rax == 1 for c in after) and pair)
    say("C3", ok_c3, f"{len(before)} calls of 21 before the first 4702 (all false: "
        f"{all(c.rax == 0 for c in before)}), {len(after)} after (all true: "
        f"{all(c.rax == 1 for c in after)}), NULL-then-buffer pair: {pair}")

    buffers = [c.reg("rdx") for c in ids if c.reg("rdx") not in (None, 0)]
    wide = all(high(v) for v in buffers)
    wide_r9 = any(high(c.reg("r9")) for c in ids)
    small_r9 = any(c.reg("r9") is not None and 0 < c.reg("r9") < 0x10000 for c in ids)
    say("C4", bool(buffers) and wide and wide_r9 and small_r9,
        f"buffers wide: {bool(buffers) and wide}, a wide r9: {wide_r9}, "
        f"a small non-zero r9: {small_r9}")


def check_bc(path, calls, cbs, say):
    prices = [c for c in calls if c.position == 26]
    results = [c.rax for c in prices]
    others = [c for c in observelog.calls(path) if c.rax is not None and not (
        c.interface == INVENTORY and c.position == 26)]
    other_results = {c.rax for c in others}
    ok_b1 = (bool(prices) and all(high(r) for r in results)
             and not set(results) & other_results and len(set(results)) == len(results))
    say("B1", ok_b1, f"{len(prices)} calls of 26, all with a 64-bit result no other call returned: {ok_b1}")

    done = []
    for ms, number, size, first in cbs:
        if number != COMPLETED:
            continue
        if first is None or len(first) < 16:
            done.append((None, None, None))
            continue
        done.append((int.from_bytes(first[0:8], "little"), u32_at(first, 8), u32_at(first, 12)))
    matched = [[d for d in done if d[0] == r] for r in results]
    ok_one = bool(results) and all(len(m) == 1 for m in matched)
    flat = [d for m in matched for d in m]
    numbers = {d[1] for d in flat}
    ok_b2 = (ok_one and all(d[2] == 8 for d in flat) and len(numbers) == 1
             and STATS_RECEIVED not in numbers)
    say("B2", ok_b2, f"{sum(len(m) for m in matched)} callbacks 703 for {len(results)} results of 26 "
        f"(one each: {ok_one}), all size 8: {all(d[2] == 8 for d in flat)}, "
        f"numbers: {len(numbers)}, not 1101: {STATS_RECEIVED not in numbers}")


def check(path, group, label):
    verdicts = []

    def say(point, ok, text):
        verdicts.append(ok)
        print(f"{label} {point}: {text} -> {verdict(ok)}")

    calls = [c for c in observelog.calls(path) if c.interface == INVENTORY]
    cbs = callbacks(path)
    written, readies = check_common(calls, cbs, say)
    if group == "CC":
        check_cc(calls, cbs, written, readies, say)
    else:
        check_bc(path, calls, cbs, say)
    ok = all(verdicts)
    print(f"{label} group {group}: {verdict(ok)}")
    return ok


def main(paths):
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
        game, group = GAMES[app]
        started = observelog.start_of(path)
        if PLAN_COMMITTED is None or started is None or started < PLAN_COMMITTED:
            print(f"{name} ({game}): started {started}, before the plan commit {PLAN_COMMITTED} -> MISMATCH")
            verdicts.append(False)
            continue
        seen.add(app)
        verdicts.append(check(path, group, f"{name} ({game})"))
    print("experiment 34:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")


if __name__ == "__main__":
    main(sys.argv[1:])
