"""Experiment 24 (docs/shim/meanings.md, loop iteration 11a): every recorded
position of SteamClient018 (Gravitas) and SteamClient019 (Dark Deception),
the two games that open a second pipe. Candidates from
https://partner.steamgames.com/doc/api/ISteamClient; the account type number
from https://partner.steamgames.com/doc/api/steam_api (EAccountType).

Handles are compared on their low 32 bits (HSteamPipe and HSteamUser are
int32 on the page). Names used below:
  P  the result of position 0 (CreateSteamPipe);
  U  the result of position 2 (ConnectToGlobalUser);
  V  the result of position 3 (CreateLocalUser);
  Q  the pipe handle of the first getter call after position 3 returned:
     rdx of position 9, r8 of positions 5, 6, 12, 26 and 38. CreateLocalUser
     writes the new pipe through its first argument; the proxy does not read
     that memory, so Q is taken from its first use.
Getter positions and the text they take (r9; r8 at position 9):
  5 SteamUser, 6 SteamGameServer (not ...Stats), 9 SteamUtils,
  12 any text, 26 SteamController, 38 SteamInput.

Predictions, per game:
A  Start: the first SteamClient call is position 0, the next position 2 with
   rdx = P.
B  Position 3 (CreateLocalUser(&pipe, eAccountType)) is called exactly once,
   with r8 = 3 (k_EAccountTypeGameServer), rdx at least 0x10000 (an
   address, not a handle) and a result V with 0 < V < 0x10000; position 7
   (SetLocalIPBinding, the page: "must be set before calling
   CreateLocalUser") is called exactly once, before position 3, with r8 at
   most 0xffff (a uint16 port).
C  Q exists, Q is not P and not 0, Q is the result of no SteamClient call,
   and no SteamClient call before position 3 returned has Q in the low 32
   bits of rdx or r8.
D  Every getter call passes a pipe in {P, Q}; with pipe P its user slot
   (rdx; none at position 9) is U or 0, with pipe Q it is V or 0.
E  Every getter call passes a text of its position's kind, and its result
   has the step 3a shape and is replaced by a stand-in "for" that same text.
   The same (text, pipe) always gives the same stand-in; for every text asked
   over both pipes the two stand-ins differ, and at least one text is asked
   over both pipes.
F  Close: position 4 (ReleaseUser(pipe, user)) is called exactly twice, once
   with (rdx, r8) = (P, U) and once with (Q, V); each is directly followed
   (next SteamClient call) by position 1 (BReleaseSteamPipe) with rdx = the
   same pipe and a lowest result byte 1, and that directly by position 23
   (BShutdownIfAllPipesClosed), whose lowest result byte is 1 if both P and
   Q have been released by then and 0 otherwise.
G  Position 34 (SetWarningMessageHook, candidate only) is called exactly
   once, before position 3, with rdx at least 0x7ff000000000 (the range
   where Windows loads program modules) and result 0.

Each log must be a different file recorded by run.sh in starts.log
(observelog.app_of) for one of the two games, one log per game, both
needed. Freshness (LOOP.md step 4): a log whose start (observelog.start_of)
is earlier than PLAN_COMMITTED is rejected. PLAN_COMMITTED is the committer
time (local, as run.sh writes starts.log) of the commit on loop/plan-11a
that holds the final plan text; it is set in a follow-up commit.

Prints only positions, handle values (small numbers), counts and
match/MISMATCH; no addresses, no account data.

Usage: python tools/shim/experiments/024_two_pipes.py <log> <log>
"""
import re
import sys
from datetime import datetime
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import observelog  # noqa: E402

GAMES = {
    1067310: ("Gravitas", "SteamClient018"),
    332950: ("Dark Deception", "SteamClient019"),
}
KIND = {
    5: re.compile(r"^SteamUser\d+$"),
    6: re.compile(r"^SteamGameServer\d+$"),
    9: re.compile(r"^SteamUtils\d+$"),
    12: re.compile(r"."),
    26: re.compile(r"^SteamController\d+$"),
    38: re.compile(r"^SteamInput\d+$"),
}
PLAN_COMMITTED = datetime(2099, 1, 1, 0, 0, 0)  # set to the final plan commit's time in a follow-up commit
RESULT = re.compile(r"^\d+ ms ThreadId\(\d+\) (\S+?)#(\d+) result (\d+) (.*)$")
STAND_IN = re.compile(r"replaced by stand-in (\d+) \(0x[0-9a-f]+\) for (\S+)$")
low = observelog.low32


def results(path):
    """(interface, position, number) -> list of `result` notes (shape, stand-in)."""
    out = {}
    for raw in open(path, encoding="utf-8", errors="replace"):
        m = RESULT.match(observelog.clean(raw))
        if m:
            out.setdefault((m.group(1), int(m.group(2)), int(m.group(3))), []).append(m.group(4))
    return out


def pipe_user(c):
    """(pipe, user) slots of a getter call; user None at position 9."""
    if c.position == 9:
        return low(c.reg("rdx")), None
    return low(c.reg("r8")), low(c.reg("rdx"))


def text_of(c):
    return c.texts.get("r8" if c.position == 9 else "r9")


def verdict(ok):
    return "match" if ok else "MISMATCH"


def check(path, version, label, verdicts):
    calls = [c for c in observelog.calls(path) if c.interface == version]
    notes = results(path)

    def say(point, ok, text):
        verdicts.append(ok)
        print(f"{label} {point}: {text} -> {verdict(ok)}")

    ok_a = len(calls) >= 2 and calls[0].position == 0 and calls[1].position == 2
    P = low(calls[0].rax) if ok_a else None
    U = low(calls[1].rax) if ok_a else None
    ok_a = ok_a and low(calls[1].reg("rdx")) == P
    say("A", ok_a, f"first calls {[c.position for c in calls[:2]]}, P={P} U={U}")

    threes = [c for c in calls if c.position == 3]
    sevens = [c for c in calls if c.position == 7]
    V = low(threes[0].rax) if len(threes) == 1 and threes[0].rax is not None else None
    ok_b = (len(threes) == 1 and low(threes[0].reg("r8")) == 3
            and (threes[0].reg("rdx") or 0) >= 0x10000 and V is not None and 0 < V < 0x10000
            and len(sevens) == 1 and calls.index(sevens[0]) < calls.index(threes[0])
            and (sevens[0].reg("r8") or 0) <= 0xFFFF)
    say("B", ok_b, f"{len(threes)} calls of 3 (r8 {[low(c.reg('r8')) for c in threes]}, V={V}), "
        f"{len(sevens)} calls of 7, 7 before 3: "
        f"{bool(threes and sevens and calls.index(sevens[0]) < calls.index(threes[0]))}")
    if not threes:
        say("C-G", False, "no position 3, nothing more to check")
        return
    at3 = calls.index(threes[0])
    getters = [c for c in calls if c.position in KIND]
    after = [c for c in getters if calls.index(c) > at3]
    Q = pipe_user(after[0])[0] if after else None
    before = [c for c in calls[:at3 + 1] if c is not threes[0]]
    seen_before = any(low(c.reg(r)) == Q for c in before for r in ("rdx", "r8"))
    as_result = any(low(c.rax) == Q for c in calls if c.rax is not None)
    ok_c = Q is not None and Q not in (P, 0) and not seen_before and not as_result
    say("C", ok_c, f"Q={Q}, used before position 3: {seen_before}, a result: {as_result}")

    bad_d = 0
    for c in getters:
        pipe, user = pipe_user(c)
        if pipe == P and (user is None or user in (U, 0)):
            continue
        if pipe == Q and calls.index(c) > at3 and (user is None or user in (V, 0)):
            continue
        bad_d += 1
    say("D", bad_d == 0 and bool(getters), f"{len(getters)} getter calls, {bad_d} with other handles")

    bad_e, by_key = 0, {}
    for c in getters:
        text = text_of(c)
        lines = notes.get((c.interface, c.position, c.number), [])
        shape = any(line.startswith("shape: first field ->") for line in lines)
        stand = [STAND_IN.search(line) for line in lines]
        stand = [m for m in stand if m]
        if not (text and KIND[c.position].search(text) and shape and len(stand) == 1
                and stand[0].group(2) == text):
            bad_e += 1
            continue
        by_key.setdefault((text, pipe_user(c)[0]), set()).add(int(stand[0].group(1)))
    unstable = sum(1 for v in by_key.values() if len(v) != 1)
    both = {t for (t, p) in by_key if p == P} & {t for (t, p) in by_key if p == Q}
    same = sum(1 for t in both if by_key[(t, P)] & by_key[(t, Q)])
    ok_e = bad_e == 0 and unstable == 0 and bool(both) and same == 0
    say("E", ok_e, f"{bad_e} getter calls without the text's shape and stand-in, {unstable} (text, pipe) "
        f"with more than one stand-in, {len(both)} texts over both pipes, {same} of them sharing a stand-in")

    fours = [c for c in calls if c.position == 4]
    pairs = sorted((low(c.reg("rdx")), low(c.reg("r8"))) for c in fours)
    ok_f = pairs == sorted([(P, U), (Q, V)]) and len(fours) == 2
    released, seq = set(), []
    for c in fours:
        i = calls.index(c)
        nxt = calls[i + 1:i + 3]
        seq.append([c.position] + [n.position for n in nxt])
        if [n.position for n in nxt] != [1, 23]:
            ok_f = False
            continue
        rel, shut = nxt
        pipe = low(c.reg("rdx"))
        if not (low(rel.reg("rdx")) == pipe and rel.rax is not None and rel.rax & 0xFF == 1):
            ok_f = False
        released.add(pipe)
        expect = 1 if {P, Q} <= released else 0
        if shut.rax is None or shut.rax & 0xFF != expect:
            ok_f = False
    say("F", ok_f, f"ReleaseUser (pipe, user) {pairs}, sequences {seq}")

    hooks = [c for c in calls if c.position == 34]
    ok_g = (len(hooks) == 1 and calls.index(hooks[0]) < at3
            and (hooks[0].reg("rdx") or 0) >= 0x7FF000000000 and hooks[0].rax == 0)
    say("G", ok_g, f"{len(hooks)} calls of 34")


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
        print(f"{name} ({GAMES[app][0]}): started {started}, before the plan commit {PLAN_COMMITTED} -> MISMATCH")
        verdicts.append(False)
        continue
    seen.add(app)
    game, version = GAMES[app]
    check(path, version, f"{name} ({game}, {version})", verdicts)
print("experiment 24:", "match" if seen == set(GAMES) and all(verdicts) else "MISMATCH")
