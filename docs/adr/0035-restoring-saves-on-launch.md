# 0035: Restoring missing saves when a game starts

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

ADR 0034 added `restore_missing_saves`: it writes cloud saves that are
missing locally into their save folders and refuses to write anything
when a local save differs or is not in the cloud. It was verified live
as `reborn-cli cloud <app_id> --restore`. The official client downloads
before a game starts (`AC Launch, down` in its cloud log), so that is
where users expect it.

## Problem

When `rebornd` runs the restore, what happens on a conflict or an error,
and how frontends learn about it.

## Alternatives considered

1. **Refuse to start the game on a conflict:** the user cannot play
   until they sort out their saves elsewhere.
2. **Start the game anyway and tell the user:** the game sees the local
   saves, exactly as it would without Reborn; nothing is lost, because
   Reborn neither overwrites nor uploads.
3. **A setting to turn it off:** not needed while the restore only adds
   missing files; can come with uploads.

## Decision

Option 2.

- `rebornd` runs `restore_missing_saves` for the app's library after the
  launch is planned and before the game process starts, with a 30-second
  limit. Errors and timeouts never stop the game.
- IPC 1.12: `Event::CloudRestore { app_id, result }` with
  `CloudRestoreResult`:
  - `Restored { files }`: saves were written,
  - `Conflict { message }`: nothing written because local saves differ
    or are not in the cloud,
  - `Failed { message }`: Steam or the download failed, or it timed out.
  No event when there was nothing to do.
- The GUI shows the result as a notice; a conflict says that the game
  started with the local saves.
- Nothing is uploaded; that stays blocked (ADR 0034).

## Consequences

- Starting a game needs one app info request, one file list request
  and the downloads, usually about a second (red line 8: once per start).
- Writes into game save folders when a game starts: needs human review
  (AGENTS.md section 7).
- Conflicts are only reported; resolving them comes with uploads.
