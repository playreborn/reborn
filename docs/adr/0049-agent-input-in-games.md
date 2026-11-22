# 0049: Input in games during agent runs

- **Status:** Accepted
- **Date:** 2026-11-22
- **Amends:** ADR 0044

## Context

ADR 0044 lets an AI agent carry out observation runs but forbids any
keyboard, mouse or controller input to games, and clicks in the official
client except in the add and install dialogs. Several candidate games
need input before they load Steam (Coloring Pixels shows a settings
window with a Play button at every start), and some calls only happen
in a game, not in its menu (position 12 of
`STEAMUSERSTATS_INTERFACE_VERSION011` in Cave Crawlers). Confirming
position 6 needs an achievement unlocked by playing
(`docs/shim/meanings.md`). Until now the maintainer did these steps by
hand.

The maintainer said on 2026-11-22 that the agent may click in games,
load into them and play them, and that only decisions such as accepting
an EULA must stay with the maintainer.

## Decision

The agent may send keyboard and mouse input to a game during a run under
ADR 0044 and 0045, within these limits:

- **Purpose:** only to reach the screen a run needs (past a launcher or
  settings window, a language choice, a main menu, into a level), or to
  carry out an experiment planned in `docs/shim/` before the run (for
  example playing until a named achievement unlocks).
- **Never for rewards:** no play for play time, trading cards, drops,
  in-game currency or anything else of value (red line 5). Runs stay
  within the 10-minute limit of ADR 0044 unless a planned experiment
  names a longer one, at most 30 minutes.
- **Achievements and stats** change only because the game itself
  records play that happened (red line 4). The agent never aims at an
  achievement unless an experiment names it, and never uses anything
  other than normal play to get there.
- **No multiplayer or online play**, and never in games with anti-cheat
  (they are not run with the proxy at all, `docs/shim/m4-games.md`).
- **Stop and ask** at anything that is the maintainer's decision: an
  EULA, any agreement or consent screen, a purchase or payment, account
  sign-in or linking (including third-party accounts), and Windows
  prompts for administrator rights.
- In the official client, clicks stay limited to adding free games and
  the install dialog (ADR 0044), plus the Play button of a game the
  test account owns.
- Input is sent the ordinary way (Windows' mouse and keyboard input); no
  memory editing, no injected code, no speed hacks.

## Consequences

- Games with a launcher window or a language screen can be recorded
  without the maintainer, and calls that only happen in a game become
  observable.
- Unlocking an achievement for a planned experiment is possible without
  the maintainer playing; it is still real play on the test account.
- Everything that needs consent stays with the maintainer.
