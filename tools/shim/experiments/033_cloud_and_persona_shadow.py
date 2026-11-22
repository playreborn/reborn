"""Experiment 33 (docs/shim/meanings.md, "Experiment 33", loop iteration
16b): the first live runs of two merged shadows.

- Cloud reads (PR #47, ADR 0055 stage 1):
  STEAMREMOTESTORAGE_INTERFACE_VERSION014 position 15 (GetFileSize) and 1
  (FileRead), both confirmed in experiment 1, compared with rebornd's
  shadow folder that `reborn-observe arm --app` has rebornd fill. Game:
  Cave Crawlers (2143030), the only recorded game that calls them.
- Persona name (PR #45, ADR 0056): SteamFriends017 position 0, narrowed
  to GetPersonaName or ClearRichPresence, compared with the account's
  persona name by digest. Games: Dark Deception (332950) and A Park Full
  of Cats (2610090).

What the proxy writes (crates/shim-observe/src/proxy.rs, the step-4 return
line, then the ADR 0046 block; answer_asks; daemon.rs, compare_file_size,
compare_file_read, compare_persona_name), for a logged call <n> of one of
these positions on thread <t>:

  "<ms> ms ThreadId(<t>) <Interface>#<pos> call <n> ..."
  "<ms> ms ThreadId(<t>) <Interface>#<pos> return <n> rax..."   (result line;
       "rax hidden (account data)" for the cloud reads, "rax=0x..." for 0)
  "<ms> ms ThreadId(<t>) <Interface>#<pos> return <n> shadow..." (shadow line,
       later, from the shadow worker, or at once for "shadow: skipped")

where the shadow note is one of "shadow match", "shadow mismatch",
"shadow mismatch (<why>)" (persona name only, for example no text at the
result), "shadow: no Reborn answer (<why>)", "shadow: skipped, <why>",
"shadow: no app ID given" or "shadow: no worker thread".

Predictions, per game (A only for the positions the game is expected to
call; B and C for every call of the three positions in the log):
A  The log has at least one call of each expected position with a result
   line (Cave Crawlers: 014 #15 and 014 #1; Dark Deception and A Park Full
   of Cats: 017 #0).
B  Every call with a result line has exactly one shadow line with the same
   position, call number and thread, after its result line; no shadow line
   is without such a call; every call line has a result line.
C  Every shadow line says exactly "shadow match".

Experiment 33 matches when all three games match. The two parts are also
reported on their own ("cloud part", "persona part"), because they
confirm different things (meanings.md).

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the three games, one log per game.
Freshness (LOOP.md step 4): a log whose start (observelog.start_of) is
earlier than PLAN_COMMITTED is rejected. PLAN_COMMITTED is the committer
time (local, as run.sh writes starts.log) of the commit on loop/plan-16b
that holds the final plan text; it is set in a follow-up commit. While it
is None, every log is rejected.

Never prints file names, sizes, digests, addresses, register values or the
reason text of a note; only counts, the kind of each note and
match/MISMATCH.

Usage: python tools/shim/experiments/033_cloud_and_persona_shadow.py <log> <log> <log>
"""
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

REMOTE = "STEAMREMOTESTORAGE_INTERFACE_VERSION014"
FRIENDS = "SteamFriends017"
POSITIONS = {(REMOTE, 15): "014 #15", (REMOTE, 1): "014 #1", (FRIENDS, 0): "017 #0"}
CLOUD, PERSONA = "cloud", "persona"
# app ID: (name, part, positions it must call)
GAMES = {
    2143030: ("Cave Crawlers", CLOUD, [(REMOTE, 15), (REMOTE, 1)]),
    332950: ("Dark Deception", PERSONA, [(FRIENDS, 0)]),
    2610090: ("A Park Full of Cats", PERSONA, [(FRIENDS, 0)]),
}
PLAN_COMMITTED = datetime(2026, 11, 22, 21, 38, 54)  # commit f2594f0 on loop/plan-16b (final plan)

LINE = re.compile(r"^(\d+) ms ThreadId\((\d+)\) (\S+?)#(\d+) (call|return) (\d+) (.*)$")


def kind_of(note):
    """The kind of a shadow note, without any reason text (it may name
    what rebornd said, which is not printed)."""
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
    return "other"


def verdict(ok):
    return "match" if ok else "MISMATCH"


def read(path):
    """The calls of the three positions: key (interface, position, number)
    -> {"thread", "result", "shadows": [(thread, after_result, kind)]};
    and the number of shadow lines without a call."""
    calls, orphans = {}, 0
    for raw in open(path, encoding="utf-8", errors="replace"):
        m = LINE.match(observelog.clean(raw))
        if not m:
            continue
        _, thread, interface, pos, what, number, rest = m.groups()
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
    return calls, orphans


def check(path, label, expected):
    """Predictions A to C for one log; prints them, returns all verdicts."""
    calls, orphans = read(path)
    returned = {k: c for k, c in calls.items() if c["result"]}

    counts = {POSITIONS[p]: sum(1 for k in returned if k[:2] == p) for p in expected}
    ok_a = all(n >= 1 for n in counts.values())
    print(f"{label} A: calls with a result line {counts} -> {verdict(ok_a)}")

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
    return [ok_a, ok_b, ok_c]


paths = sys.argv[1:]
parts = {CLOUD: [], PERSONA: []}
verdicts, seen = [], set()
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
    game, part, expected = GAMES[app]
    started = observelog.start_of(path)
    if PLAN_COMMITTED is None or started is None or started < PLAN_COMMITTED:
        print(f"{name} ({game}): started {started}, before the plan commit {PLAN_COMMITTED} -> MISMATCH")
        verdicts.append(False)
        continue
    seen.add(app)
    label = f"{name} ({game})"
    result = check(path, label, expected)
    print(f"{label}: {verdict(all(result))}")
    parts[part].extend(result)
    verdicts.extend(result)

# A rejected log (given twice, unexpected, too early) fails both parts.
clean_run = len(verdicts) == sum(len(v) for v in parts.values())
for part in (CLOUD, PERSONA):
    apps = {a for a, (_, p, _) in GAMES.items() if p == part}
    ok = clean_run and apps <= seen and bool(parts[part]) and all(parts[part])
    print(f"{part} part:", verdict(ok))
print("experiment 33:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
