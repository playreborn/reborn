# 0017: Windows games on Linux through Proton

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

ADR 0016 starts games natively and left Windows-only games on Linux with
an error until Proton support lands. `reborn-platform::proton` (from the
compatibility work, ADR 0015) finds installed Proton builds and builds the
`proton run` command line with sources for every variable
(`docs/platform/proton.md`). Proton needs a per-app data directory
(`STEAM_COMPAT_DATA_PATH`) and newer builds declare that they expect the
Steam Linux Runtime container (`toolmanifest.vdf`).

## Problem

Which Proton build runs a game, where its data lives, and whether Reborn
wraps it in the runtime container now.

## Alternatives considered

1. **Download and manage Proton with Reborn's content engine:** the plan's
   goal (§ 9.2), but more work; installed builds from the Steam client or
   `compatibilitytools.d` already exist on most Linux gaming systems.
2. **Use installed builds, run Proton directly:** small, testable now;
   the container and downloads can follow.
3. **Use installed builds inside the runtime container:** closest to the
   official client, but pressure-vessel's invocation is not verified yet.

## Decision

Option 2.

- `reborn-core::launch` falls back to Proton on Linux when an app has only
  Windows launch entries: it plans the Windows entry (same path checks as
  ADR 0016) and wraps it with `proton_command`.
- Choice of build (Reborn's default until a setting exists): the newest
  numbered official build (`Proton 9.0`, `Proton 10.0`, ...), else another
  official build such as `Proton Experimental`, else a custom build.
- The prefix lives in Reborn's data folder, `compatdata/<app id>`, created
  right before the start; Steam libraries stay untouched.
- If no build is installed, the launch fails with a hint to install one.

## Consequences

- Proton runs without the runtime container; whether that works for a
  given build is checked live. Wrapping in the container, letting the user
  choose a build per game, and downloading Proton with Reborn come later.
- Steamworks games under Proton still need the official client running
  (tier 1), as on Windows; Proton's `lsteamclient` path is verified in M4.
