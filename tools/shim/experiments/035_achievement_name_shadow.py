"""Experiment 35 (docs/shim/meanings.md, "Experiment 35", loop iteration
17b): the first live runs of the achievement-name shadow (PR #56, ADR 0059
decision 2).

STEAMUSERSTATS_INTERFACE_VERSION011 and 012 position 14
(GetNumAchievements, confirmed under A1 and A3) and position 15
(GetAchievementName, confirmed), experiment 32. The proxy compares the
official count with the length of Reborn's achievement list from
rebornd's StatsReceived, and the official text at index i with Reborn's
i-th API name, by SHA-256 (crates/shim-observe/src/daemon.rs,
compare_achievement; proxy.rs, achievement_shadow, answer_asks).

Games:
- known: Outcore: Desktop Adventure (1275670, 011) and One's Own Exile
  (3086720, 012), the two recorded games that call these positions;
- candidates for ADR 0059 decision 3 (a third game): Nonogram - Master's
  Legacy (742500), Zaccaria Pinball (444930) and vivid/stasis (2093940),
  never recorded; whether they call 011/012 14 or 15 shows only in the
  log.

What the proxy writes, for a logged call <n> of one of these positions on
thread <t>:

  "<ms> ms ThreadId(<t>) <Interface>#<pos> call <n> ..."
  "<ms> ms ThreadId(<t>) <Interface>#<pos> return <n> rax=0x..."  (result)
  "<ms> ms ThreadId(<t>) <Interface>#<pos> return <n> <note>"     (shadow,
       later, from the shadow worker once Reborn's list is there, or at
       once for a skipped one)

where <note> is "shadow match", "shadow mismatch", "shadow mismatch
(<why>)" (position 15 only: no text at the result), "shadow: no Reborn
answer (<why>)", "shadow: skipped, <why>", "shadow: no app ID given" or
"shadow: no worker thread".

Predictions, per log (the positions are 011 14/15 and 012 14/15):
S  (survey, no verdict) the STEAMUSERSTATS versions the game called, and
   how many calls of each of the four positions.
A  Known games: at least one call of 14 and one of 15 on the expected
   version, each with a result line. Candidates: a candidate with no call
   of the four positions is "not a third game" (no verdict); one with a
   call is checked by B and C like a known game.
B  Every call of the four positions has a result line and exactly one
   shadow line with the same position, call number and thread, after the
   result line; no shadow line is without such a call.
C  Every such shadow line says exactly "shadow match".

Findings printed on a mismatch of C (counts only): whether the count (14)
mismatched, and how many calls of 15 mismatched. A count that matches
with names that do not is the index-mapping finding ADR 0059 asks about.

"experiment 35: match" when both known games match and every candidate
that called the positions matches. "ADR 0059 decision 3: reached" when,
in addition, at least one candidate called them and matched.

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the five games, one log per game; the two
known games must both be given. Freshness (LOOP.md step 4): a log whose
start (observelog.start_of) is earlier than PLAN_COMMITTED is rejected.
PLAN_COMMITTED is the committer time (local, as run.sh writes
starts.log) of the commit that holds the final plan text; it is set in a
follow-up commit. While it is None, every log is rejected.

Never prints file names other than the log's, names, digests, addresses
or register values, nor the reason text of a note; only counts, versions,
the kind of each note and match/MISMATCH.

Usage: python tools/shim/experiments/035_achievement_name_shadow.py <log> <log> ...
"""
import re
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

V011 = "STEAMUSERSTATS_INTERFACE_VERSION011"
V012 = "STEAMUSERSTATS_INTERFACE_VERSION012"
POSITIONS = {
    (V011, 14): "011 #14", (V011, 15): "011 #15",
    (V012, 14): "012 #14", (V012, 15): "012 #15",
}
KNOWN, CANDIDATE = "known", "candidate"
# app ID: (name, role, version it must call; None for candidates)
GAMES = {
    1275670: ("Outcore: Desktop Adventure", KNOWN, V011),
    3086720: ("One's Own Exile", KNOWN, V012),
    742500: ("Nonogram - Master's Legacy", CANDIDATE, None),
    444930: ("Zaccaria Pinball", CANDIDATE, None),
    2093940: ("vivid/stasis", CANDIDATE, None),
}
PLAN_COMMITTED = None  # set in a follow-up commit to the final plan commit's time

LINE = re.compile(r"^(\d+) ms ThreadId\((\d+)\) (\S+?)#(\d+) (call|return) (\d+) (.*)$")


def kind_of(note):
    """The kind of a shadow note, without any reason text."""
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


def verdict(ok):
    return "match" if ok else "MISMATCH"


def read(path):
    """The calls of the four positions: key (interface, position, number)
    -> {"thread", "result", "shadows": [(thread, after_result, kind)]};
    the number of shadow lines without a call; and the STEAMUSERSTATS
    versions the log has calls of."""
    calls, orphans, versions = {}, 0, set()
    for raw in open(path, encoding="utf-8", errors="replace"):
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
            calls[key] = {"thread": int(thread), "result": False, "shadows": []}
            continue
        call = calls.get(key)
        if rest.startswith("shadow"):
            if call is None:
                orphans += 1
            else:
                call["shadows"].append((int(thread), call["result"], kind_of(rest)))
        elif call is not None and rest.startswith("rax"):
            call["result"] = True
    return calls, orphans, versions


def check(path, label, version):
    """S and A to C for one log. Returns None for a candidate without a
    call of the four positions, otherwise the list of verdicts."""
    calls, orphans, versions = read(path)
    counts = Counter(POSITIONS[k[:2]] for k in calls)
    print(f"{label} S: versions {sorted(v[-3:] for v in versions)}, calls {dict(sorted(counts.items()))}")
    returned = {k: c for k, c in calls.items() if c["result"]}

    if version is None:
        if not calls:
            print(f"{label} A: no call of 011/012 14 or 15 -> not a third game")
            return None
        ok_a = True
        print(f"{label} A: calls 011/012 14 or 15 -> candidate checked")
    else:
        need = {POSITIONS[(version, 14)], POSITIONS[(version, 15)]}
        have = Counter(POSITIONS[k[:2]] for k in returned)
        ok_a = all(have[p] >= 1 for p in need)
        shown = {p: have[p] for p in sorted(need)}
        print(f"{label} A: calls with a result line {shown} -> {verdict(ok_a)}")

    unreturned = len(calls) - len(returned)
    good = sum(
        1 for c in returned.values()
        if len(c["shadows"]) == 1 and c["shadows"][0][0] == c["thread"] and c["shadows"][0][1]
    )
    ok_b = ok_a and good == len(returned) and orphans == 0 and unreturned == 0
    print(f"{label} B: {good} of {len(returned)} calls with exactly one shadow line after the result "
          f"on the same thread, {orphans} shadow lines without a call, "
          f"{unreturned} calls without a result line -> {verdict(ok_b)}")

    kinds = Counter(
        (POSITIONS[k[:2]], kind) for k, c in calls.items() for (_, _, kind) in c["shadows"]
    )
    ok_c = ok_b and {kind for (_, kind) in kinds} == {"match"}
    shown = ", ".join(f"{pos} {kind}: {n}" for (pos, kind), n in sorted(kinds.items()))
    print(f"{label} C: shadow notes [{shown}] -> {verdict(ok_c)}")
    if any(kind.startswith("mismatch") for (_, kind) in kinds):
        count_off = sum(n for (pos, kind), n in kinds.items()
                        if pos.endswith("#14") and kind == "mismatch")
        names_off = sum(n for (pos, kind), n in kinds.items()
                        if pos.endswith("#15") and kind.startswith("mismatch"))
        print(f"{label} finding: {count_off} count mismatches (14), {names_off} name mismatches (15)")
    return [ok_a, ok_b, ok_c]


paths = sys.argv[1:]
verdicts, seen, third = [], set(), []
if PLAN_COMMITTED is None:
    print("plan time not set yet (PLAN_COMMITTED) -> MISMATCH")
    verdicts.append(False)
if len({Path(p).resolve() for p in paths}) != len(paths):
    print("a log was given twice -> MISMATCH")
    verdicts.append(False)
for path in paths:
    name, app = Path(path).name, observelog.app_of(path)
    if app not in GAMES or app in seen:
        print(f"{name}: game {app} not expected or given twice -> MISMATCH")
        verdicts.append(False)
        continue
    game, role, version = GAMES[app]
    started = observelog.start_of(path)
    if PLAN_COMMITTED is None or started is None or started < PLAN_COMMITTED:
        print(f"{name} ({game}): started {started}, before the plan commit {PLAN_COMMITTED} -> MISMATCH")
        verdicts.append(False)
        continue
    seen.add(app)
    label = f"{name} ({game})"
    result = check(path, label, version)
    if result is None:
        continue
    print(f"{label}: {verdict(all(result))}")
    verdicts.extend(result)
    if role == CANDIDATE:
        third.append(all(result))

known = {a for a, (_, role, _) in GAMES.items() if role == KNOWN}
if not known <= seen:
    print("a known game has no accepted log -> MISMATCH")
    verdicts.append(False)
ok = all(verdicts)
print("experiment 35:", verdict(ok))
print("ADR 0059 decision 3:",
      "reached" if ok and any(third) else
      f"not reached ({len(third)} candidates called the positions)")
