# 0025: Reconnecting after a lost connection

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

Until now `rebornd` noticed a lost Steam connection only when the next
request failed, and then signed in again for that request. Meanwhile no
messages or status changes arrived. steam-vent has no "connection closed"
signal: its heartbeat task only logs send errors (`src/connection/raw.rs`,
`setup_heartbeat`), and its message streams stay open after the socket
closed. Steam announces some logoffs with `CMsgClientLoggedOff` (EMsg 757,
`eresult`).

Reconnecting has a risk: if the session ended because the same account
signed in elsewhere, two clients that both reconnect would keep replacing
each other, an endless loop against Valve's servers (red line 8).

## Problem

How `rebornd` detects a lost connection, when it signs in again on its
own, and how often.

## Alternatives considered

1. **Keep reconnecting on the next request only:** simple, but the app is
   silently offline in between.
2. **Detect and reconnect with backoff, except when replaced:** follows
   node-steam-user's rules.

## Decision

Option 2.

- Detection: `CMsgClientLoggedOff` (its `eresult`), and a probe every 60
  seconds that sends the same empty `CMsgClientHeartBeat` steam-vent's own
  heartbeat sends; on a closed socket it fails at once and locally.
- Reconnect only for logoff results 0, Fail (2), NoConnection (3),
  ServiceUnavailable (20), TryAnotherCM (48), and for a closed socket
  without a logoff message. Every other result, in particular
  LoggedInElsewhere (6) and LogonSessionReplaced (34), ends the session
  until the user's next action. Source: node-steam-user
  `components/09-logon.js` (`nonFatalLogOffResults`; other results are
  fatal and not relogged); numbers from steam-vent `src/eresult.rs`.
- Backoff: 5 s, 15 s, 30 s, 1 min, 2 min, then every 5 minutes, gentler
  than node-steam-user's 1 to 60 seconds. Reconnecting stops when the user
  signs out, a request signed in already, or Steam refuses the login
  itself. Network errors while signing in are not a refused login: a token
  login used to report every error as "login rejected", which ended the
  attempts during a short outage (live test 2026-11-20).
- Every sign-in by `rebornd` has a 60-second limit, so a sign-in caught by
  a network outage cannot hold the session lock and block all requests.
- IPC protocol 1.9 (compatible addition): event `steam_connection {
  connected, reconnecting, reason }`. The desktop app shows the state and
  reloads friends after a reconnect.

## Consequences

- At most one probe per minute while signed in, plus the sign-in attempts
  of the backoff.
- When another session took over, Reborn stays offline until the user
  acts; that action may in turn end the other session.
