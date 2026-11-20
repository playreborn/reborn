# 0031: Steam Cloud, step by step

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

Steam Cloud keeps save games per account. Without it, switching between
Reborn and the official client loses progress. The plan asks for a spike
first, because Cloud is less documented than the rest (`docs/plan.md`,
7.5). The spike is `docs/protocol/cloud.md`:

- Listing and downloading are described by ValvePython/steamctl (MIT)
  and Valve's public `ICloudService` documentation.
- Uploading and deleting from a client, and the order of calls around a
  game session, are only described by third-party tools.

Cloud touches the user's data twice: files on disk and files in the
account. AGENTS.md demands human review for both and forbids overwriting
on conflict.

## Problem

In which order Reborn builds Cloud support, so that nothing is guessed
and no save game is lost.

## Alternatives considered

1. **Full sync at once:** too much unverified behaviour at the same time;
   a mistake could delete saves in the account.
2. **Step by step, each step recorded and verified before the next:**
   slower, but every step can be checked against the official client.

## Decision

Option 2, in these steps:

1. **Read (this ADR):** `Session::cloud_files(app_id)`
   (`Cloud.EnumerateUserFiles`, paged, at most 20 pages) and
   `Session::record_cloud_files(app_id)`. `reborn-cli cloud <app_id>
   [--record <dir>]`. Fixtures drop download links, UGC IDs and the
   creator's SteamID, and replace the own SteamID in file names. New
   public items in `reborn-cm`: `CloudFile`, `cloud_files_from`.
   `Cloud.EnumerateUserApps` (all apps with cloud files) is not used:
   Steam answered Fail, also for an account with cloud files.
2. **Download to a separate folder:** files go to a folder of Reborn's,
   never into a game's save folder, checked against `file_sha`. Own ADR;
   writes files, so human review.
3. **Map files to local paths:** Auto-Cloud roots from the app info,
   compared without writing. Own ADR.
4. **Sync around a game session:** only after recording what the
   official client sends at launch and exit. Never overwrites on
   conflict; the user decides. Uploads act on the account: own ADR and
   human review.

## Consequences

- Step 1 changes nothing on disk or in the account.
- Each later step has a recording to test against before it lands.
- Until step 4, users still need the official client to sync saves.
