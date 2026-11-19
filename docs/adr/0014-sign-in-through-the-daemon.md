# 0014: Sign-in through the daemon

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

Until now an account is signed in with `reborn-cli login`, which talks to
Steam itself and stores the refresh token in the OS keyring (ADR 0010).
The desktop app is a thin frontend on `rebornd` (ADR 0002) and may not use
`reborn-cm` or `reborn-core` directly. To sign in from the app, the
password and the Steam Guard code have to reach the daemon.

ADR 0008 states that no secrets travel over IPC and lists a gap: on
Windows the named pipe uses the default security descriptor, which lets
other local users open it for reading. A password over IPC needs that gap
closed first.

Steam Guard is interactive: after the password, Steam may ask for a code
(email or authenticator app) or wait for approval in the Steam mobile app,
or both at once. steam-vent drives this through its
`AuthConfirmationHandler` trait (`src/auth/confirmation.rs`) and races the
handler against polling for tokens (`src/connection/unauthenticated.rs`,
`Connection::login`, v0.5.0), so approval in the mobile app wins even
while a code prompt is open. steam-vent's `SteamGuardToken` cannot be built
outside the crate; its public `UserProvidedAuthConfirmationHandler::new`
reads a code from any `AsyncRead`.

## Problem

How does a frontend sign in through the daemon, including Steam Guard,
without exposing the password to other local users?

## Alternatives considered

1. **Frontends sign in themselves** (as the CLI does): breaks ADR 0002;
   every frontend would need `reborn-core` and keyring access.
2. **Daemon opens its own login window:** the daemon has no UI and may run
   without a display.
3. **Sign-in requests over IPC, after restricting the Windows pipe to the
   current user:** one place handles tokens and the keyring; frontends only
   collect input.

## Decision

Option 3.

- **IPC (protocol 1.1, compatible additions):**
  - `sign_in { account, password }` answers `signed_in { account }` once
    Steam accepted the login, or an error. A new `sign_in` cancels one
    that is still waiting.
  - While it waits, the daemon sends the event `steam_guard { code,
    confirm_in_app, hint }`: `code` is `email`, `authenticator` or absent;
    `confirm_in_app` says whether approval in the mobile app works.
  - `submit_guard_code { code }` answers that prompt; `cancel_sign_in`
    gives up.
  - `sign_out` removes the stored login of the current account (refused
    while downloads run). The events `signed_in` / `signed_out` tell every
    frontend.
  - `status` gains `stored_login` (whether a refresh token is stored).
  - Passwords and codes use a redacting `Secret` type in `reborn-ipc`
    (`Debug` prints `<redacted>`). The daemon never logs requests.
  - Requests on one connection are handled concurrently, so a frontend
    can answer the Steam Guard prompt while `sign_in` is pending.
- **`reborn-cm`:** a `GuardPrompt` confirmation handler. It reports a
  `GuardChallenge` through a channel and waits for the code on a second
  one; the code is fed to steam-vent's `UserProvidedAuthConfirmationHandler`
  through an in-memory reader, which builds the token. If only app
  approval is possible it returns "no action" and steam-vent keeps
  polling. This uses steam-vent's public API only and adds no wire
  behaviour, so no new fixture is needed; unit tests cover the handler.
- **Windows pipe:** the daemon creates the named pipe with the DACL
  `D:P(A;;GA;;;<user SID>)`, so only the current user can open it. The SID
  comes from the process token (`reborn-platform`, the only crate allowed
  to contain `unsafe`). This closes the gap from ADR 0008. Linux keeps the
  socket file with mode `0600` in `$XDG_RUNTIME_DIR`.

## Consequences

- The desktop app can sign in and out; the CLI keeps its own `login` for
  scripting and debugging.
- A wrong Steam Guard code ends the attempt; the user enters the password
  again. Retrying inside one Steam auth session can come later.
- Residual risk on Windows: another local user could create a pipe with
  our name before the daemon starts (pipe squatting). The daemon then
  fails to start (`FILE_FLAG_FIRST_PIPE_INSTANCE`), but a frontend would
  connect to the impostor. Follow-up: frontends check that the pipe's
  server process belongs to the same user before sending secrets. On a
  single-user machine this does not apply.
- QR-code sign-in will be another request with an event carrying the
  challenge URL, once steam-vent supports it.
- Login and tokens require human review (AGENTS.md § 7).
