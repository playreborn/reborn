"""Experiment 30 (docs/shim/meanings.md, "Callback 0x44d: stats
received", loop iteration 14a): the first live run of the 0x44d stats
shadow (ADR 0055 stage 1) in Cave Crawlers (2143030), Gravitas (1067310)
and Dark Deception (332950), the three STEAMUSERSTATS_INTERFACE_VERSION011
games that got exactly one 0x44d per run in experiments 20 and 21.

What the proxy writes (crates/shim-observe/src/proxy.rs, observe_return
and answer_asks; daemon.rs, compare_stats_received):

- step 4b, for every Steam_BGetCallback that returned 1:
  "<ms> ms ThreadId(<t>) Steam_BGetCallback return <n> wrote rdx->[...] ..."
  whose header (bytes 4..8) names the callback number;
- for a 0x44d, later and from the shadow worker thread:
  "<ms> ms ThreadId(<t>) Steam_BGetCallback return <n> callback 0x44d <note>"
  with the same <ms>, <t> and <n> as the step-4b line, and <note> one of
  "shadow match", "shadow mismatch", "shadow: no Reborn answer (<why>)",
  "shadow: no app ID given", "shadow: skipped, <why>" or
  "shadow: no worker thread".

Predictions, per game:
A  The log has at least one step-4b line for callback 0x44d.
B  Every such line is followed, later in the log, by exactly one stats
   shadow line with the same thread and call number, and there is no
   stats shadow line without a step-4b 0x44d line before it.
C  Every stats shadow line says exactly "shadow match" (no mismatch, no
   "no Reborn answer", no skipped or other note).

All three games must match ("experiment 30: match"). A run in which the
game exits before the shadow line is written gives a MISMATCH for B.

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the three games, one log per game.
Freshness (LOOP.md step 4): a log whose start (observelog.start_of) is
earlier than PLAN_COMMITTED is rejected. PLAN_COMMITTED is the committer
time (local, as run.sh writes starts.log) of the commit on loop/plan-14a
that holds the final plan text; it is set in a follow-up commit.

Never prints payload bytes, IDs, register values or the reason text of a
"no Reborn answer" note; only counts, the kind of note and match/MISMATCH.

Usage: python tools/shim/experiments/030_stats_shadow.py <log> <log> <log>
"""
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {2143030: "Cave Crawlers", 1067310: "Gravitas", 332950: "Dark Deception"}
STATS_RECEIVED = 0x44D
NAME = "Steam_BGetCallback"
SHADOW = re.compile(r"^callback 0x44d (shadow.*)$")
PLAN_COMMITTED = datetime(2026, 11, 22, 0, 0, 0)  # placeholder, set in the follow-up commit


def kind_of(note):
    """The kind of a shadow note, without any reason text (it may name
    what rebornd said, which is not printed)."""
    if note == "shadow match":
        return "match"
    if note == "shadow mismatch":
        return "mismatch"
    if note.startswith("shadow: no Reborn answer"):
        return "no Reborn answer"
    if note.startswith("shadow: skipped"):
        return "skipped"
    return "other"


def verdict(ok):
    return "match" if ok else "MISMATCH"


def check(path, label):
    """Predictions A to C for one log; prints them, returns all verdicts."""
    wrote, shadows, orphans = [], {}, 0
    for e in observelog.exports(path):
        if e.name != NAME:
            continue
        if e.kind == "wrote":
            h = observelog.header(e)
            if h is not None and h.u32(4) == STATS_RECEIVED:
                wrote.append((e.thread, e.number))
                shadows[(e.thread, e.number)] = []
        elif e.kind == "return":
            m = SHADOW.match(e.rest)
            if not m:
                continue
            key = (e.thread, e.number)
            if key in shadows:
                shadows[key].append(kind_of(m.group(1)))
            else:
                orphans += 1

    ok_a = len(wrote) >= 1
    print(f"{label} A: {len(wrote)} step-4b lines for callback 0x44d -> {verdict(ok_a)}")

    counts = Counter(len(notes) for notes in shadows.values())
    ok_b = ok_a and set(counts) == {1} and orphans == 0
    shown = ", ".join(f"{k} shadow line(s): {v}" for k, v in sorted(counts.items()))
    print(f"{label} B: per 0x44d [{shown}], {orphans} shadow lines without a 0x44d -> {verdict(ok_b)}")

    kinds = Counter(kind for notes in shadows.values() for kind in notes)
    ok_c = ok_b and set(kinds) == {"match"}
    print(f"{label} C: shadow notes {dict(sorted(kinds.items()))} -> {verdict(ok_c)}")
    return [ok_a, ok_b, ok_c]


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
    label = f"{name} ({GAMES[app]})"
    game = check(path, label)
    print(f"{label}: {verdict(all(game))}")
    verdicts.extend(game)
print("experiment 30:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
