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

  "<ms> ms ThreadId(<t>) <Interface>#<pos> call <n> ... rdx=0x... ..."
  "<ms> ms ThreadId(<t>) <Interface>#<pos> return <n> rax=0x..."  (result)
  "<ms> ms ThreadId(<t>) <Interface>#<pos> return <n> <note>"     (shadow,
       later, from the shadow worker once Reborn's list is there, or at
       once for a skipped one)

where <note> is "shadow match", "shadow mismatch", "shadow mismatch
(<why>)" (position 15 only: no text at the result), "shadow: no Reborn
answer (<why>)", "shadow: skipped, <why>" (for example "worker busy"),
"shadow: no app ID given" or "shadow: no worker thread".

Predictions, per log (the positions are 011 14/15 and 012 14/15):
S  (survey, no verdict) the STEAMUSERSTATS versions the game called, and
   how many calls of each of the four positions.
A  At least one call of 14 and one of 15 on the same version, each with a
   result line (known games: on the expected version). A candidate
   without that is "not a third game": with no call of the four positions
   it gets no verdict; with some calls (for example 14 only, or 14 and 15
   on different versions) B and C are still checked, as findings.
B  Every call of the four positions has a result line and exactly one
   shadow line with the same position, call number and thread, after the
   result line; no shadow line is without such a call.
C  Every such shadow line says exactly "shadow match".
K  (coverage, no verdict) "distinct indices compared k of N": k distinct
   indices (low 32 bits of rdx at the call of 15) whose call got "shadow
   match" or "shadow mismatch", N the official count (low 32 bits of the
   result of 14; "N not one value" if the calls of 14 disagree). A match
   covers only these k indices.

Findings printed when a note is a mismatch (counts only): the number of
count mismatches (14) and of name mismatches (15), with "the official name
at i differs from Reborn's i-th" for the second.

"experiment 35: match" when both known games match and every candidate
that called the positions matches. "ADR 0059 decision 3: reached" when,
in addition, at least one candidate matched A, B and C.

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
       python tools/shim/experiments/035_achievement_name_shadow.py --self-check
(the second runs made-up logs through A to C and K, no file is read)
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


def read(lines):
    """The calls of the four positions: key (interface, position, number)
    -> {"thread", "result", "value", "index", "shadows": [(thread,
    after_result, kind)]}; the number of shadow lines without a call; and
    the STEAMUSERSTATS versions the log has calls of. "index" is the low
    32 bits of rdx at a call of 15, "value" the low 32 bits of the result
    (used only for 14)."""
    calls, orphans, versions = {}, 0, set()
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
                          "shadows": []}
            continue
        call = calls.get(key)
        if rest.startswith("shadow"):
            if call is None:
                orphans += 1
            else:
                call["shadows"].append((int(thread), call["result"], kind_of(rest)))
        elif call is not None and rest.startswith("rax"):
            call["result"] = True
            call["value"] = observelog.low32(observelog.register("rax", rest))
    return calls, orphans, versions


def both_on_one_version(returned, version):
    """Whether 14 and 15 both have a call with a result line on `version`,
    or (version None) on one of 011 and 012."""
    have = {k[:2] for k in returned}
    versions = [version] if version else [V011, V012]
    return any((v, 14) in have and (v, 15) in have for v in versions)


def coverage(calls):
    """K: distinct compared indices of 15 and the official count of 14."""
    compared = {c["index"] for k, c in calls.items()
                if k[1] == 15 and any(kind in ("match", "mismatch") for (_, _, kind) in c["shadows"])}
    counts = {c["value"] for k, c in calls.items() if k[1] == 14 and c["result"]}
    if len(counts) == 1:
        return f"distinct indices compared {len(compared)} of {counts.pop()}"
    if not counts:
        return f"distinct indices compared {len(compared)} of N (no call of 14)"
    return f"distinct indices compared {len(compared)} of N (N not one value: {len(counts)} values)"


def check(lines, label, version):
    """S, A to C and K for one log. Returns (verdicts, third) or None for a
    candidate without a call of the four positions; verdicts are the ones
    that count for the experiment, third whether it can be the third game."""
    calls, orphans, versions = read(lines)
    counts = Counter(POSITIONS[k[:2]] for k in calls)
    print(f"{label} S: versions {sorted(v[-3:] for v in versions)}, calls {dict(sorted(counts.items()))}")
    returned = {k: c for k, c in calls.items() if c["result"]}
    if version is None and not calls:
        print(f"{label} A: no call of 011/012 14 or 15 -> not a third game")
        return None

    ok_a = both_on_one_version(returned, version)
    have = Counter(POSITIONS[k[:2]] for k in returned)
    shown = dict(sorted(have.items()))
    if version is None:
        note = "candidate checked" if ok_a else "not a third game (not 14 and 15 on one version)"
        print(f"{label} A: calls with a result line {shown} -> {note}")
    else:
        print(f"{label} A: calls with a result line {shown} -> {verdict(ok_a)}")

    unreturned = len(calls) - len(returned)
    good = sum(
        1 for c in returned.values()
        if len(c["shadows"]) == 1 and c["shadows"][0][0] == c["thread"] and c["shadows"][0][1]
    )
    ok_b = bool(returned) and good == len(returned) and orphans == 0 and unreturned == 0
    print(f"{label} B: {good} of {len(returned)} calls with exactly one shadow line after the result "
          f"on the same thread, {orphans} shadow lines without a call, "
          f"{unreturned} calls without a result line -> {verdict(ok_b)}")

    kinds = Counter(
        (POSITIONS[k[:2]], kind) for k, c in calls.items() for (_, _, kind) in c["shadows"]
    )
    ok_c = ok_b and {kind for (_, kind) in kinds} == {"match"}
    shown = ", ".join(f"{pos} {kind}: {n}" for (pos, kind), n in sorted(kinds.items()))
    print(f"{label} C: shadow notes [{shown}] -> {verdict(ok_c)}")
    print(f"{label} K: {coverage(calls)}")
    if any(kind.startswith("mismatch") for (_, kind) in kinds):
        count_off = sum(n for (pos, kind), n in kinds.items()
                        if pos.endswith("#14") and kind == "mismatch")
        names_off = sum(n for (pos, kind), n in kinds.items()
                        if pos.endswith("#15") and kind == "mismatch")
        no_text = sum(n for (pos, kind), n in kinds.items()
                      if pos.endswith("#15") and kind == "mismatch with reason")
        print(f"{label} finding: {count_off} count mismatches (14); {names_off} calls where the "
              f"official name at i differs from Reborn's i-th (15); {no_text} with no official text")
    if version is None:
        # A candidate's A only decides whether it is a third game; its B
        # and C count for the experiment either way.
        return [ok_b, ok_c], ok_a and ok_b and ok_c
    return [ok_a, ok_b, ok_c], False


def self_check():
    """Made-up logs through check(); prints nothing about real logs."""
    def log(entries):
        out = []
        for n, (version, pos, index, note) in enumerate(entries, 1):
            p = f"{n} ms ThreadId(1) {version}#{pos}"
            out.append(f"{p} call {n} rcx=0x1 rdx=0x{index:x} r8=0x0 r9=0x0")
            out.append(f"{p} return {n} rax=0x{3 if pos == 14 else 0x1000:x}")
            if note:
                out.append(f"{p} return {n} {note}")
        return out
    ok = "shadow match"
    cases = [
        ("all match", log([(V011, 14, 0, ok)] + [(V011, 15, i, ok) for i in range(3)]),
         V011, ([True, True, True], False)),
        ("candidate, all match", log([(V012, 14, 0, ok)] + [(V012, 15, i, ok) for i in range(3)]),
         None, ([True, True], True)),
        ("candidate, 14 only", log([(V011, 14, 0, ok)]), None, ([True, True], False)),
        ("candidate, 14 and 15 on different versions",
         log([(V011, 14, 0, ok), (V012, 15, 0, ok)]), None, ([True, True], False)),
        ("candidate, no call", ["0 ms ThreadId(1) STEAMUSERSTATS_INTERFACE_VERSION013#14 call 1 rdx=0x0"],
         None, None),
        ("one name mismatch", log([(V011, 14, 0, ok), (V011, 15, 0, ok), (V011, 15, 1, "shadow mismatch")]),
         V011, ([True, True, False], False)),
        ("worker busy", log([(V011, 14, 0, ok), (V011, 15, 0, "shadow: skipped, worker busy")]),
         V011, ([True, True, False], False)),
        ("no shadow line", log([(V011, 14, 0, ok), (V011, 15, 0, None)]), V011, ([True, False, False], False)),
        ("known game on the wrong version", log([(V012, 14, 0, ok), (V012, 15, 0, ok)]), V011,
         ([False, True, True], False)),
    ]
    failed = 0
    for name, lines, version, expected in cases:
        got = check(lines, f"self-check '{name}'", version)
        if got != expected:
            failed += 1
            print(f"self-check '{name}': got {got}, expected {expected} -> MISMATCH")
    print("self-check:", "match" if failed == 0 else f"MISMATCH ({failed} cases)")
    return failed == 0


def main(paths):
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
        with open(path, encoding="utf-8", errors="replace") as lines:
            result = check(lines, label, version)
        if result is None:
            continue
        counted, is_third = result
        print(f"{label}: {verdict(all(counted))}")
        verdicts.extend(counted)
        if role == CANDIDATE:
            third.append(is_third)

    known = {a for a, (_, role, _) in GAMES.items() if role == KNOWN}
    if not known <= seen:
        print("a known game has no accepted log -> MISMATCH")
        verdicts.append(False)
    ok = all(verdicts)
    print("experiment 35:", verdict(ok))
    print("ADR 0059 decision 3:",
          "reached" if ok and any(third) else
          f"not reached ({sum(third)} of {len(third)} checked candidates are a third game)")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-check"]:
        sys.exit(0 if self_check() else 1)
    main(sys.argv[1:])
