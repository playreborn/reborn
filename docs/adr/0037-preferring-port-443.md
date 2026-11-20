# 0037: Preferring Steam servers on port 443

- **Status:** Accepted
- **Date:** 2026-11-21

## Context

ADR 0036 left open why so many connections to Steam were reset after
about 20 seconds (os error 10054). On 2026-11-21 the picked server of
each attempt was logged (`steam_vent::serverlist` at debug level), ten
attempts in six runs:

- all five attempts on port 27018 or 27019 were reset
  (`cmp1-fra1`, `cmp1-fra2`, `cmp2-fra2`, `cmp2-gru1`);
- all five attempts on other ports worked: port 443 (`cmp1-fra1`,
  `cmp1-fra2`, `cmp2-mad1`), 27021 and 27033;
- the same host, `cmp1-fra1`, failed on 27018 and 27019 and worked on 443.

So something on the network path resets those ports, not the servers.
Port 443 is the usual HTTPS port, which networks rarely block.
`ISteamDirectory/GetCMList` lists WebSocket servers on several ports;
steam-vent shuffles them and hands them out in turn.

## Problem

How Reborn avoids ports that a network resets, without guessing which
ones.

## Alternatives considered

1. **Leave it:** three attempts usually find a working server, but cost
   20 seconds per reset, and sometimes all three fail.
2. **Remember failed ports across runs:** needs a file and more logic.
3. **Try servers on port 443 first:** one line of ordering; every server
   stays in the list.

## Decision

Option 3.

- After discovery, `reborn-cm` keeps steam-vent's random order but moves
  WebSocket servers on port 443 to the front (stable, so the random order
  inside both groups stays). Both token and password logins use it.
- ADR 0028's retries stay as they are.
- No other server is dropped: on a network that resets 443 instead, the
  later attempts still reach the other ports.

## Consequences

- On the observed network, the first attempt should work.
- The observation is from one network; logs from other networks may
  refine this later.
