# 0030: Achievements in the daemon and the GUI

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

ADR 0029 added reading achievements to `reborn-cm` and the CLI; the
format is verified with a recording. Frontends talk only to `rebornd`
over IPC, so the daemon has to pass achievements on.

## Problem

How frontends get an app's achievements, and how the GUI shows them.

## Alternatives considered

1. **The daemon loads achievements for every owned app at sign-in:** one
   request per app, many requests nobody looks at (red line 8).
2. **On request, per app:** one request when the user opens the list.

## Decision

Option 2.

- IPC 1.11: `Request::Achievements { app_id, language }` (language
  optional, Steam's language names, default `english`), answered with
  `Response::Achievements { app_id, achievements }`. Each
  `AchievementInfo` has the internal name, display name, description,
  hidden flag and unlock time (seconds since the Unix epoch, `null` while
  locked). The daemon sends everything; frontends decide what to show.
- Reading is safe to repeat, so the daemon signs in again and retries
  once when the connection was lost, like the chat history (ADR 0024).
- No cache: every opening asks Steam once.
- The GUI gets an "Achievements" button per game. The list shows how many
  are unlocked, unlocked ones first (newest first, with date in UTC), then
  locked ones in the game's order. Locked hidden achievements show neither
  name nor description, like the official client.
- The GUI asks in English, as the rest of the GUI is English. A language
  setting can come later.
- Reborn still never writes stats or achievements (red line 4).

## Consequences

- One request per opening; nothing in the background.
- Icons and global unlock percentages are not shown yet (they need
  `Player.GetGameAchievements` and image loading).
