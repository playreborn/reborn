# 0033: Mapping Steam Cloud files to local paths

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

ADR 0031 step 3: tell for each cloud file where it belongs on this
computer and whether the local copy matches, without writing anything.
Recordings (`docs/protocol/cloud.md`) show:

- Games that save through the Steam interface have no `savefiles` rules;
  their cloud names have no root token (Cave Crawlers).
- Auto-Cloud games have `ufs` → `savefiles` (`root`, `path`, `pattern`,
  `recursive`, `platforms`) and `rootoverrides`. Their cloud names are the
  root token, the rule's path and the file name, e.g.
  `%WinAppDataLocalLow%Studio312/Coin Maker Simulator Demo/save_slot_demo_1.json`
  (Coin Maker Simulator Demo).
- Valve's Steam Cloud documentation lists where the roots are on each
  system and that patterns use `*` as wildcard.

Unknown: whether `addpath` in a root override replaces the path or is
added to it (the recorded Linux override of Coin Maker Simulator Demo
only makes sense as a replacement, Valve's documentation says "add");
where Windows games keep their saves under Proton.

## Problem

How Reborn finds the local counterpart of a cloud file and the local
files that would belong in the cloud, without guessing the unknown parts.

## Alternatives considered

1. **Full mapping including Linux overrides and Proton prefixes now:**
   rests on guesses about `addpath` and the prefix layout.
2. **Map what is documented and recorded, report the rest as not
   supported:** less coverage, but every answer is backed.

## Decision

Option 2.

- `reborn-formats`: `cloud_config(document)` reads `appinfo` → `ufs`
  into `CloudConfig` (`quota`, `max_files`, `save_files`,
  `root_overrides`). Entries that are not objects (such as
  `copied_from_parent`) are skipped.
- `reborn-platform`: `cloud_root_dir(root, game_dir)` maps a root name
  (case-insensitive) to a folder, following Valve's documentation:
  - Windows: `WinMyDocuments` (Documents), `WinAppDataLocal`,
    `WinAppDataRoaming` (from the `directories` crate, which asks Windows
    for the current locations), `WinAppDataLocalLow` and `WinSavedGames`
    (below the home folder), `gameinstall` (the game's install folder).
  - Linux: `LinuxHome`, `LinuxXdgDataHome`, `LinuxXdgConfigHome` (XDG
    folders from `directories`), `gameinstall`.
  - Anything else: not supported.
- `reborn-core`: `compare_cloud(files, config, resolve)` gives each cloud
  file a state: same (SHA-1 equal), differs (with both times), missing
  locally, or not mapped with the reason:
  - no root token (saved through the Steam interface; needs the shim),
  - an unknown or unsupported root,
  - a root override for this system (`addpath` unknown),
  - a Windows root on Linux (Proton prefix not supported yet),
  - an unsafe name (checked like install paths).
  `local_only_files(...)` lists local files that match a `savefiles` rule
  (path with `{64BitSteamID}` and `{Steam3AccountID}` filled in, `*`
  pattern, `recursive`) but are not in the cloud.
- Only reads and hashes files; symlinks are not followed.
- `reborn-cli cloud <app_id> --compare` prints the result.

## Consequences

- Nothing is written; this step needs no human review beyond the usual.
- Linux overrides and Proton saves stay open until recorded on the Linux
  VM.
- Step 4 (sync) can rely on these states to decide what to download or
  upload, and stops on "differs" for the user to decide.
