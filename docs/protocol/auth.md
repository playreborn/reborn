# Authentication

Implemented through steam-vent (ADR 0007). Sources refer to steam-vent
v0.5.0 unless noted.

### CM server discovery

- **What:** Fetch the list of CM servers before connecting.
- **Message / method:** Steam Directory Web API (`ServerList::discover`).
- **Source:** steam-vent `src/serverlist.rs`
- **Verified:** yes (2026-11-19, live with a test account; Reborn commit after `5e8b4c6`)

### Password login

- **What:** Encrypt the password with Steam's RSA key and start an auth
  session.
- **Message / method:** `Authentication.GetPasswordRSAPublicKey#1`, then
  `Authentication.BeginAuthSessionViaCredentials#1`
  (sent as `k_EMsgServiceMethodCallFromClientNonAuthed`).
- **Source:** steam-vent `src/auth/mod.rs` (`begin_password_auth`, `get_password_rsa`)
- **Verified:** yes (2026-11-19, live with a test account; Reborn commit after `5e8b4c6`)

### Steam Guard confirmation

- **What:** Submit an e-mail/TOTP code, or wait for approval in the Steam
  mobile app. Live-verified so far: approval in the mobile app.
- **Message / method:** `Authentication.UpdateAuthSessionWithSteamGuardCode#1`;
  device confirmation needs no request from the client.
- **Source:** steam-vent `src/auth/mod.rs` (`submit_confirmation`),
  `src/auth/confirmation.rs`
- **Verified:** yes (2026-11-19, live with a test account; Reborn commit after `5e8b4c6`)
- **From a frontend (ADR 0014):** `reborn-cm`'s `GuardPrompt` reports the
  allowed confirmations and waits for the code from the daemon; the code
  goes through steam-vent's `UserProvidedAuthConfirmationHandler`, so the
  request on the wire is the same as for a code typed into the CLI.
  steam-vent races the handler against token polling
  (`src/connection/unauthenticated.rs`, `Connection::login`), so approval in
  the mobile app still ends the login while the code prompt is open.
  Sign-in through the desktop app live-verified 2026-11-20 with a test account
  (approval in the mobile app; the code path is covered by unit tests only).

### Token polling

- **What:** Poll until Steam returns the refresh token (and optional new
  guard data).
- **Message / method:** `Authentication.PollAuthSessionStatus#1`, at the
  interval returned by the begin call.
- **Source:** steam-vent `src/auth/mod.rs` (`PendingAuth::wait_for_tokens`)
- **Verified:** yes (2026-11-19, live with a test account; Reborn commit after `5e8b4c6`)

### Logon

- **What:** Log on to the CM with the token. After a password login the
  **refresh token** is sent in the `access_token` field.
- **Message / method:** `CMsgClientLogon` (`k_EMsgClientLogon`),
  `protocol_version` 65580.
- **Source:** steam-vent `src/session.rs` (`login`),
  `src/connection/unauthenticated.rs` (comment: "yes we send the refresh token
  as access token ... this is actually required")
- **Verified:** yes (2026-11-19, live with a test account; Reborn commit after `5e8b4c6`)

### Token login

- **What:** Log on with a stored token, without password. The token is a
  JWT; steam-vent checks `iss == "steam"` and takes the SteamID from `sub`.
- **Message / method:** `CMsgClientLogon` with the stored token.
- **Source:** steam-vent `src/connection/unauthenticated.rs` (`access`)
- **Verified:** yes (2026-11-19, live with a test account; Reborn commit after `5e8b4c6`)

### QR login

- **What:** Login by scanning a QR code with the Steam mobile app.
- **Message / method:** `Authentication.BeginAuthSessionViaQR#1` (message
  exists in `steam-vent-proto-steam`, `steammessages_auth_steamclient`).
- **Source:** not implemented in steam-vent ("QR Authentication" unchecked in
  its README); needs an upstream contribution.
- **Verified:** no

### Logoff and reconnect

- **What:** Steam logs a session off with `CMsgClientLoggedOff` (EMsg 757,
  `eresult`). A socket can also close without that message. Reborn checks
  every 60 seconds by sending the empty `CMsgClientHeartBeat` (EMsg 703)
  steam-vent's heartbeat sends; on a closed socket the send fails at once,
  locally. It signs in again with backoff (5 s, 15 s, 30 s, 1 min, 2 min,
  then every 5 minutes) only for logoff results 0, 2, 3, 20, 48 or a
  closed socket (ADR 0025).
- **Source:** node-steam-user `components/09-logon.js` (`ClientLoggedOff`
  handler reads `eresult`; `nonFatalLogOffResults` = 0, Fail,
  NoConnection, ServiceUnavailable, TryAnotherCM; other results are fatal
  and not relogged); steam-vent `src/connection/raw.rs`
  (`setup_heartbeat` sends `CMsgClientHeartBeat::default()`) and
  `src/eresult.rs` (numbers).
- **Verified:** yes for a closed socket (2026-11-20, test account, Wi-Fi
  switched off and on): steam-vent logged the reset, Reborn's probe noticed
  it within the minute, and the first sign-in attempt after 5 seconds
  succeeded. A logoff with an EResult (for example a replaced session) has
  not been seen live yet.

### Choosing a server

- **What:** A token login tries up to three CM servers when connecting
  fails before logon (reset, closed, timeout), a second apart.
- **Why:** steam-vent's `ServerList` hands out servers in turn, starting
  with the first of `ISteamDirectory/GetCMList` in each process
  (`src/serverlist.rs`, `pick_ws`). On 2026-11-20 one server reset every
  connection (os error 10054), so every new CLI process failed.
- **Source:** node-steam-user `components/09-logon.js` picks at random
  among the five least loaded servers and skips recently failed ones;
  Reborn moves to the next server instead.
- **Verified:** yes (2026-11-20: several servers reset connections after
  about 20 seconds; the second or third server worked in three of four
  runs, and one run failed on all three).
