# 0012: Shim approach: clean room, observation, daemon first

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

M4 replaces `steamclient`, the library that a game's `steam_api` loads to
reach the Steam client (plan § 8). Research on 2026-11-20 found the
following.

1. **SDK License.** The Steamworks SDK License forbids its licensees to
   "develop software to replace the SDK's functionality" (§ 2.4). It
   binds whoever downloads or uses the SDK (§ 3.1).
2. **Every open source of interface layouts found is SDK material or
   unlicensed:**
   - Proton `lsteamclient/` contains Valve's headers for SDK 099u to 162.
   - `steamworks-sys` bundles 45 SDK headers.
   - gbe_fork has an `sdk/` folder, and it is an emulator used to fake
     ownership.
   - Two OpenSteamworks repositories have no license.
3. **How games find the client (observed):**
   - Linux: `steam_api` tries a local `steamclient.so`, then
     `~/.steam/sdk64/steamclient.so`. Proton's `lsteamclient` loads that
     path too.
   - Windows: Valve's BSD-licensed Proton helper `steam_helper/steam.cpp`
     writes `HKCU\Software\Valve\Steam\ActiveProcess\pid`. Where
     `steam_api` reads the DLL path is not yet confirmed.
4. **Tickets** come from Valve over the CM connection, described in
   node-steam-user `components/appauth.js` (MIT):
   - `ClientGetAppOwnershipTicket` and `ClientRequestEncryptedAppTicket`
     return the tickets.
   - Auth session tickets are registered with `ClientAuthList`, using
     tokens from `ClientGameConnectTokens`.
   - `ClientTicketAuthComplete` reports the result.

The legal assessment is in `docs/legal/shim-assessment.md` (not legal
advice).

## Problem

Under which rules can Reborn build a `steamclient` replacement, and where
does M4 start?

## Alternatives considered

1. **Shim from SDK headers (Proton, steamworks-rs):** fast, but a breach of
   the SDK License for its licensees and a copy of Valve code (red line 6).
   Rejected.
2. **Clean-room shim:** interface details come only from observing games
   the user owns (§ 69d(3) UrhG, which a contract cannot exclude, § 69g(2))
   and from Valve's public documentation. This is defensible, but slow and
   needs `unsafe` expert work.
3. **No shim:** legally the safest, but the project goal of "no official
   client needed" would be out of reach.

## Decision

Option 2, in this order:

1. **Daemon first.** M4 starts with work that needs no shim and is useful
   on its own:
   - ownership and encrypted app tickets, auth session tickets;
   - user stats and achievements (reading and displaying);
   - Steam Cloud.

   These use the CM protocol with MIT references and fixtures, and each
   gets its own ADR.
2. **Clean room for the shim:**
   - nobody downloads or uses the Steamworks SDK;
   - nobody reads SDK-licensed files;
   - no dependency bundles them;
   - contributors who have accepted the SDK License do not work on the
     shim.

   The test game of plan § 8.4 (steamworks-rs) is dropped. Tests use
   real games and recordings instead.
3. **Provenance:** every interface version, slot number and argument the
   shim implements is recorded in `docs/shim/` with its source: an
   observation recording (game, date, what was seen) or a link to the
   public documentation. This mirrors the rule "no protocol behaviour
   without a source". No decompilation; if it ever seems necessary, a new
   ADR decides.
4. **Observation tool:** a logging proxy, run by the user with their own
   game and the official client, records version strings, slot numbers,
   arguments and results. It needs `unsafe` and human review, and gets its
   own ADR.
5. **Running games:**
   - the shim is opt-in per game, with a warning about the account risk;
   - DRM is never detected, answered or worked around (red line 1);
   - games that do not start stay unsupported.
6. **Separation:** the shim and its tools live in their own crates, so
   they could move to a separate repository if a takedown requires it.

## Consequences

- The first M4 releases bring Cloud sync and achievement display, not yet
  games without the official client.
- The shim grows interface by interface from recordings, which is slow,
  but every line has a documented, clean source.
- The remaining risks are listed in the assessment: a DMCA takedown,
  account bans, and an untested question of whether an interface layout
  is protected expression.
- Whether a contributor ever used the SDK cannot be verified. The project
  therefore relies on two things:
  - Each shim PR carries a statement (PR template); a false statement is
    that contributor's breach of their own contract with Valve, not
    Reborn's.
  - The clean source of the code rests on its provenance entries, not on
    trust in people. Each detail can be traced to a recording made by a
    user who never agreed to the SDK License, and a contribution found to
    be tainted is removed and rebuilt from the recordings.
- AI models may have SDK headers in their training data. Therefore memory
  is never a source (AGENTS.md § 4, shim rules); only recordings and
  public documentation are.
