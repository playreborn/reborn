# 0048: How games find the shim when Reborn starts them

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

M4's goal is a system on which the official client was never installed
(`docs/plan.md`). Then nothing tells a game where `steamclient` is,
unless Reborn does. What is recorded (`docs/shim/loading.md`):

- **Windows:** before loading, the game reads under
  `HKCU\Software\Valve\Steam\ActiveProcess` the values `pid`,
  `SteamClientDll` and `SteamClientDll64`, and loads `steamclient64.dll`
  from exactly the path in `SteamClientDll64`. No other place is tried.
  What `pid` is checked for is not known; in the recording it held the
  official client's process ID.
- **Windows games under Proton:** Proton copies its own
  `steamclient64.dll` into the game's prefix and writes the same key
  inside the prefix's registry, with `PID`, `SteamClientDll`,
  `SteamClientDll64` and `SteamPath`. When Reborn runs a game through
  Proton, the prefix is Reborn's own (ADR 0016, ADR 0017).
- **Native Linux games:** not recorded yet; ADR 0012 notes that
  `steam_api` tries a local `steamclient.so`, then
  `~/.steam/sdk64/steamclient.so`.
- The observation proxy already proves that a game loads whatever DLL
  `SteamClientDll64` names (ADR 0039, `docs/shim/objects.md`).

Not recorded yet: what a game does when `pid` names a process that is
not the official client, or when the values are missing; whether
`SteamPath` matters on Windows; whether games call
`SteamAPI_RestartAppIfNecessary` and hand a `steam://` URL to the system
when they were not started the expected way.

## Problem

Where does Reborn put the shim and what does it write so that a game it
starts loads the shim, on a system with or without the official client,
without silently changing the official client's state and without
guessing values whose meaning is not recorded?

## Alternatives considered

1. **Install-time setup:** write the registry values once when Reborn is
   installed. Simple, but it overwrites the official client's values if
   that client is installed too, and every game started any way would
   load the shim.
2. **Per launch:** `rebornd` writes the values right before it starts a
   game in shim mode and restores what was there when the game exits,
   with the same safeguards `reborn-observe` uses (ADR 0039).
3. **Replace or copy files into game folders.** Rejected: it changes game
   installs and is close to patching games (red line 1).

## Decision

Option 2, decided in steps; each step that depends on unrecorded
behaviour starts with an observation, not an assumption.

### Where the shim lives

- Windows: `steamclient64.dll` in Reborn's data folder (next to where
  `reborn-observe` puts the proxy today), installed and updated with
  Reborn.
- Inside a Proton prefix that Reborn manages: the Windows shim, placed
  by Reborn when it prepares the prefix.
- Native Linux: decided after the first native recording.

### Shim mode per launch (Windows)

- A launch from `rebornd` can run in **shim mode**. Then `rebornd`:
  1. refuses if the official client is running (its values are live
     state; ADR 0038 forbids changing them silently);
  2. saves the current `ActiveProcess` values, if any, in Reborn's data
     folder;
  3. writes `SteamClientDll64` (and `SteamClientDll`, pointing to the
     same folder) to the shim;
  4. starts the game, and restores the saved values, or removes the ones
     it created, when the game process exits, and also at its own start
     if a previous run was interrupted.
- What `pid` gets is decided by **experiment 6** (below). Until then shim
  mode stays off by default and is only used for planned experiments.
- Games started any other way are not affected.

### Proton (Reborn's own prefixes)

- The same values are written into the prefix's registry when Reborn
  prepares it, pointing to the Windows shim in the prefix. Because the
  prefix belongs to Reborn, nothing of the official client changes. The
  role of Proton's `lsteamclient` in such a prefix is checked by an
  observation run before shim mode is used there.

### Experiment 6 (to be planned in `docs/shim/` before it runs)

With the observation proxy (official client running, as allowed today),
record what the game reads and how it behaves when `pid` names
`rebornd`'s process instead of the official client's. If the game
behaves the same, `pid` gets `rebornd`'s process ID in shim mode; if
not, a new ADR decides.

### `steam://` and restarts

- If recordings show games handing `steam://` URLs to the system
  (`SteamAPI_RestartAppIfNecessary`), Reborn registers as the handler
  only on systems without the official client, by a separate decision.

## Consequences

- On a system without the official client, a game started from Reborn
  finds the shim; started from a desktop shortcut it does not, until the
  `steam://` decision.
- With the official client installed, shim mode only runs while that
  client is closed, and its values come back afterwards.
- Writing the registry is a change to the user's system that
  `rebornd` makes per launch; it needs human review (AGENTS.md section
  7) and is logged.
- Native Linux and the meaning of `pid` stay open until recorded.
