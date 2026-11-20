# 0027: App tickets from Valve

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

ADR 0012 starts M4 with daemon-side work. Games ask the client for proof
of ownership: an ownership ticket signed by Valve, or an encrypted app
ticket for their own servers. Steam hands both out over the CM connection;
node-steam-user (MIT, `components/appauth.js`) shows how. Tickets contain
the SteamID, the public IP and licenses.

## Problem

How `reborn-cm` gets tickets, and how they are protected.

## Alternatives considered

1. **Wait for the shim:** nothing to test until then.
2. **Fetch them now in `reborn-cm`, with a CLI check and fixtures:** the
   shim can rely on tested code later.

## Decision

Option 2.

- `Session::app_ownership_ticket(app_id)`: `ClientGetAppOwnershipTicket`
  as a job; and `Session::encrypted_app_ticket(app_id, userdata)`:
  `ClientRequestEncryptedAppTicket` as a job. Both check the result, the
  app ID and that a ticket is present, like node-steam-user. Details:
  `docs/protocol/tickets.md`.
- Tickets are a `Ticket` type whose `Debug` prints only the length. They
  are never logged, never stored and never shown, not even by the CLI.
- `reborn-cli ticket <app_id>` prints the sizes; `--record <dir>` writes
  fixtures with the ticket bytes replaced by zeros of the same length and
  CRCs set to zero.
- No daemon or IPC part yet: nothing outside the shim needs tickets. No
  caching yet (node-steam-user caches ownership tickets while more than
  6 hours remain); it comes with the shim, which asks per game start.
- Reborn never builds, changes or forges a ticket (red line 3).

## Consequences

- The shim gets tested ticket code.
- One request per CLI call; nothing runs in the background (red line 8).
