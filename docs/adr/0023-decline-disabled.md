# 0023: Declining friend requests disabled until verified

- **Status:** Accepted
- **Date:** 2026-11-20
- **Amends:** ADR 0022 (the decline part)

## Context

ADR 0022 declines a request with `CMsgClientRemoveFriend`, as
node-steam-user documents it ("... or decline an invitation"). In the live
tests on 2026-11-20 (two test accounts, the user's main account sending):

- The request disappeared, as intended.
- Steam then sent an incremental friends list that Reborn showed as
  "Blocked" (it grouped the values Blocked 1, Ignored 5 and IgnoredFriend 6
  under that label).
- The sender's attempts to send a new request failed ("Fehler beim
  Hinzufügen des Freundes") for at least the rest of the session.
- The test account's block list on the Steam website did not contain the
  sender, and after a new login Steam reported no relationship at all.

So no lasting block was created, but the exact meaning of the change and
whether the official client's "Ignore" behaves the same is not verified.
Older statements (node-steam-user's author: an ignored request is "fully
deleted from Steam") predate Steam's 2018 friends system.

## Problem

Whether to ship a decline whose effect on the other person is not fully
understood.

## Alternatives considered

1. **Keep it:** it does what the reference library does, but users would
   be surprised if the sender cannot send again.
2. **Disable it until compared with the official client:** accept and send
   stay; declining is done in the official client meanwhile.

## Decision

Option 2.

- `rebornd` refuses `decline_friend` with an explanation, the CLI refuses
  `friends decline`, and the desktop app shows no Decline button. The IPC
  request stays in the protocol so frontends need no change later.
- The relationship `Ignored` gets its own name (`ignored`, "Request
  ignored") instead of "Blocked", and `rebornd` logs every relationship
  change with its exact value.

## Consequences

- Next step: ignore a request in the official client, compare the change
  Steam sends (value in the daemon log) and whether the sender can send
  again; if both match, enable declining with that explanation.
