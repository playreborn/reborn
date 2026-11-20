# 0034: Restoring missing saves from Steam Cloud

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

ADR 0031 step 4 is sync around a game session. The official client's
`cloud_log.txt` (`docs/protocol/cloud.md`) shows that it downloads before
a game starts and uploads after it exits. Uploading is blocked: the
client sent a 20496-byte save as a 9019-byte body, and how the body is
transformed is not known from any source. Downloading is backed:

- the download itself (ADR 0032) and the mapping to local paths
  (ADR 0033) are verified live;
- the official client also moves local files of another account aside
  before it syncs, so a folder can hold saves that are not this
  account's.

## Problem

How Reborn brings the account's cloud saves to this computer without
ever replacing a local save or mixing two accounts' saves.

## Alternatives considered

1. **Mirror the cloud into the save folders:** replaces local progress
   on any difference. Rejected (AGENTS.md: never overwrite on conflict).
2. **Move other files aside like the official client:** needs the format
   of `steam_autocloud.vdf` or the client's bookkeeping, not recorded.
3. **Restore only into a clean state:** write the cloud files that are
   missing locally, and only when nothing local conflicts.

## Decision

Option 3, first as a command, later before a game starts.

- `reborn-core`: `restore_missing_saves(session, app_id, library)`
  compares like ADR 0033 and then:
  - does nothing and reports a conflict if any file **differs**, or if
    the save folders hold files that match a rule but are **not in the
    cloud** (unsynced progress, or another account's saves);
  - otherwise downloads every file that is **missing locally** to its
    mapped path; files that are not mapped are skipped and listed.
- `reborn-content`: `download_cloud_file_into(client, base, relative,
  file)` writes to a path below a root folder that the caller already
  checked; `download_cloud_file` (ADR 0032) uses it too. All rules of
  ADR 0032 stay: listed size limit, SHA-1 before writing, part file then
  rename, no symlinks, never overwrite.
- The root folder itself must exist (e.g. `AppData\LocalLow`); Reborn
  only creates folders below it.
- `reborn-cli cloud <app_id> --restore` runs it and prints what happened.
- Wiring it into game launch in the daemon and GUI follows once the
  command is verified live, in its own change.
- Nothing is uploaded or deleted; the account is not changed.

## Consequences

- Writes into game save folders: needs human review (AGENTS.md
  section 7).
- A conflict always stops everything for that app; the user decides
  (later in the GUI).
- Uploading stays open until the upload format is backed by a source.
