# 0036: Steam servers are already chosen at random

- **Status:** Accepted
- **Date:** 2026-11-21
- **Amends:** ADR 0028 (its context)

## Context

ADR 0028 says steam-vent's `ServerList` starts with the first entry of
`ISteamDirectory/GetCMList` in every process, so each new process
reached the same failing server. That is wrong: steam-vent 0.5 shuffles
both server lists at random when it builds the list from Steam's answer
(`src/serverlist.rs`, `TryFrom<ServerListResponse>`), and only hands them
out in turn within one process.

Connection resets after about 20 seconds (os error 10054) kept happening
on 2026-11-21, on randomly chosen servers.

## Problem

Whether Reborn should choose servers itself, as planned after ADR 0028
(at random, like node-steam-user `components/09-logon.js`).

## Alternatives considered

1. **Own random choice:** changes nothing; the choice is random already.
2. **Keep ADR 0028's retries and find out which servers fail first.**

## Decision

Option 2. ADR 0028's decision (up to three servers, 90 seconds) stays;
only its explanation was wrong. Before any further change, the failing
servers are logged (`steam_vent::serverlist` at debug level shows each
pick) to see what they have in common.

## Consequences

- No code change now.
- `docs/protocol/auth.md` is corrected.
