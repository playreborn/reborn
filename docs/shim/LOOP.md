# The shim loop (ADR 0051)

One iteration of autonomous work on the shim. An agent in a Claude Code
session started from a normal terminal (not the desktop app, whose
registry is virtualized) repeats it until stopped. Every rule of
`AGENTS.md` applies; this file only fixes the order.

## Before the first iteration of a session

1. `git pull` on `main`; `cargo build -p reborn-shim-observe -p reborn-shim`.
2. Check: the official client runs and is signed in to the test account;
   `reborn-observe status` says not armed.
3. `python tools/shim/coverage.py` regenerates `coverage.md` and
   `gaps.md`.

## One iteration

1. **Pick** the first row of `gaps.md` that no open pull request works on
   (`gh pr list`). Skip a row whose last attempt in `digest.md` ended
   without a candidate, unless new recordings exist.
2. **Look** at the recorded calls of that position in the logs listed in
   `tools/shim/games.json` (`tools/shim/observelog.py`): arguments,
   results, timing, what comes before and after.
3. **Candidate:** read the public page of that interface on
   <https://partner.steamgames.com/doc/api/> (one page at a time; never
   from memory, AGENTS.md section 4). If nothing fits, record "no
   candidate" in `digest.md` and go to the next row.
4. **Plan:** write the candidate and an experiment with predictions a
   script can check into `meanings.md`; write the check as
   `tools/shim/experiments/NNN_short_name.py` (reads logs given on the
   command line, prints match or mismatch per prediction; rejects a log given twice and
   checks which game each log came from with the `log` lines `run.sh`
   writes to `starts.log`). Commit and
   push the plan on a branch **before** any run.
5. **Run:** `tools/shim/run.sh <app id>` for the games the plan names
   (limits of ADR 0045 and 0052 apply: 2 minutes between starts, no daily cap; input in games under ADR 0049).
6. **Evaluate:** run the experiment script on the new logs; record the
   result in `meanings.md` with an evidence excerpt (no account data, no
   floating-point registers). Update `tools/shim/status.json`, then run
   `coverage.py` again.
7. **Implement** (confirmed meanings with a result): add Reborn's answer to
   the shim (`crates/shim`) with a test, and a shadow-mode entry in the
   proxy (ADR 0046) where Reborn can compute the answer. Answer mode only
   after shadow mode matched in three games.
8. **Merge** under ADR 0051: CI green, no new CodeQL alerts, and a review
   pass by a fresh agent that only sees the diff and `AGENTS.md`.
9. **Report:** one line in `digest.md`: date, position, result, PR.

## Stop and ask the maintainer

EULAs and other agreements, purchases, sign-ins, Windows administrator
prompts, anything a red line touches, credentials or account data beyond
ADR 0043 and 0050. Write the question into `digest.md` under "Waiting
for the maintainer" and continue with the next row.
