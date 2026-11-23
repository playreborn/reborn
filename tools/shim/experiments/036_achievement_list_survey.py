"""Experiment 36 (docs/shim/meanings.md, "Experiment 36", loop iteration
18a): why the achievement names of experiment 35 differ at most indices,
with the survey of ADR 0061.

STEAMUSERSTATS_INTERFACE_VERSION011 and 012 position 14
(GetNumAchievements, confirmed under A1 and A3) and position 15
(GetAchievementName, confirmed), experiment 32. In experiment 35 the
official count matched the length of Reborn's list, but the official name
at index i differed from Reborn's i-th name in 26 of 29 calls (Outcore)
and 15 of 16 (One's Own Exile). ADR 0061 adds three lines the proxy writes
once per process, after the call that completed the official list
(crates/shim-observe/src/daemon.rs, AchievementSurvey; proxy.rs,
answer_asks):

  "<prefix> shadow set: official N (distinct d, no text k), Reborn M
       (distinct e); shared s, official-only o, Reborn-only r"
  "<prefix> shadow set normalised: shared s, official-only o, Reborn-only r"
  "<prefix> shadow map: 0:j 1:j ..."   (j a Reborn index, "none" or "notext")

where <prefix> is "<ms> ms ThreadId(<t>) <Interface>#<pos> return <n>" of
the completing call. The per-call notes of ADR 0059 ("shadow match",
"shadow mismatch", ...) stay as in experiment 35.

Games (known, the only recorded games that call these positions):
Outcore: Desktop Adventure (1275670, 011) and One's Own Exile (3086720,
012).

This is a survey: the result is open. Preconditions (match/MISMATCH, the
tool worked), per log:
S  (survey, no verdict) versions, calls per position, per-call note kinds.
A  At least one call of 14 and one of 15 on the game's version, each with
   a result line.
B  Exactly one "shadow set:", one "shadow set normalised:" and one
   "shadow map:" line, each with the prefix of a call of the four
   positions that has a result line, on that call's thread, after it;
   official N equals the official count from 14 (one value), and the map
   has exactly the pairs 0 to N-1 in order.
C  The map agrees with the per-call notes: for each index i with exactly
   one "shadow match" or "shadow mismatch" note (and no other), the note
   is "match" exactly when the map gives j = i.

Reading table (fixed before the runs), for a log that matched A to C:
- no text k > 0                                    -> "not read (no text)"
- exact: shared = N = M, official-only = Reborn-only = 0, d = e = N
    - map is the identity                          -> "same order (contradicts experiment 35)"
    - otherwise                                    -> "order differs"
- normalised: shared = N = M, official-only = Reborn-only = 0
                                                   -> "encoding/case"
- otherwise                                        -> "contents differ"
For "order differs" the mapping pattern is printed as a finding: fixed
points, whether it is the reversal (j = N-1-i), whether it is a rotation
(j = i + r mod N, with r), and the number of cycles. These describe the
pairs; none of them is an order rule Reborn adopts (ADR 0061: a rule needs
a source or an observation, and a planned experiment).

"experiment 36: survey complete" when both known games have an accepted
log and both matched A to C; otherwise "experiment 36: MISMATCH (survey
incomplete)". The readings are printed per game, then "readings:" with
both.

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the two games, one log per game, and both
must be given. Freshness (LOOP.md step 4): a log whose start
(observelog.start_of) is earlier than PLAN_COMMITTED is rejected.
PLAN_COMMITTED is the committer time (local, as run.sh writes starts.log)
of the commit that holds the final plan text; it is set in a follow-up
commit. While it is None, every log is rejected.

Prints only counts, versions, the kinds of notes, readings and the index
pairs of the map (public schema positions, ADR 0061); never a name, a
digest, an address or a register value, nor the reason text of a note.

Usage: python tools/shim/experiments/036_achievement_list_survey.py <log> <log>
       python tools/shim/experiments/036_achievement_list_survey.py --self-check
(the second runs made-up logs through A to C and the reading table, no
file is read)
"""
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

V011 = "STEAMUSERSTATS_INTERFACE_VERSION011"
V012 = "STEAMUSERSTATS_INTERFACE_VERSION012"
POSITIONS = {
    (V011, 14): "011 #14", (V011, 15): "011 #15",
    (V012, 14): "012 #14", (V012, 15): "012 #15",
}
# app ID: (name, version it must call)
GAMES = {
    1275670: ("Outcore: Desktop Adventure", V011),
    3086720: ("One's Own Exile", V012),
}
PLAN_COMMITTED = None  # set in a follow-up commit to the plan commit's local committer time

LINE = re.compile(r"^(\d+) ms ThreadId\((\d+)\) (\S+?)#(\d+) (call|return) (\d+) (.*)$")
SET = re.compile(
    r"^shadow set: official (\d+) \(distinct (\d+), no text (\d+)\), Reborn (\d+) "
    r"\(distinct (\d+)\); shared (\d+), official-only (\d+), Reborn-only (\d+)$")
NORMALISED = re.compile(
    r"^shadow set normalised: shared (\d+), official-only (\d+), Reborn-only (\d+)$")
MAP = re.compile(r"^shadow map:((?: \d+:(?:\d+|none|notext))*)$")
SURVEY_KINDS = ("set", "normalised", "map")


def kind_of(note):
    """The kind of a per-call shadow note, without any reason text."""
    if note == "shadow match":
        return "match"
    if note == "shadow mismatch":
        return "mismatch"
    if note.startswith("shadow mismatch ("):
        return "mismatch with reason"
    if note.startswith("shadow: no Reborn answer"):
        return "no Reborn answer"
    if note.startswith("shadow: skipped"):
        return "skipped"
    if note == "shadow: no app ID given":
        return "no app ID given"
    if note == "shadow: no worker thread":
        return "no worker thread"
    return "other"


def survey_of(note):
    """(kind, parsed) for a survey line of ADR 0061, or None."""
    m = SET.match(note)
    if m:
        keys = ("n", "distinct", "no_text", "m", "reborn_distinct", "shared", "official_only",
                "reborn_only")
        return "set", dict(zip(keys, map(int, m.groups())))
    m = NORMALISED.match(note)
    if m:
        return "normalised", dict(zip(("shared", "official_only", "reborn_only"),
                                      map(int, m.groups())))
    m = MAP.match(note)
    if m:
        pairs = []
        for pair in m.group(1).split():
            i, j = pair.split(":")
            pairs.append((int(i), int(j) if j.isdigit() else j))
        return "map", pairs
    if note.startswith("shadow set") or note.startswith("shadow map"):
        return "unparsed", None
    return None


def verdict(ok):
    return "match" if ok else "MISMATCH"


def read(lines):
    """The calls of the four positions: key (interface, position, number) ->
    {"thread", "result", "value", "index", "notes": [(thread,
    after_result, kind)]}; the survey lines [(kind, parsed, key, thread,
    after_result)]; the per-call notes without a call; the STEAMUSERSTATS
    versions called."""
    calls, surveys, orphans, versions = {}, [], 0, set()
    for raw in lines:
        m = LINE.match(observelog.clean(raw))
        if not m:
            continue
        _, thread, interface, pos, what, number, rest = m.groups()
        if interface.startswith("STEAMUSERSTATS_INTERFACE_VERSION"):
            versions.add(interface)
        if (interface, int(pos)) not in POSITIONS:
            continue
        key = (interface, int(pos), int(number))
        if what == "call":
            calls[key] = {"thread": int(thread), "result": False, "value": None,
                          "index": observelog.low32(observelog.register("rdx", rest)),
                          "notes": []}
            continue
        call = calls.get(key)
        survey = survey_of(rest)
        if survey is not None:
            after = call is not None and call["result"] and call["thread"] == int(thread)
            surveys.append((survey[0], survey[1], key, int(thread), after))
        elif rest.startswith("shadow"):
            if call is None:
                orphans += 1
            else:
                call["notes"].append((int(thread), call["result"], kind_of(rest)))
        elif call is not None and rest.startswith("rax"):
            call["result"] = True
            call["value"] = observelog.low32(observelog.register("rax", rest))
    return calls, surveys, orphans, versions


def pattern(pairs, n):
    """The mapping pattern of a permutation, as a finding (descriptive)."""
    js = [j for _, j in pairs]
    fixed = sum(1 for i, j in pairs if i == j)
    reverse = all(j == n - 1 - i for i, j in pairs)
    rotations = [r for r in range(1, n) if all(j == (i + r) % n for i, j in pairs)]
    seen, cycles = set(), 0
    for start in range(n):
        if start in seen:
            continue
        cycles += 1
        at = start
        while at not in seen:
            seen.add(at)
            at = js[at]
    rotation = f"rotation by {rotations[0]}" if rotations else "no rotation"
    return (f"{fixed} fixed points of {n}, {'reversal' if reverse else 'not the reversal'}, "
            f"{rotation}, {cycles} cycles")


def reading(exact, normalised, pairs):
    """The fixed reading table of the plan; returns (reading, pattern or None)."""
    n = exact["n"]
    if exact["no_text"] > 0:
        return "not read (no text)", None
    all_shared = (exact["shared"] == n == exact["m"] and exact["official_only"] == 0
                  and exact["reborn_only"] == 0 and exact["distinct"] == n
                  and exact["reborn_distinct"] == n)
    if all_shared:
        if all(i == j for i, j in pairs):
            return "same order (contradicts experiment 35)", None
        return "order differs", pattern(pairs, n)
    if (normalised["shared"] == n == exact["m"] and normalised["official_only"] == 0
            and normalised["reborn_only"] == 0):
        return "encoding/case", None
    return "contents differ", None


def check(lines, label, version):
    """S, A to C and the reading for one log. Returns (ok, reading)."""
    calls, surveys, orphans, versions = read(lines)
    counts = Counter(POSITIONS[k[:2]] for k in calls)
    notes = Counter((POSITIONS[k[:2]], kind) for k, c in calls.items() for (_, _, kind) in c["notes"])
    shown = ", ".join(f"{pos} {kind}: {n}" for (pos, kind), n in sorted(notes.items()))
    print(f"{label} S: versions {sorted(v[-3:] for v in versions)}, calls {dict(sorted(counts.items()))}, "
          f"per-call notes [{shown}], {orphans} notes without a call")

    returned = {k: c for k, c in calls.items() if c["result"]}
    have = {k[:2] for k in returned}
    ok_a = (version, 14) in have and (version, 15) in have
    print(f"{label} A: calls with a result line "
          f"{dict(sorted(Counter(POSITIONS[k[:2]] for k in returned).items()))} -> {verdict(ok_a)}")

    by_kind = Counter(kind for (kind, _, _, _, _) in surveys)
    placed = all(after for (_, _, _, _, after) in surveys)
    one_each = all(by_kind[kind] == 1 for kind in SURVEY_KINDS) and by_kind["unparsed"] == 0
    found = {kind: parsed for (kind, parsed, _, _, _) in surveys}
    values = {c["value"] for k, c in returned.items() if k[1] == 14}
    official = values.pop() if len(values) == 1 else None
    ok_b = ok_a and one_each and placed
    n = None
    if ok_b:
        n = found["set"]["n"]
        ok_b = (official == n and [i for i, _ in found["map"]] == list(range(n)))
    print(f"{label} B: survey lines {dict(sorted(by_kind.items()))}, all after their call's result "
          f"on its thread: {placed}, official count from 14: "
          f"{official if official is not None else 'not one value'}, N in the set line: "
          f"{n if n is not None else '-'} -> {verdict(ok_b)}")

    ok_c = False
    if ok_b:
        per_index = {}
        for k, c in returned.items():
            if k[1] == 15:
                per_index.setdefault(c["index"], []).extend(kind for (_, _, kind) in c["notes"])
        mapping = dict(found["map"])
        checked = disagree = 0
        for index, kinds in per_index.items():
            if len(kinds) == 1 and kinds[0] in ("match", "mismatch") and index in mapping:
                checked += 1
                if (kinds[0] == "match") != (mapping[index] == index):
                    disagree += 1
        ok_c = checked > 0 and disagree == 0
        print(f"{label} C: {checked} indices with one per-call note checked against the map, "
              f"{disagree} disagree -> {verdict(ok_c)}")
    else:
        print(f"{label} C: not checked (B) -> MISMATCH")

    ok = ok_a and ok_b and ok_c
    if not ok:
        return False, None
    exact, normalised, pairs = found["set"], found["normalised"], found["map"]
    print(f"{label} set: official {exact['n']} (distinct {exact['distinct']}, no text "
          f"{exact['no_text']}), Reborn {exact['m']} (distinct {exact['reborn_distinct']}); shared "
          f"{exact['shared']}, official-only {exact['official_only']}, Reborn-only "
          f"{exact['reborn_only']}")
    print(f"{label} set normalised: shared {normalised['shared']}, official-only "
          f"{normalised['official_only']}, Reborn-only {normalised['reborn_only']}")
    print(f"{label} pairs (official i: Reborn j): {' '.join(f'{i}:{j}' for i, j in pairs)}")
    result, shape = reading(exact, normalised, pairs)
    print(f"{label} reading: {result}")
    if shape:
        print(f"{label} finding: mapping pattern: {shape}")
    return True, result


def self_check():
    """Made-up logs through check(); prints nothing about real logs."""
    def log(version, pairs, set_line, normalised_line, matches=None, drop=None):
        n = len(pairs)
        out = []
        number = 0

        def call(pos, index, rax, note):
            nonlocal number
            number += 1
            p = f"{number} ms ThreadId(1) {version}#{pos}"
            out.append(f"{p} call {number} rcx=0x1 rdx=0x{index:x} r8=0x0 r9=0x0")
            out.append(f"{p} return {number} rax=0x{rax:x}")
            if note:
                out.append(f"{p} return {number} {note}")
            return p, number

        call(14, 0, n, "shadow match")
        last = None
        for i, j in pairs:
            same = (j == i) if matches is None else matches[i]
            last = call(15, i, 0x1000, "shadow match" if same else "shadow mismatch")
        prefix = f"{last[0]} return {last[1]}"
        survey = {
            "set": f"shadow set: {set_line}",
            "normalised": f"shadow set normalised: {normalised_line}",
            "map": "shadow map: " + " ".join(f"{i}:{j}" for i, j in pairs),
        }
        for kind in SURVEY_KINDS:
            if kind != drop:
                out.append(f"{prefix} {survey[kind]}")
        return out

    full = "official 4 (distinct 4, no text 0), Reborn 4 (distinct 4); shared 4, official-only 0, Reborn-only 0"
    norm_full = "shared 4, official-only 0, Reborn-only 0"
    rotated = [(0, 1), (1, 2), (2, 3), (3, 0)]
    cases = [
        ("order differs", log(V011, rotated, full, norm_full), V011, (True, "order differs")),
        ("same order", log(V011, [(i, i) for i in range(4)], full, norm_full), V011,
         (True, "same order (contradicts experiment 35)")),
        ("encoding/case",
         log(V012, [(0, 0), (1, "none"), (2, "none"), (3, 3)],
             "official 4 (distinct 4, no text 0), Reborn 4 (distinct 4); shared 2, official-only 2, Reborn-only 2",
             norm_full), V012, (True, "encoding/case")),
        ("contents differ",
         log(V011, [(0, 0), (1, "none"), (2, 2), (3, 3)],
             "official 4 (distinct 4, no text 0), Reborn 4 (distinct 4); shared 3, official-only 1, Reborn-only 1",
             "shared 3, official-only 1, Reborn-only 1"), V011, (True, "contents differ")),
        ("no text",
         log(V011, [(0, "notext"), (1, 1), (2, 2), (3, 3)],
             "official 4 (distinct 3, no text 1), Reborn 4 (distinct 4); shared 3, official-only 0, Reborn-only 1",
             "shared 3, official-only 0, Reborn-only 1"), V011, (True, "not read (no text)")),
        ("no map line", log(V011, rotated, full, norm_full, drop="map"), V011, (False, None)),
        ("map disagrees with the per-call notes",
         log(V011, rotated, full, norm_full, matches=[True, False, False, False]), V011, (False, None)),
        ("wrong version", log(V012, rotated, full, norm_full), V011, (False, None)),
        ("N differs from the count",
         log(V011, rotated,
             "official 5 (distinct 5, no text 0), Reborn 5 (distinct 5); shared 5, official-only 0, Reborn-only 0",
             "shared 5, official-only 0, Reborn-only 0"), V011, (False, None)),
    ]
    failed = 0
    for name, lines, version, expected in cases:
        got = check(lines, f"self-check '{name}'", version)
        if got != expected:
            failed += 1
            print(f"self-check '{name}': got {got}, expected {expected} -> MISMATCH")
    reverse = pattern([(0, 3), (1, 2), (2, 1), (3, 0)], 4)
    if reverse != "0 fixed points of 4, reversal, no rotation, 2 cycles":
        failed += 1
        print(f"self-check 'reversal pattern': got {reverse} -> MISMATCH")
    print("self-check:", "match" if failed == 0 else f"MISMATCH ({failed} cases)")
    return failed == 0


def main(paths):
    ok, seen, readings = True, set(), {}
    if PLAN_COMMITTED is None:
        print("plan time not set yet (PLAN_COMMITTED) -> MISMATCH")
        ok = False
    if len({Path(p).resolve() for p in paths}) != len(paths):
        print("a log was given twice -> MISMATCH")
        ok = False
    for path in paths:
        name, app = Path(path).name, observelog.app_of(path)
        if app not in GAMES or app in seen:
            print(f"{name}: game {app} not expected or given twice -> MISMATCH")
            ok = False
            continue
        game, version = GAMES[app]
        started = observelog.start_of(path)
        if PLAN_COMMITTED is None or started is None or started < PLAN_COMMITTED:
            print(f"{name} ({game}): started {started}, before the plan commit {PLAN_COMMITTED} -> MISMATCH")
            ok = False
            continue
        seen.add(app)
        label = f"{name} ({game})"
        with open(path, encoding="utf-8", errors="replace") as lines:
            good, result = check(lines, label, version)
        print(f"{label}: {verdict(good)}")
        ok = ok and good
        readings[game] = result or "-"
    if set(GAMES) - seen:
        print("a known game has no accepted log -> MISMATCH")
        ok = False
    print("experiment 36:", "survey complete" if ok else "MISMATCH (survey incomplete)")
    print("readings:", ", ".join(f"{game}: {r}" for game, r in readings.items()) or "none")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        sys.exit(0 if self_check() else 1)
    main(sys.argv[1:])
