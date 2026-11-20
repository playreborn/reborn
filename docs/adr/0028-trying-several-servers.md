# 0028: Trying several Steam servers when signing in

- **Status:** Accepted
- **Date:** 2026-11-20
- **Amends:** ADR 0025 (the 60-second sign-in limit becomes 90 seconds)

## Context

On 2026-11-20 several Steam servers reset every new connection after
about 20 seconds (os error 10054). steam-vent's `ServerList` hands out
servers in turn, starting with the first entry of
`ISteamDirectory/GetCMList` in every process, so each new CLI process
reached the same failing server. node-steam-user avoids this: it picks at
random among the five least loaded servers and skips recently failed ones
(`components/09-logon.js`).

## Problem

How Reborn gets past a failing server, and how that fits the sign-in
limit of ADR 0025.

## Alternatives considered

1. **Random server like node-steam-user:** needs a random number source
   (a new dependency) and our own copy of the server list handling.
2. **Next server on failure:** the server list already rotates, so trying
   again with the same list reaches the next server.

## Decision

Option 2.

- A token login tries up to three servers, a second apart, when the
  connection fails before logon (reset, closed or timeout). Login errors
  are never retried. Password logins are not repeated either, because
  they may ask for Steam Guard again.
- `rebornd` allows a sign-in 90 seconds instead of 60 (ADR 0025), room for
  three servers that take about 20 seconds each to fail.

## Consequences

- One failing server no longer blocks sign-ins: in the live test, the
  second or third server worked in three of four runs.
- At most three connection attempts per sign-in, so Valve's servers are
  not hammered (red line 8).
- If many servers fail at once, signing in can still fail after about a
  minute; the reconnect backoff of ADR 0025 then tries again later.
