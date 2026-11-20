# Changelog

All notable changes to Reborn are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [0.1.1] - unreleased

### Added

- **Settings** tab in the desktop app (ADR 0019):
  - choose the folder where new games are installed, or go back to the
    default;
  - on Linux, choose which installed Proton build runs Windows games, or
    keep the automatic choice.

## [0.1.0] - 2026-11-20

The first alpha: install, update and play the games of your Steam account
without the command line, on Windows and Linux.

### Added

- **Sign in** with account name, password and Steam Guard (code from the
  authenticator or by email, or approval in the Steam mobile app). The
  login is stored only in the system keyring (Windows Credential Manager,
  Secret Service on Linux).
- **Library** of all apps your account owns, read from your licenses.
- **Own download engine:** install, update, resume interrupted downloads,
  verify and repair, and uninstall. Installs use the Steam library format, so the
  official client recognizes them.
- **Desktop app** (`reborn`): sign in and out, library with search,
  install / update / verify / uninstall, download queue with progress and
  cancel, Play, Quit.
- **Background service** (`rebornd`): owns the Steam session and the
  download queue; the app starts it when needed. It is reachable only by
  your own user.
- **Playing games:** native games start directly. On Linux, Windows-only
  games are installed with their Windows files and start through an
  installed Proton build.
- **Command line** (`reborn-cli`) for scripting and debugging.
- **Compatibility database** format (`compat/`) and a report template, so
  anyone can report how well a game runs.

### Known limitations

- Most games use Steamworks and need the **official Steam client running in
  the background** (compatibility tier 1). Reborn's own replacement for it
  is planned for milestone M4.
- No QR-code sign-in yet.
- Proton is not downloaded by Reborn: install one in the Steam client
  (for example Proton Experimental). It runs without the Steam Linux
  Runtime container, which some games may need.
- Install scripts (redistributables such as DirectX or Visual C++) are not
  run yet; some games may not start on a fresh system.
- No settings screen yet; games go into Reborn's own library (settings
  follow in 0.1.1).
- The binaries are not code-signed; Windows SmartScreen may warn.
- On Windows PCs shared with other users, another user could in theory
  impersonate the background service before it starts (see ADR 0014).

[0.1.1]: https://github.com/playreborn/reborn/releases/tag/v0.1.1
[0.1.0]: https://github.com/playreborn/reborn/releases/tag/v0.1.0
