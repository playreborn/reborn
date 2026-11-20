# 0020: Friends and presence, read-only first

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

Milestone M3 (Release 0.2) brings friends, presence and chat. The CM
protocol messages are known from node-steam-user (MIT,
`components/friends.js`) and SteamKit2 (LGPL, read only), and their
protobufs ship with steam-vent-proto-steam. Anything that acts on the
account (setting the own status, sending messages, accepting or removing
friends) needs human review (AGENTS.md § 7) and is visible to other people.

## Problem

In which order to build social features, and how friend data reaches the
frontends.

## Alternatives considered

1. **Everything at once** (list, status, chat, requests): a large change
   that touches the account in many ways before the basics are verified.
2. **Read-only first:** friends list with names, status and current game.
   The only message sent is the request for friend data; the user's own
   status is not changed. Writing features follow one by one.

## Decision

Option 2.

- `reborn-cm` reads `CMsgClientFriendsList` (sent by Steam after logon; it
  can arrive while logging in, so it is taken from the messages steam-vent
  kept unprocessed, like the license list) and requests persona data for
  every friend with `CMsgClientRequestFriendData` (flags: status, player
  name, presence, last seen, game extra info). `CMsgClientPersonaState`
  updates arrive as a stream. Clans and other non-individual SteamIDs are
  skipped.
- A recording helper writes the friends list and persona messages as
  fixtures with every SteamID replaced by a placeholder in the dev universe
  (never a real account) and names replaced; only the fields Reborn uses
  are kept, so avatars, IPs and rich presence never reach a fixture.
- `rebornd` keeps the friends and their latest persona once signed in and
  forwards changes. IPC protocol 1.5 (compatible additions): request
  `friends` answers `friends { friends }`; event `friend_updated`.
- The desktop app gets a Friends tab; the CLI `friends`.

## Consequences

- Reborn shows friends but does not appear online itself; the official
  client, if running, still decides the user's status.
- Changing the own status, chat and friend requests come next, each with
  its own review.
- Friend SteamIDs travel over the local IPC to the user's own frontends
  only.
