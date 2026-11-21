# 0039: Loading the observation proxy

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

ADR 0038 stage 2 is a logging proxy that is loaded instead of
`steamclient`, loads the real one and forwards the entry points recorded
in stage 1. Recorded so far (`docs/shim/loading.md`,
`docs/shim/entry-points.md`):

- Windows: the game reads `HKCU\Software\Valve\Steam\ActiveProcess` and
  loads `steamclient64.dll` from the path in `SteamClientDll64`, nowhere
  else. The official client did not write that key while the game ran
  (it only read `pid`, Process Monitor, 2026-11-21); when it writes it is
  not recorded.
- The game asks `steamclient64.dll` for five entry points.
- Under Proton the same key exists inside the prefix, and Proton copies
  the `steamclient` DLLs into the prefix before each start.

ADR 0038 forbids changing Valve's or the game's files, and changing the
official client's state silently.

## Problem

How the proxy gets loaded on Windows, and who changes
`SteamClientDll64`.

## Alternatives considered

1. **The user changes the value by hand** (Registry Editor) for one
   observation session and sets it back afterwards. Reborn never writes
   the official client's registry. Error-prone, but nothing in Reborn
   touches Valve's state.
2. **A Reborn command does it on the user's request** (`arm` saves the
   current value and writes the proxy's path; `disarm` restores it). More
   convenient and less error-prone, but Reborn code writes the official
   client's registry, which needs human review and a careful restore.
3. **Replace the file in the official client's folder:** forbidden by
   ADR 0038.
4. **Proton prefixes:** Proton sets up the prefix before each start, so a
   change there is likely lost; deferred until Reborn runs games through
   Proton itself (ADR 0016, ADR 0017), where the prefix is Reborn's.

## Decision

Option 2, decided by the maintainer on 2026-11-21. The commands:

- live in the same crate as the proxy, as a small program
  (`reborn-observe`), so the shim tools stay together (ADR 0012);
- `arm`: refuses if a previous session was not disarmed, or if
  `SteamClientDll64` is missing or already points to a Reborn path.
  Otherwise it saves the current value to a file in Reborn's data folder
  (which the proxy also reads to find the real DLL), copies the proxy to
  Reborn's folder and writes the proxy's path to `SteamClientDll64`;
- `disarm`: writes the saved value back, but only if `SteamClientDll64`
  still holds the proxy's path (if something else changed it in the
  meantime, it leaves it alone and says so), then removes the saved
  state. It can be run any number of times;
- `status`: shows the current value and whether a session is armed;
- touch nothing else: no other value, no key outside
  `HKCU\Software\Valve\Steam\ActiveProcess`, no file outside Reborn's
  folders. They are only ever run by the user.

The rest:

- The proxy is a `cdylib` in its own crate, `crates/shim-observe`
  (`unsafe` allowed, each block with a `// SAFETY:` comment, human
  review). It is copied to a folder of Reborn's, never into the official
  client's or the game's folder.
- The path of the real `steamclient64.dll` is not built in: it is the
  value `SteamClientDll64` had before the session, given to the proxy in
  a small text file next to it.
- The proxy exports exactly the five entry points from stage 1 and
  nothing else, forwards each call unchanged, and logs as ADR 0038 says
  (raw argument values, text only for printable ASCII, buffers by length
  and SHA-256 only).
- One session at a time: set the value after the official client has
  started, run the game once, set the value back before the official
  client is closed. If the game misbehaves with the proxy, the session is
  stopped and the value restored.
- Windows 64-bit only for now; 32-bit games (`SteamClientDll`) and Proton
  later.

## Consequences

- Reborn writes one value of the official client's registry, only on
  the user's command and always with a saved copy. `arm` and `disarm`
  are human review items (AGENTS.md section 7).
- A crash of the game or the proxy leaves the value pointing to the
  proxy until the user runs `disarm`; `status` shows it.
- The proxy is the first `unsafe` code outside `reborn-platform`.
