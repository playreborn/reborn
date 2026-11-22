# 0057: The notice before the first shim-mode launch

- **Status:** Accepted (by the maintainer, 2026-11-22)
- **Date:** 2026-11-22
- **Implements:** ADR 0055, section "Cloud", "Two save lines": "The
  frontend says so before the first shim-mode launch of a game."
- **Touches:** the public API of `reborn-ipc` (compatible additions,
  protocol 1.13).

## Context

In shim mode a game's saves stay in Reborn's own folder; they are not
uploaded to Steam Cloud yet, and the official client, also on the same
computer, never sees them, so one game can end up with two diverging
saves (ADR 0055). ADR 0055 requires the frontend to say so before the
first shim-mode launch of each game.

Only `rebornd` knows whether launches run in shim mode
(the environment variable `REBORN_SHIM_MODE`, read in
`apps/rebornd/src/shim_launch.rs`); the frontends do not. The desktop app
keeps no preferences of its own; its settings live in `rebornd` and
`reborn-platform::settings` (ADR 0019).

## Problem

Where "first shim-mode launch of this app" is decided and remembered,
and how the frontends learn that they must ask.

## Alternatives considered

1. **Each frontend remembers what it showed:** needs the frontends to
   know about shim mode (a new status field) and a store in each
   frontend; the GUI and the CLI would ask separately for the same app.
2. **`rebornd` decides and remembers, the frontends only ask:** one
   place for the decision, next to where shim mode is decided; one
   answer per app for every frontend.

## Decision

Option 2.

- `Request::Launch` gets `shim_notice_accepted` (default false, left out
  of the JSON when false, so a 1.12 request is unchanged).
- In shim mode, once the local checks pass (not running, not being
  installed, installed in the library) and before anything else of the
  launch (no session, no Valve request, no registry change), `rebornd`
  checks whether the user confirmed the notice for this app. If not, and
  the request does not carry the confirmation, it answers the new
  `Response::ShimModeNotice { app_id }`, starts nothing and writes
  nothing. A launch with `shim_notice_accepted: true` stores the
  confirmation and goes on.
- The confirmation is stored **before** the launch is known to succeed:
  the user saw and confirmed the text, so a launch that fails later (no
  session, shim mode refused, the game does not start) does not ask
  again.
- `rebornd` cannot verify that a frontend showed the text. The flag is a
  frontend convention: a frontend sends it only after the user confirmed
  `SHIM_MODE_NOTICE`. A frontend that sends it unasked skips the notice;
  that is the frontend's bug, not something the daemon can prevent.
- The confirmation is stored per app in the settings file
  `shim-notice-seen` (one app ID per line, ADR 0019's one-file-per-setting
  scheme). It holds app IDs only, no account data. The new list is
  written to a temporary file and renamed over the old one, and the
  read-modify-write is done under a lock, so two first launches at the
  same time cannot lose an app ID.
- The text is one constant, `reborn_ipc::SHIM_MODE_NOTICE`, so every
  frontend says the same.
- Frontends older than protocol 1.13 cannot confirm; `rebornd` sends them
  `Response::Error` with the notice instead, and the game does not start.
  The same holds for the cloud conflict of ADR 0055
  (`Response::CloudConflict`), which also arrives in 1.13.
- The notice comes before the cloud conflict, which needs a session.
  Frontends keep the answers of one launch together and send all of them
  with every resend, so a resend after a cloud conflict still carries
  `shim_notice_accepted`.
- The desktop app shows a dialog over the page with "Continue" and
  "Cancel". `reborn-cli daemon launch` prints the notice and continues
  only with `--yes` or when the user types "yes"; without a terminal and
  without `--yes` it does not start the game.

## Consequences

- Without shim mode nothing changes: the flag is ignored and nothing is
  stored.
- The confirmation is per computer user, not per account: the notice is
  about where files go on this computer, which is the same for every
  account.
- There is no way yet to show the notice again for an app except
  deleting `shim-notice-seen`.
- If the confirmation cannot be stored, the launch still goes on (the
  user just confirmed); the next launch asks again.
