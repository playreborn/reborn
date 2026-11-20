# Changelog

All notable changes to Reborn are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Reconnecting:** when the connection to Steam drops, Reborn notices it
  within a minute and signs in again on its own, with growing pauses
  (5 s up to 5 minutes). If Steam ended the session because the account
  signed in elsewhere, Reborn waits for your next action instead, so two
  sessions do not keep replacing each other (ADR 0025).
- **Chat:** see when a friend is typing, and your friend sees when you
  type. Messages you read in Reborn are marked as read on Steam, so the
  official client and the phone no longer show them as new. Unread counts
  from Steam appear in the friends list (ADR 0026).
- `reborn-cli ticket <app_id>` asks Valve for the app's ownership ticket
  and encrypted app ticket and shows their sizes: groundwork for running
  games without the official client (ADR 0012, ADR 0027).
- **Achievements:** every game in the library has an "Achievements"
  button that shows which ones you have unlocked and when, newest first.
  Hidden ones stay hidden until unlocked. `reborn-cli achievements
  <app_id>` lists them too. Reborn only reads them; it never unlocks or
  changes achievements (ADR 0029, ADR 0030).
- `reborn-cli cloud <app_id>` lists the files a game keeps in your Steam
  Cloud. First step towards syncing saves; nothing is downloaded,
  uploaded or deleted yet (ADR 0031).

### Changed

- The rules for the planned `steamclient` replacement are set: it is built
  without Valve's Steamworks SDK, only from observing games you own
  (ADR 0012, `docs/legal/shim-assessment.md`).

### Fixed

- A network error while signing in with the stored login was reported as
  "login rejected, sign in again". Signing in now also gives up after 90
  seconds instead of hanging.
- When one Steam server kept resetting the connection, every sign-in with
  the stored login failed. Reborn now tries up to three servers, and the
  daemon allows a sign-in 90 seconds (ADR 0028).

## [0.2.0] - 2026-11-20

Friends and chat: see who is online and what they play, set your own
status, answer and send friend requests, and chat with your friends.

### Added

- **Friends** tab in the desktop app and `reborn-cli friends`: your friends
  and pending friend requests with name, online status and current game,
  updated live (ADR 0020).
- **Your own online status** (online, away, busy, invisible, offline) can
  be chosen in the Friends tab; Reborn sets it after every sign-in. Nothing
  changes until you choose one (ADR 0021).
- **Friend requests:** accept incoming requests and send your own by friend
  code, SteamID64 or profile URL, in the Friends tab or with
  `reborn-cli friends add|accept` (ADR 0022). Declining is switched off for
  now: on current Steam the sender could not send a new request afterwards;
  decline in the official client meanwhile (ADR 0023).
- **Chat** with friends: open a conversation from the Friends tab, see the
  recent messages, send and receive messages live; friends with unread
  messages are marked. Also `reborn-cli chat` (ADR 0024).

### Fixed

- Signing in could fail with "Steam did not send the license list in time"
  when Steam bundled the license list with the logon reply.
- After Steam closed the connection (for example when the same account
  signed in elsewhere), friends and chat kept failing until the daemon was
  restarted; now the next action signs in again.

### Known limitations

- Declining friend requests is off for now (see above).
- No group chats, no "is typing" indicator, and messages read in Reborn are
  not marked as read on Steam, so the official client may still show them
  as unread.
- Signing in with the same account in `reborn-cli` while the desktop app
  runs ends the app's connection; its next action signs in again.
- After a lost connection, Reborn signs in again only on the next action,
  so new messages and status changes wait until then.
- Sending friend requests needs an account that Steam allows to (new
  accounts without purchases are refused).

## [0.1.1] - 2026-11-20

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

[0.2.0]: https://github.com/playreborn/reborn/releases/tag/v0.2.0
[0.1.1]: https://github.com/playreborn/reborn/releases/tag/v0.1.1
[0.1.0]: https://github.com/playreborn/reborn/releases/tag/v0.1.0
