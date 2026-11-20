# 0024: One-to-one chat with friends

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

M3 stage 4, the last part before Release 0.2. Modern Steam carries
one-to-one messages over service methods of the `FriendMessages` service;
node-steam-user (MIT, `components/chatroom.js`) uses them, and the
generated protobufs ship with steam-vent-proto-steam. Messages are
personal data of the user and of their friends. Sending acts on the
account and is visible to the friend (AGENTS.md § 7).

## Problem

Which parts of chat Reborn supports first, and how message text is
protected.

## Alternatives considered

1. **Group chats and one-to-one at once:** group chats use a separate,
   larger chat room protocol.
2. **One-to-one first:** read the recent history, receive and send
   messages; group chats, typing indicators, read markers and reactions
   later.

## Decision

Option 2.

- History: `FriendMessages.GetRecentMessages#1` with `steamid1` (own),
  `steamid2` (friend), `count`, `most_recent_conversation` false,
  `bbcode_format` false (plain text) and `time_last` 2^31 − 1, as
  node-steam-user's `getFriendMessageHistory` does (except that it asks
  for BBCode).
- Sending: `FriendMessages.SendMessage#1` with `steamid`,
  `chat_entry_type` 1 (`EChatEntryType::ChatMsg`, SteamKit2 `enums.steamd`),
  `message` and `contains_bbcode` true, with `[` escaped as `\[`, like
  node-steam-user's `sendFriendMessage`.
- Receiving: the notification `FriendMessagesClient.IncomingMessage#1`;
  type 1 is a message, others (typing, left) are ignored for now.
  `local_echo` marks the user's own messages sent from another client.
- Message text never appears in logs. Fixtures replace it with
  placeholders, like names and SteamIDs (ADR 0020).
- IPC protocol 1.8 (compatible additions): `chat_history { steam_id }`
  answers `chat_messages`; `send_chat_message { steam_id, text }` answers
  `chat_message_sent`; event `chat_message`.
- The desktop app opens a conversation from the Friends tab and marks
  friends with unread messages; the CLI gets `chat`.

## Consequences

- Reborn does not mark messages as read on Steam yet, so the official
  client may still show them as unread.
- Messages are sent only when the user sends them; nothing is automated
  (red line 5).
