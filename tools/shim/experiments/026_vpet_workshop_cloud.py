"""Experiment 26 (docs/shim/meanings.md, loop iteration 13a): positions
VPet-Simulator (1920960) called in experiment 25 that a public page can
explain. Candidates from https://partner.steamgames.com/doc/api/ISteamUGC,
.../ISteamUtils, .../ISteamUser and .../ISteamRemoteStorage (read
2026-11-22); EResult 1 = k_EResultOK from .../steam_api, as in experiment 23.

Interfaces: U = STEAMUGC_INTERFACE_VERSION020, T = SteamUtils010,
S = SteamUser023, R = STEAMREMOTESTORAGE_INTERFACE_VERSION016.

Needs READS (crates/shim-observe, ADR 0043/0053), added by the main
session before the run (the plan names them, see meanings.md):
  (U, 5, OutputBytes("r9")), (U, 9, OutputBytes("r9")),
  (R, 19, OutputBytes("r8")).
Optional, only with the maintainer's OK (meanings.md says why):
  (T, 13, OutputBytes("r8")).
Without one of the three entries the prediction that needs it is a
MISMATCH; what follows from each outcome is fixed in meanings.md.

The proxy logs the first 50 calls of each (interface, position) and then
every 10000th; calls with a number above 50 are left out below.

Predictions:
U2  At least one call of U 2; every call has rdx in 0..21 (EUGCQuery), r8
    in 0..12 (EUGCMatchingUGCType), the low 32 bits of r9 equal to the
    game's app ID (nCreatorAppID), and a result that is not 0, has upper 32
    bits zero and differs from every earlier result of U 2.
U4  Every call of U 4 passes in rdx the result of the latest U 2 call before
    it, and no other position of U is called between that U 2 and U 4 (no
    SetReturn... call); every result has non-zero upper 32 bits (a 64-bit
    call handle, not a bool or a uint32).
T   For every result A of U 4: every call of T 11 or T 13 with rdx = A
    comes after that U 4; T 13 is called with A exactly once, the last
    T 11 call with A before it returned 1, and no T 11 call with A comes
    after it; every T 11 result is 0 or 1 and every T 13 result is 1
    (lowest byte, upper 32 bits zero); T 13 has the same r9 (cubCallback)
    in every call, at least 16; T 11 has at least two different r9 values.
    Only call handles whose T 13 call comes before the 50th logged T 11 call
    (without a T 13 call: whose U 4 comes before the 50th logged T 13 call)
    are checked, at least one; later calls may be missing from the log.
T13 (optional READS) After T 13 with rdx = A returned: bytes 0..8 at r8 are
    the handle the U 4 that returned A was given, bytes 8..12 are 1. Without
    the entry T13 prints "not logged" and has no verdict (meanings.md fixes
    what that leaves open).
U5  Every call of U 5 and of U 9 passes in rdx a result of U 2 whose U 4
    call result was already given to a T 13 that returned 1, and in r8 an
    index; per handle the indexes of each position run 0, 1, 2, ... in
    order, at most 49 (kNumUGCResultsPerPage is 50); every result has
    lowest byte 1.
H5  After every call of U 5: bytes 8..12 at r9 are 1 (m_eResult); when the
    next U call is U 9 or U 12 for the same handle and index and the next
    U 77 call after it comes before the next U 5, bytes 0..8 at r9
    (m_nPublishedFileId) equal rdx of that U 77 call; at least one pair.
H9  After every call of U 9: bytes 0..4 at r9 are the ASCII text "http".
U12 Every call of U 12 passes a U 2 handle in rdx, an index in r8 and a
    value in 0..12 (EItemStatistic) in r9, with lowest result byte 1; per
    (handle, index) the r9 values run 0, 1, 2, ... in order; at least one
    (handle, index) has all of 0..12.
U77 Every call of U 77 has a result with lowest byte 0 and r9 equal to r9
    of the latest U 9 call before it.
S12 S 12 is called exactly once; its result is from 11025 to 48000 (the
    range the page gives for the voice decoder), upper 32 bits zero, not the
    user handle (SteamClient021 position 2's result), and equal to the
    result of SteamUser020 position 12 in Cave Crawlers' run of experiment
    18 (observe-52912.log, read from the log folder; a missing file is a
    MISMATCH). Values are never printed.
R0  At least two calls of R 0; every call has a text in rdx, r8 at least
    0x10000, 0 < r9 < 0x80000000 and a result with lowest byte 1 and upper
    32 bits zero; no text is passed to R 0 twice (compared, never printed).
R18 A save sequence is the R 18 and R 19 calls before a call of R 0 (after
    the previous R 0). In each sequence all of whose calls have numbers up
    to 49, every R 18 returns the same N, and the R 19 calls have rdx 0, 1,
    ..., N-1 in order. At least two such sequences follow each other
    (sequence k ends with the k-th R 0 call, k+1 with the next); for each
    such pair N(k+1) = N(k) + 1.
H19 In each sequence k+1 of such a pair, the first four bytes at r8 after
    one of its R 19 calls (int32 pnFileSizeInBytes) equal r9 of the k-th
    R 0 call.

One log, recorded by run.sh in starts.log (observelog.app_of) for
VPet-Simulator; given twice or for another game it is rejected. Freshness
(LOOP.md step 4): a log whose start (observelog.start_of) is earlier than
PLAN_COMMITTED is rejected. PLAN_COMMITTED is the committer time (local,
as run.sh writes starts.log) of the commit on loop/plan-13a that holds the
final plan text; it is set in a follow-up commit.

Never prints texts, IDs, handles, sizes or other register values; only
positions, counts and match/MISMATCH.

Usage: python tools/shim/experiments/026_vpet_workshop_cloud.py <log>
"""
import re
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {1920960: "VPet-Simulator"}
U = "STEAMUGC_INTERFACE_VERSION020"
T = "SteamUtils010"
S = "SteamUser023"
R = "STEAMREMOTESTORAGE_INTERFACE_VERSION016"
LOGGED = 50
REFERENCE = "observe-52912.log"  # Cave Crawlers, experiment 18
PLAN_COMMITTED = datetime(2026, 11, 22, 0, 0, 0)  # set in the follow-up commit

OUT = re.compile(r"after (rdx|r8|r9)->\[([0-9a-f ]+)\]")


def out_bytes(call, register):
    """The bytes the proxy read at `register` after `call` returned, or None."""
    for extra in call.extra:
        m = OUT.search(extra)
        if m and m.group(1) == register:
            return bytes.fromhex(m.group(2).replace(" ", ""))
    return None


def u32(data, at):
    return int.from_bytes(data[at:at + 4], "little")


def u64(data, at):
    return int.from_bytes(data[at:at + 8], "little")


def low(value):
    return None if value is None else value & 0xFF


def hi(value):
    return None if value is None else value >> 32


def verdict(ok):
    return "match" if ok else "MISMATCH"


def check(path, app):
    label = f"{Path(path).name} ({GAMES[app]})"
    everything = observelog.calls(path)
    cs = [c for c in everything if c.number <= LOGGED]
    ok = []

    def say(name, good, text):
        ok.append(good)
        print(f"{label} {name}: {text} -> {verdict(good)}")

    def of(interface, position):
        return [c for c in cs if c.interface == interface and c.position == position]

    ugc = [c for c in cs if c.interface == U]
    index = {id(c): i for i, c in enumerate(cs)}

    # U2: CreateQueryAllUGCRequest.
    creates = of(U, 2)
    seen, fresh = set(), True
    for c in creates:
        if c.rax in seen:
            fresh = False
        seen.add(c.rax)
    good = bool(creates) and fresh and all(
        c.reg("rdx") is not None and c.reg("rdx") <= 21
        and c.reg("r8") is not None and c.reg("r8") <= 12
        and observelog.low32(c.reg("r9")) == app
        and c.rax not in (None, 0) and hi(c.rax) == 0 for c in creates)
    say("U2", good, f"{len(creates)} calls, arguments in the page's ranges and r9 the app ID, distinct handles")
    handles = {c.rax for c in creates}

    # U4: SendQueryUGCRequest.
    sends, bad = of(U, 4), 0
    sent_handle = {}
    for c in sends:
        before = [x for x in ugc if index[id(x)] < index[id(c)]]
        last_create = next((x for x in reversed(before) if x.position == 2), None)
        between = [x for x in before if last_create is not None and index[id(x)] > index[id(last_create)]]
        if (last_create is None or observelog.low32(c.reg("rdx")) != observelog.low32(last_create.rax)
                or between or c.rax is None or hi(c.rax) == 0):
            bad += 1
        else:
            sent_handle[c.rax] = (last_create.rax, c)
    say("U4", bool(sends) and bad == 0, f"{len(sends)} calls, {bad} not sending the latest new handle as a 64-bit call")

    # T: IsAPICallCompleted (11) and GetAPICallResult (13).
    t11, t13 = of(T, 11), of(T, 13)
    # Past the 50th logged call of T 11 (or T 13) calls may be missing from
    # the log, so a call handle is checked only up to there.
    cap11 = next((index[id(c)] for c in t11 if c.number == LOGGED), len(cs))
    cap13 = next((index[id(c)] for c in t13 if c.number == LOGGED), len(cs))
    bad, done, skipped = 0, {}, 0
    for result, (_, send) in sent_handle.items():
        mine11 = [c for c in t11 if c.reg("rdx") == result]
        mine13 = [c for c in t13 if c.reg("rdx") == result]
        if (mine13 and index[id(mine13[0])] >= cap11) or (not mine13 and index[id(send)] >= cap13):
            skipped += 1
            continue
        if any(index[id(c)] < index[id(send)] for c in mine11 + mine13) or len(mine13) != 1:
            bad += 1
            continue
        got = mine13[0]
        before = [c for c in mine11 if index[id(c)] < index[id(got)]]
        after = [c for c in mine11 if index[id(c)] > index[id(got)]]
        if not before or low(before[-1].rax) != 1 or after:
            bad += 1
            continue
        done[result] = got
    values11 = all(c.rax is not None and hi(c.rax) == 0 and c.rax in (0, 1) for c in t11)
    values13 = all(c.rax is not None and hi(c.rax) == 0 and low(c.rax) == 1 for c in t13)
    sizes13 = {c.reg("r9") for c in t13}
    sizes11 = {c.reg("r9") for c in t11}
    good = (bool(done) and bad == 0 and values11 and values13 and len(sizes13) == 1
            and all(v is not None and v >= 16 for v in sizes13) and len(sizes11) >= 2)
    say("T", good, f"{len(sent_handle)} call handles ({skipped} past the logging limit), {bad} out of order, "
        f"results 0/1 and 1: {values11}/{values13}, "
        f"one size at 13: {len(sizes13) == 1}, sizes at 11: {len(sizes11)}")

    # T13: the call result bytes (optional READS).
    read13 = [(r, c, out_bytes(c, "r8")) for r, c in done.items()]
    logged = [x for x in read13 if x[2] is not None and len(x[2]) >= 12]
    if not logged:
        print(f"{label} T13: not logged (optional READS entry) -> no verdict")
    else:
        good = len(logged) == len(read13) and all(
            u64(data, 0) == sent_handle[r][0] and u32(data, 8) == 1 for r, _, data in logged)
        say("T13", good, f"{len(logged)} of {len(read13)} call results read, handle and result 1 in each")

    # U5 / U9: per-item calls on a completed query.
    completed_at = {}
    for result, got in done.items():
        completed_at[sent_handle[result][0]] = index[id(got)]
    bad = 0
    for position in (5, 9):
        expected = {}
        for c in of(U, position):
            h, i = c.reg("rdx"), c.reg("r8")
            if (h not in completed_at or index[id(c)] < completed_at[h] or low(c.rax) != 1
                    or i != expected.get(h, 0) or i > 49):
                bad += 1
            expected[h] = (i or 0) + 1
    say("U5", bool(of(U, 5)) and bool(of(U, 9)) and bad == 0,
        f"{len(of(U, 5))} and {len(of(U, 9))} calls, {bad} not on a completed query in index order or not true")

    # H5: SteamUGCDetails_t at r9 after U 5.
    pairs, bad, unread = 0, 0, 0
    for c in of(U, 5):
        data = out_bytes(c, "r9")
        if data is None or len(data) < 12:
            unread += 1
            continue
        if u32(data, 8) != 1:
            bad += 1
        later = [x for x in ugc if index[id(x)] > index[id(c)]]
        nxt5 = next((k for k, x in enumerate(later) if x.position == 5), len(later))
        nxt77 = next((x for x in later[:nxt5] if x.position == 77), None)
        if later and later[0].position in (9, 12) and later[0].reg("rdx") == c.reg("rdx") \
                and later[0].reg("r8") == c.reg("r8") and nxt77 is not None:
            pairs += 1
            if u64(data, 0) != nxt77.reg("rdx"):
                bad += 1
    say("H5", unread == 0 and bad == 0 and pairs > 0,
        f"{unread} calls not read, {pairs} pairs with a following 77, {bad} not matching")

    # H9: the preview URL at r9 after U 9.
    nines = of(U, 9)
    datas = [out_bytes(c, "r9") for c in nines]
    unread = sum(1 for d in datas if d is None)
    http = sum(1 for d in datas if d is not None and d[:4] == b"http")
    say("H9", bool(nines) and unread == 0 and http == len(nines),
        f"{len(nines)} calls, {unread} not read, {http} starting with the text http")

    # U12: GetQueryUGCStatistic.
    twelves, bad = of(U, 12), 0
    runs = {}
    for c in twelves:
        key = (c.reg("rdx"), c.reg("r8"))
        value = c.reg("r9")
        if (key[0] not in handles or value is None or value > 12 or low(c.rax) != 1
                or value != len(runs.get(key, []))):
            bad += 1
        runs.setdefault(key, []).append(value)
    full = sum(1 for v in runs.values() if v == list(range(13)))
    say("U12", bool(twelves) and bad == 0 and full > 0,
        f"{len(twelves)} calls, {bad} out of shape, {full} items with all 13 statistics")

    # U77.
    sevens, bad = of(U, 77), 0
    for c in sevens:
        before9 = [x for x in nines if index[id(x)] < index[id(c)]]
        if low(c.rax) != 0 or not before9 or before9[-1].reg("r9") != c.reg("r9"):
            bad += 1
    say("U77", bool(sevens) and bad == 0, f"{len(sevens)} calls, {bad} not false or not with the 9 buffer")

    # S12: GetVoiceOptimalSampleRate.
    s12 = [c for c in everything if c.interface == S and c.position == 12]
    user = next((c.rax for c in everything if c.interface == "SteamClient021" and c.position == 2), None)
    ref = observelog.log_dir() / REFERENCE
    ref_value = None
    if ref.exists():
        ref_value = next((c.rax for c in observelog.calls(ref)
                          if c.interface == "SteamUser020" and c.position == 12), None)
    good = (len(s12) == 1 and s12[0].rax is not None and 11025 <= s12[0].rax <= 48000
            and s12[0].rax != user and ref_value is not None and s12[0].rax == ref_value)
    say("S12", good, f"{len(s12)} calls, one value in the decoder's range equal to the reference run's"
        f" (reference found: {ref_value is not None})")

    # R0: FileWrite.
    rs = [c for c in cs if c.interface == R]
    writes = [c for c in rs if c.position == 0]
    texts = [c.texts.get("rdx") for c in writes]
    good = (len(writes) >= 2 and all(t is not None for t in texts) and len(set(texts)) == len(texts)
            and all(c.reg("r8") is not None and c.reg("r8") >= 0x10000
                    and c.reg("r9") is not None and 0 < c.reg("r9") < 0x80000000
                    and c.rax is not None and low(c.rax) == 1 and hi(c.rax) == 0 for c in writes))
    say("R0", good, f"{len(writes)} calls, a new text, a buffer, a size and true in each")

    # R18 / H19: GetFileCount and GetFileNameAndSize before each write.
    every_rs = [c for c in everything if c.interface == R]
    sequences, current = [], []
    for c in every_rs:
        if c.position in (18, 19):
            current.append(c)
        elif c.position == 0:
            sequences.append((current, c))
            current = []
    complete, count_of = [], {}
    for k, (seq, write) in enumerate(sequences):
        counts = [c.rax for c in seq if c.position == 18]
        indexes = [c.reg("rdx") for c in seq if c.position == 19]
        whole = all(c.number < LOGGED for c in seq) and write.number <= LOGGED and counts
        if whole and len(set(counts)) == 1 and indexes == list(range(counts[0])):
            complete.append(k)
            count_of[k] = counts[0]
    pair_ks = [k for k in complete if k + 1 in complete]
    grows = all(count_of[k + 1] == count_of[k] + 1 for k in pair_ks)
    say("R18", bool(pair_ks) and grows,
        f"{len(sequences)} sequences, {len(complete)} complete, {len(pair_ks)} following pairs, count +1 in each: {grows}")
    bad, unread = 0, 0
    for k in pair_ks:
        size = sequences[k][1].reg("r9")
        reads = [out_bytes(c, "r8") for c in sequences[k + 1][0] if c.position == 19]
        unread += sum(1 for d in reads if d is None)
        if not any(d is not None and len(d) >= 4 and u32(d, 0) == size for d in reads):
            bad += 1
    say("H19", bool(pair_ks) and bad == 0 and unread == 0,
        f"{len(pair_ks)} pairs, {unread} calls of 19 not read, {bad} without the written size")
    print(f"{label}: {verdict(all(ok))}")
    return all(ok)


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
    verdicts.append(check(path, app))
print("experiment 26:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
