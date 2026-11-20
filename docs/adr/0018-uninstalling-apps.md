# 0018: Uninstalling apps

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

Reborn installs, updates, verifies and starts games, but could not remove
them; the plan lists "Deinstallieren" for the game details (§ 10). Removing
means deleting files, which AGENTS.md § 7 lists for human review, and the
folder name comes from Steam data (the appmanifest's `installdir`).

## Problem

What uninstalling removes, how it stays inside the library, and how
frontends ask for it.

## Alternatives considered

1. **Move files to the system trash:** recoverable, but needs per-OS trash
   APIs and fills the trash with tens of gigabytes.
2. **Delete the game folder, download leftovers and the appmanifest:** what
   a library without the game looks like; permanent, so frontends confirm.

## Decision

Option 2.

- `reborn-core::uninstall::uninstall_app` removes
  `steamapps/common/<installdir>`, `steamapps/downloading/<app id>` and the
  appmanifest, in that order, so a failed removal can be retried. The
  folder name must be a single plain path component (same check as for
  installs). `remove_dir_all` does not follow symlinks; a symlinked game
  folder only loses the link.
- Save games outside the game folder and Proton prefixes
  (`compatdata/<app id>`) stay, because they may hold the player's data.
- IPC protocol 1.3 (compatible additions): request `uninstall { app_id,
  library }` answers `ok`; event `app_uninstalled { app_id }`. The daemon
  refuses while the app is running, installing or verifying.
- The desktop app asks twice ("Uninstall", then "Really delete?"). The CLI
  offers `daemon uninstall <app id>`.

## Consequences

- Freed space is not reported yet.
- Removing Proton prefixes and cloud-synced saves on request can come with
  the settings screen.
