# 0022: Friend requests: send, accept, decline

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

M3 stage 3. Friends are visible (ADR 0020) and the own status can be set
(ADR 0021). Users need to answer incoming friend requests and send their
own. Both act on the account and are visible to other people (AGENTS.md
§ 7). Steam reports every relationship change with an incremental
`CMsgClientFriendsList`, which Reborn did not read yet.

## Problem

Which messages Reborn sends, how users name the person to add, and how
changes reach the frontends.

## Alternatives considered

1. **Accept and decline only:** smallest change, but users asked for
   sending requests too.
2. **Send, accept and decline, with SteamID input only:** complete, but
   few users know their friends' SteamID64.
3. **Send, accept and decline; input as SteamID64, profile URL or friend
   code:** covers what the official client shows to users.

## Decision

Option 3.

- Send and accept: `CMsgClientAddFriend` (EMsg 791) with `steamid_to_add`,
  sent as a job; the answer `CMsgClientAddFriendResponse` carries
  `eresult` and `persona_name_added`. Decline: `CMsgClientRemoveFriend`
  (EMsg 714) with `friendid`, no answer. Source: node-steam-user
  `components/friends.js` (`addFriend`: "Send (or accept) a friend
  invitation"; `removeFriend`: "... (or decline an invitation)").
  `accountname_or_email_to_add` is never used.
- Input: a SteamID64, a profile URL `…/profiles/<SteamID64>`, or a friend
  code. A friend code is taken as the 32-bit account ID of an individual
  account in the public universe (SteamID64 = 76561197960265728 + code;
  bit layout from SteamKit2 `Types/SteamID.cs`), verified live. Custom
  profile URLs (`/id/<name>`) need the Web API and come later.
- `rebornd` follows incremental friends lists for the whole session:
  changed entries are updated and their persona requested, ended ones
  removed. IPC protocol 1.7 (compatible additions): `add_friend { friend }`,
  `accept_friend { steam_id }`, `decline_friend { steam_id }`; event
  `friend_removed { steam_id }`. Accept and decline are refused unless the
  person actually sent a request.
- The desktop app shows Accept and Decline on incoming requests and a
  field to send a request; the CLI gets `friends add|accept|decline`.

## Consequences

- Removing existing friends and cancelling sent requests are not offered
  yet; that the same message also cancels a sent request is not verified.
- Steam may refuse requests (limits, privacy); the EResult is shown.
