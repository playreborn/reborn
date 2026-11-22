"""Experiment 32 (docs/shim/meanings.md, loop iteration 16a): the
achievement count and names of STEAMUSERSTATS_INTERFACE_VERSION011 and 012
(positions 14 and 15) and the leaderboard calls of 012 (23, 29) and 013
(22, 23, 30). Candidates from
https://partner.steamgames.com/doc/api/ISteamUserStats.

Games (tools/shim/games.json):
  Outcore: Desktop Adventure (1275670)  ...011 14, 15  (group N011)
  One's Own Exile (3086720)             ...012 14, 15  (group N012)
  Fallout Shelter (588430)              ...012 23, 29  (group L012)
  Pixel Puzzles Ultimate (351030)       ...013 22, 23, 30  (group L013)

Needs READS (crates/shim-observe, ADR 0043 and 0053), added on the plan's
branch, all of the kind `result text` (the text rax points to, 2 to 64
printable characters, or `result: no text`):
  ("STEAMUSERSTATS_INTERFACE_VERSION011", 15, Read::ResultText)
  ("STEAMUSERSTATS_INTERFACE_VERSION012", 15, Read::ResultText)
  ("STEAMUSERSTATS_INTERFACE_VERSION013", 23, Read::ResultText)
Every function on the page that returns a text returns the game's public
data (GetAchievementName, GetAchievementDisplayAttribute,
GetLeaderboardName). Without the reads N3, N4 and T1 mismatch. Groups
L012 and the call-result parts of L013 use the 703 payloads that
PAYLOAD_LISTED already has (experiment 22).

Assumptions (stated in the plan):
  A1  A bool, 8-, 16- or 32-bit argument or result (an enum included)
      arrives with the upper 32 bits of its 64-bit register zero (as in
      experiments 23, 24 and 31; about the compiled code, not the page).
  A2  The size in a 703 payload (bytes 12..16) is the size of the call
      result struct (experiment 22, F), and a struct's size lies between
      the sum of its fields' sizes and its size under natural alignment
      (packing only lowers alignment).
  A3  A value a game takes as the end of the indices it passes to a
      function that returns a name is a count of those names
      (GetNumAchievements, not GetTrophySpaceRequiredBeforeInstall, which
      has the same shape).

Predictions, groups N011 and N012 (S = the game's UserStats interface):
N0  Before the first call of S position 14, S was called only at
    position 0, and no 703 callback header is logged before the first
    call of position 15 (so no leaderboard handle can exist yet).
N1  Position 14 is called at least once; every call returns the same
    value N with 4 <= N <= 49 and upper 32 bits zero.
N2  Position 15 is called exactly N times; the low 32 bits of rdx run
    0, 1, ..., N-1 in call order.
N3  Every position-15 call has a `result->"..."` note; the N texts differ.
N4  (N012 only) Every position-15 call is followed, as the next S call,
    by a position-6 call whose rdx text equals the position-15 result
    text.

Predictions, groups L012 (Fallout Shelter, find = 23, download = 29) and
L013 (Pixel Puzzles, find = 22, upload = 30, name = 23):
F1  The find position is called at least once; every call has a text at
    rdx and a result with non-zero upper 32 bits; at least one call has
    non-zero upper 32 bits in r8, and at least one has low 32 bits of r8
    outside 0..=2, the page's ELeaderboardSortMethod values (so not
    FindOrCreateLeaderboard, whose second parameter is that enum; the low
    32 bits need no A1).
F2  At least one 703 payload has bytes 0..8 equal to a find result; every
    such 703 has size 16 (bytes 12..16), and they all carry one number
    (bytes 8..12).
D1  (L012) Position 29 is called at least once; every call has non-zero
    upper 32 bits in r8 and in its result, and low 32 bits of r8 outside
    0..=3 (ELeaderboardDataRequest) and 0..=2
    (ELeaderboardUploadScoreMethod), so not DownloadLeaderboardEntries or
    UploadLeaderboardScore, whose second parameter is one of those enums
    (no A1 needed).
D2  (L012) At least one 703 payload has bytes 0..8 equal to a position-29
    result; every such 703 has size 24 and a number other than 1101
    (UserStatsReceived_t, experiments 21 and 22) and other than F2's
    number, and they all carry one number.
U1  (L013) Position 30 is called at least once; every call's rdx equals
    the rdx of a later position-23 call or of another position-30 call
    (a leaderboard handle used again) and its result has non-zero upper
    32 bits.
U2  (L013) At least one 703 payload has bytes 0..8 equal to a position-30
    result; every such 703 has size 32, they all carry one number, and it
    is not F2's.
T1  (L013) Position 23 is called at least once; every call has a
    `result->"..."` note whose text equals the rdx text of a find call
    (position 22) made before it, and its rdx equals the rdx of a
    position-30 call.

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the four games, one log per game. A group
is decided by its own game's log; the last line says whether all four
matched. Freshness (LOOP.md step 4): a log whose start
(observelog.start_of) is earlier than PLAN_COMMITTED is rejected.
PLAN_COMMITTED is the committer time (local, as run.sh writes starts.log)
of the commit on the plan branch that holds the final plan text and this
script; it is set in a follow-up commit.

Prints only positions, counts and match/MISMATCH; no addresses, handles,
callback numbers, texts or account data.

Usage: python tools/shim/experiments/032_achievement_names_and_leaderboards.py <log>...
"""
import re
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

V011 = "STEAMUSERSTATS_INTERFACE_VERSION011"
V012 = "STEAMUSERSTATS_INTERFACE_VERSION012"
V013 = "STEAMUSERSTATS_INTERFACE_VERSION013"
GAMES = {
    1275670: ("Outcore: Desktop Adventure", "N011"),
    3086720: ("One's Own Exile", "N012"),
    588430: ("Fallout Shelter", "L012"),
    351030: ("Pixel Puzzles Ultimate Jigsaw Puzzles", "L013"),
}
COMPLETED = 703
STATS_RECEIVED = 0x44D  # UserStatsReceived_t, experiments 21 and 22
LOGGED = 50  # LOG_FIRST in crates/shim-observe/src/proxy.rs
PLAN_COMMITTED = datetime(2026, 11, 22, 21, 44, 12)  # commit fa474dc on loop/plan-16a (final plan, after review)
# Enum values on the ISteamUserStats page.
SORT_METHODS = {0, 1, 2}  # ELeaderboardSortMethod
DATA_REQUESTS = {0, 1, 2, 3}  # ELeaderboardDataRequest
UPLOAD_METHODS = {0, 1, 2}  # ELeaderboardUploadScoreMethod
RESULT_TEXT = re.compile(r'^result->"(.*)"$')


def verdict(ok):
    return "match" if ok else "MISMATCH"


def result_text(call):
    """The text of the `result->"..."` note of a listed call, or None."""
    for extra in call.extra:
        m = RESULT_TEXT.match(extra)
        if m:
            return m.group(1)
    return None


def completions(path):
    """[(ms, handle, number, size)] of every 703 header with payload bytes."""
    out = []
    for e in observelog.exports(path):
        if e.kind != "wrote":
            continue
        h = observelog.header(e)
        if h is None or h.u32(4) != COMPLETED:
            continue
        b = h.payload_first
        if b is None or len(b) < 16:
            out.append((e.ms, None, None, None))
            continue
        out.append((e.ms, int.from_bytes(b[0:8], "little"),
                    int.from_bytes(b[8:12], "little"), int.from_bytes(b[12:16], "little")))
    return out


def high(value):
    return value is not None and value >> 32 != 0


def check_names(path, interface, with_n4, say):
    calls = [c for c in observelog.calls(path) if c.interface == interface]
    counts = [c for c in calls if c.position == 14]
    names = [c for c in calls if c.position == 15]
    first14 = calls.index(counts[0]) if counts else len(calls)
    before = {c.position for c in calls[:first14]}
    first15_ms = names[0].ms if names else None
    early_703 = [ms for ms, *_ in completions(path) if first15_ms is not None and ms < first15_ms]
    say("N0", bool(counts) and before <= {0} and not early_703,
        f"positions before the first 14: {sorted(before)}, {len(early_703)} callbacks 703 before the first 15")

    values = {c.rax for c in counts}
    n = values.pop() if len(values) == 1 else None
    ok_n1 = n is not None and 4 <= n <= LOGGED - 1
    say("N1", ok_n1, f"{len(counts)} calls of 14, one value: {n is not None}, value in 4..=49: {ok_n1}")
    if not ok_n1:
        say("N2-N4", False, "no count, nothing more to check")
        return
    indices = [observelog.low32(c.reg("rdx")) for c in names]
    say("N2", indices == list(range(n)),
        f"{len(names)} calls of 15, indices 0..N-1 in order: {indices == list(range(n))}")
    texts = [result_text(c) for c in names]
    ok_n3 = bool(texts) and None not in texts and len(set(texts)) == len(texts)
    say("N3", ok_n3, f"{sum(t is not None for t in texts)} of {len(names)} calls with a result text, "
        f"all different: {len(set(texts)) == len(texts)}")
    if not with_n4:
        return
    follow = 0
    for c, text in zip(names, texts):
        at = calls.index(c)
        nxt = calls[at + 1] if at + 1 < len(calls) else None
        if text is not None and nxt is not None and nxt.position == 6 and nxt.texts.get("rdx") == text:
            follow += 1
    say("N4", bool(names) and follow == len(names),
        f"{follow} of {len(names)} calls of 15 followed by a 6 with the same name")


def check_leaderboards(path, interface, find, second, size, name_pos, say):
    calls = [c for c in observelog.calls(path) if c.interface == interface]
    done = completions(path)
    finds = [c for c in calls if c.position == find]
    wide = any(high(c.reg("r8")) for c in finds)
    not_sort = any(observelog.low32(c.reg("r8")) not in SORT_METHODS for c in finds)
    ok_f1 = (bool(finds) and all("rdx" in c.texts and high(c.rax) for c in finds)
             and wide and not_sort)
    say("F1", ok_f1, f"{len(finds)} calls of {find}, all with a text and a 64-bit result: "
        f"{all('rdx' in c.texts and high(c.rax) for c in finds)}, "
        f"one with a wide r8: {wide}, one with r8 no sort method: {not_sort}")
    found = {c.rax for c in finds}
    f_done = [(num, sz) for _, h, num, sz in done if h in found]
    f_numbers = {num for num, _ in f_done}
    say("F2", bool(f_done) and all(sz == 16 for _, sz in f_done) and len(f_numbers) == 1,
        f"{len(f_done)} callbacks 703 for a find result, all size 16: "
        f"{all(sz == 16 for _, sz in f_done)}, numbers: {len(f_numbers)}")

    seconds = [c for c in calls if c.position == second]
    handles = {c.rax for c in seconds}
    s_done = [(num, sz) for _, h, num, sz in done if h in handles]
    s_numbers = {num for num, _ in s_done}
    if name_pos is None:
        ok_d1 = bool(seconds) and all(
            high(c.reg("r8")) and high(c.rax)
            and observelog.low32(c.reg("r8")) not in DATA_REQUESTS | UPLOAD_METHODS
            for c in seconds)
        say("D1", ok_d1, f"{len(seconds)} calls of {second}, all with a wide r8 that is no "
            f"data request or upload method, and a 64-bit result: {ok_d1}")
        ok_d2 = (bool(s_done) and all(sz == size for _, sz in s_done) and len(s_numbers) == 1
                 and not s_numbers & ({STATS_RECEIVED} | f_numbers))
        say("D2", ok_d2, f"{len(s_done)} callbacks 703 for a {second} result, all size {size}: "
            f"{all(sz == size for _, sz in s_done)}, numbers: {len(s_numbers)}, "
            f"other than 1101 and F2's: {not s_numbers & ({STATS_RECEIVED} | f_numbers)}")
        return
    names = [c for c in calls if c.position == name_pos]
    used = [c.reg("rdx") for c in names] + [c.reg("rdx") for c in seconds]
    ok_u1 = bool(seconds) and all(used.count(c.reg("rdx")) >= 2 and high(c.rax) for c in seconds)
    say("U1", ok_u1, f"{len(seconds)} calls of {second}, handle used again and 64-bit result: {ok_u1}")
    ok_u2 = (bool(s_done) and all(sz == size for _, sz in s_done) and len(s_numbers) == 1
             and not s_numbers & f_numbers)
    say("U2", ok_u2, f"{len(s_done)} callbacks 703 for a {second} result, all size {size}: "
        f"{all(sz == size for _, sz in s_done)}, other than F2's: {not s_numbers & f_numbers}")
    upload_rdx = {c.reg("rdx") for c in seconds}
    good = 0
    for c in names:
        text = result_text(c)
        earlier = {f.texts.get("rdx") for f in finds if f.ms <= c.ms}
        if text is not None and text in earlier and c.reg("rdx") in upload_rdx:
            good += 1
    say("T1", bool(names) and good == len(names),
        f"{good} of {len(names)} calls of {name_pos} with a find name as result and a used handle")


def check(path, group, label):
    verdicts = []

    def say(point, ok, text):
        verdicts.append(ok)
        print(f"{label} {point}: {text} -> {verdict(ok)}")

    if group == "N011":
        check_names(path, V011, False, say)
    elif group == "N012":
        check_names(path, V012, True, say)
    elif group == "L012":
        check_leaderboards(path, V012, 23, 29, 24, None, say)
    else:
        check_leaderboards(path, V013, 22, 30, 32, 23, say)
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
    print("experiment 32:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")


if __name__ == "__main__":
    main(sys.argv[1:])
