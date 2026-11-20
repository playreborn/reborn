# 0026: Typing notices, read markers and unread counts

- **Status:** Accepted
- **Date:** 2026-11-20

## Context

ADR 0024 left out typing notices and read markers. Without read markers,
messages read in Reborn stay unread in the official client and on the
phone. Steam carries all three over the `FriendMessages` service, and
node-steam-user (MIT, `components/chatroom.js`) uses them. Typing notices
are visible to the friend, and a read marker changes the account's state
(AGENTS.md § 7). Both only happen in response to what the user does.

## Problem

How Reborn sends and shows typing notices, when it marks messages as read,
and where unread counts come from.

## Alternatives considered

1. **Only show the friend's typing:** simpler, but the friend never sees
   that the user is typing, and unread counts stay out of sync.
2. **Typing both ways, read markers and unread counts from Steam:** the
   behaviour of the official client.

## Decision

Option 2.

- Typing (sending): `FriendMessages.SendMessage#1` with
  `chat_entry_type` 2 (`EChatEntryType::Typing`) and an empty message, like
  node-steam-user's `sendFriendTyping`. It is sent only while the user
  types in an open conversation, at most once every 10 seconds per friend.
  The 10 seconds are Reborn's own limit; node-steam-user sets none.
- Typing (receiving): `FriendMessagesClient.IncomingMessage#1` with type 2
  becomes the event `friend_typing`. The user's own typing in another
  client (`local_echo`) is ignored. Steam sends no "stopped typing", and
  node-steam-user has no expiry either. The desktop app shows the notice
  for 15 seconds after the last one, or until the friend's message
  arrives.
- Read markers: `FriendMessages.AckMessage#1` with `steamid_partner` and
  `timestamp` (the newest message's server time), sent as a notification
  without a job, like node-steam-user's `ackFriendMessage` (`_sendUnified`
  without a callback, `components/03-messages.js`). steam-vent's own
  service-method wrapper always waits for an answer, so `reborn-cm` sends
  it through a small wrapper that sets the target job name and no source
  job. The desktop app sends one when a conversation is opened and when a
  message arrives in the open conversation.
- Unread counts: `FriendMessages.GetActiveMessageSessions#1` without
  fields, reading `accountid_friend` and `unread_message_count`, as
  node-steam-user's `getActiveFriendMessageSessions`. They are loaded with
  the friends list.
- Public API of `reborn-cm`: `Session::chat_messages` is replaced by
  `chat_events` (messages and typing). New: `send_typing`, `mark_read`,
  `unread_chats`, `ChatEvent`, `UnreadChat`, `chat_event_from`,
  `unread_from`.
- IPC protocol 1.10 (compatible additions): requests `send_typing
  { steam_id }`, `mark_chat_read { steam_id, timestamp }` and
  `unread_chats` (answer `unread_chats { chats }`); event `friend_typing
  { steam_id }`.

## Consequences

- Messages read in Reborn are read everywhere.
- At most one typing notice per friend every 10 seconds, and one read
  marker per opened conversation or received message. Nothing is sent
  without user activity (red lines 5 and 8).
- If the official client sends typing notices less often than every 15
  seconds, the notice may flicker during long replies. The live test shows
  how often they come.
