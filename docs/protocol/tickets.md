# App tickets

Proof from Valve that the account owns an app (ADR 0027). Needed by the
shim later (ADR 0012); Reborn never builds or changes a ticket (red line
3). Tickets carry the SteamID, the public IP and licenses: never logged,
replaced in fixtures.

### Ownership ticket

- **What:** A ticket signed by Valve that the account owns an app.
- **Message:** `CMsgClientGetAppOwnershipTicket` (EMsg 857) with `app_id`,
  sent as a job; answer `CMsgClientGetAppOwnershipTicketResponse` (EMsg
  858) with `eresult`, `app_id`, `ticket`.
- **Checks:** `eresult` 1 (OK), `app_id` equals the request, ticket
  present.
- **Source:** node-steam-user `components/appauth.js`
  (`getAppOwnershipTicket`: these fields and checks; it also caches
  tickets on disk while more than 6 hours remain, which Reborn does not do
  yet). EMsg numbers from steam-vent-proto-steam `enums_clientserver.rs`.
- **Verified:** yes (2026-11-20, first test account: 178-byte ticket for
  an owned app, EResult 15 AccessDenied for an app not owned; fixtures
  `ownership_ticket.bin`, `ownership_ticket_not_owned.bin`).

### Encrypted app ticket

- **What:** The ownership ticket encrypted with the game's key, plus the
  game's own `userdata`, for the game's servers to check. Only for apps
  that set up a key with Valve.
- **Message:** `CMsgClientRequestEncryptedAppTicket` (EMsg 5526) with
  `app_id` and `userdata`, sent as a job; answer
  `CMsgClientRequestEncryptedAppTicketResponse` (EMsg 5527) with
  `app_id`, `eresult` and `encrypted_app_ticket` (`EncryptedAppTicket`:
  `ticket_version_no`, `crc_encryptedticket`, `cb_encrypteduserdata`,
  `cb_encrypted_appownershipticket`, `encrypted_ticket`).
- **Checks:** `eresult` 1, `app_id` equals the request, ticket present.
  Games receive the `EncryptedAppTicket` message protobuf-encoded.
- **Source:** node-steam-user `components/appauth.js`
  (`createEncryptedAppTicket`: these fields and checks, `userdata`
  defaults to empty, the result is encoded as `EncryptedAppTicket`).
- **Verified:** yes (2026-11-20, first test account): app 480 (Spacewar)
  returned a ticket (version 2, 128 encrypted bytes, 78 of them the
  ownership ticket); EResult 2 Fail for an owned app without an encryption
  key; EResult 15 for an app not owned. Fixtures `encrypted_app_ticket.bin`,
  `encrypted_app_ticket_no_key.bin`, `encrypted_app_ticket_not_owned.bin`.
  Steam gave tickets for app 480 although it is not in the account's
  license list; the entitlement comes from Steam, not from Reborn.

### Not yet

- Auth session tickets (`ClientGameConnectTokens`, `ClientAuthList`,
  `ClientTicketAuthComplete`): documented in the same node-steam-user file;
  they need the shim's game sessions, so they come with the shim.
- The ticket's inner format (node `steam-appticket`): not needed until the
  shim hands tickets to games.
