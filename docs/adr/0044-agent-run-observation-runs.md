# 0044: Observation runs carried out by an AI agent

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

Every observation recording in `docs/shim/` so far was made by hand: the
maintainer armed the proxy with `reborn-observe`, started a game through
the official client, quit it, disarmed and sent the log lines to the AI
agent, which then analysed them and wrote the entries. Most of the
maintainer's time went into these steps, and many runs need no playing at
all: the game only has to start, reach its menu and quit (stage 3 runs,
experiment 2 of `docs/shim/meanings.md`).

The rules today:

- AGENTS.md § 6: "Live experiments are manual, with a secondary test
  account, and never part of `cargo test`."
- ADR 0038: only games the user owns, with the official client running
  and signed in; developers use secondary accounts.
- ADR 0039: `reborn-observe` changes one registry value, only when the
  user runs it.
- ADR 0042: the maintainer reviews every candidate and every
  confirmation.
- Red lines 4, 5 and 8: no manipulated achievements or stats, no
  idling or farming, be gentle with Valve's servers.

The maintainer has asked the agent to take these runs over on the
maintainer's computer.

## Problem

May an AI agent carry out observation runs on the maintainer's computer,
and if so, within which limits, so that the red lines hold and the
maintainer still decides what goes into the project?

## Alternatives considered

1. **Keep everything manual.** Safe, but slow; the recorder's time is
   the bottleneck of stage 4.
2. **The agent runs observation runs within fixed limits** on the
   maintainer's computer, and the maintainer reviews the results in
   batches.
3. **A fully automated test rig** (scheduled or in CI) that starts games
   on its own. Rejected: it would act on a real account without anyone
   watching, and runs in CI are excluded by red line 8.

## Decision

Option 2. An AI agent working in a session the maintainer started may
carry out observation runs under these rules.

### Before a run

- The official client is already running and signed in with the test
  account. The agent never signs in, never types a password or a Steam
  Guard code, and never starts or quits the official client. If it is
  not running or not signed in, the agent stops and asks.
- Only games that the test account owns. The agent never buys a game.
- The agent may add a game that costs nothing (free to play, a demo, or
  free to keep) to the test account, if the game is on the M4 list or
  needed for an experiment planned in `docs/shim/`, at most 3 a day. It
  does so in the official client, through the store page's button that
  adds the game or the install dialog below. Ownership stays real: the
  license comes from Valve's servers (red line 3). Never in bulk and
  never for rewards (red line 5).
- The agent may install or update an owned game through the official
  client if the size the client shows is at most 10 GB: it hands
  `steam://install/<app id>` to the client and, in the install dialog
  that opens, picks only the default folder and the button that starts
  the download.
- These two are the only places where it clicks in the official client.
  A game over 10 GB, or a dialog asking for anything else (another
  folder, an agreement, a payment), is left for the maintainer. It does
  not uninstall games.
- Each run has a purpose written down first: a stage of ADR 0038, 0040
  or 0041, or an experiment planned in `docs/shim/meanings.md`
  (ADR 0042).

### During a run

The agent may only:

- arm and disarm with `reborn-observe` (ADR 0039), with a proxy built
  from this repository;
- start a game by handing `steam://rungameid/<app id>` to the official
  client;
- watch the game's process and the proxy's log;
- ask the game to close normally (a close request to its window, never a
  forced kill), and disarm afterwards.

It never sends keyboard, mouse or controller input to a game, and never
clicks in the official client or a game (apart from adding free games
and the install dialog above). Runs that need playing stay with a person.

It stops, disarms if it can, and reports instead of acting when
anything unexpected happens: the game does not start, crashes, does not
close within two minutes of the close request, or the official client
shows any dialog (for example a cloud conflict, an agreement or an
update prompt). Such dialogs are left for the maintainer.

### Limits

- At most 10 game starts a day, at least 5 minutes apart, each run at
  most 10 minutes. Runs are never made for play time, trading cards or
  other rewards (red line 5).
- No action on the account beyond what the game does by itself and
  adding free games as above: no achievements, stats, friends,
  purchases, cloud or settings changes (red lines 3 and 4).
- Screenshots of the desktop are allowed, to see whether a game reached
  its menu or a dialog is open. They stay on the computer and never go
  into the repository, fixtures or docs; what goes into `docs/shim/`
  comes from the proxy's log. Looking is not acting: the rule against
  input above still holds.

### After a run

- Raw logs stay on the computer, as before. Entries in `docs/shim/`
  follow the existing redaction rules.
- Entries name the recorder as "an AI agent on the maintainer's
  computer, on the maintainer's instruction". The agent has not accepted
  the Steamworks SDK License; AGENTS.md § 4 (memory is not a source)
  applies to it as before.
- An experiment the agent ran is written up with its result, but its
  meaning is marked confirmed only after the maintainer's OK
  (ADR 0042). The agent collects such results and asks for them
  together, not one at a time.
- The agent may commit its work on a separate branch, push that branch
  and open a pull request. Merging into `main`, accepting ADRs, the
  human review of `unsafe` code (AGENTS.md § 7) and releases stay with
  the maintainer.

### Change to AGENTS.md

§ 6 reads afterwards: "Live experiments are run by a person, or by an AI
agent within ADR 0044, with a secondary test account, and are never part
of `cargo test`."

## Consequences

- Runs that only need a game to start and quit no longer take the
  maintainer's time; experiments that need playing still do.
- Every action on the account stays within what the game does by itself
  and what a person would do by hand, at a lower rate than a person
  might.
- The maintainer reviews in batches, so mistakes can sit on a branch
  until the review; nothing reaches `main` without it.
- The maintainer can stop a session at any time; these rules end with
  the session that the maintainer started.
