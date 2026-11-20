# 0019: Settings for the library folder and the Proton build

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

Release 0.1.0 always installed into Reborn's built-in library folder (in
the user's local data directory, usually on the system drive) and always
picked the newest installed Proton build (ADR 0017). Both are the first
things users want to change. Settings so far were one small text file per
value in the config directory (`default-account`).

## Problem

How settings are stored, who changes them, and how frontends show them.

## Alternatives considered

1. **One settings file (TOML) for everything:** tidy, but needs a schema,
   migration and a parser in `reborn-platform`.
2. **One small text file per setting, as for the default account:** no new
   dependency, trivially readable and editable, enough for a handful of
   values.

## Decision

Option 2, extended.

- `reborn-platform::settings`: `library` (absolute folder; created with its
  `steamapps` folder when set) and `proton` (display name of a build). A
  missing or blank file means "not set". `default_library()` returns the
  set folder, else the built-in one, so install, launch, uninstall and the
  installed list follow the setting without further changes.
- The chosen Proton build is used if it is installed; otherwise the
  automatic choice from ADR 0017 applies and a warning is logged.
- IPC protocol 1.4 (compatible additions): `get_settings`, `set_library {
  library }` (refused while downloads are queued or running) and
  `set_proton { name }` (only installed builds) answer `settings { library,
  library_is_builtin, proton, protons }`; the event `settings_changed` tells
  every frontend.
- The desktop app gets a Settings tab: a text field for the library folder
  with Save and "Use default", and on Linux a list of the installed Proton
  builds plus "Automatic".

## Consequences

- Games already installed stay in their folder; changing the library only
  affects new installs and what the library list shows. Moving games and
  several libraries at once can come later.
- There is no folder picker yet; the path is typed or pasted.
- More settings can be added the same way until a single file pays off.
