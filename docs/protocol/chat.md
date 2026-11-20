# One-to-one chat

Friend messages over the `FriendMessages` service (ADR 0024). Each section
names its sources. Message text is personal data: never logged, replaced
in fixtures.

### Recent history

- **What:** The last messages of a conversation, oldest to newest after
  sorting by timestamp and ordinal.
- **Method:** `FriendMessages.GetRecentMessages#1`, request
  `CFriendMessages_GetRecentMessages_Request` with `steamid1` (own
  SteamID), `steamid2` (friend), `count`, `most_recent_conversation`
  false, `bbcode_format`, `time_last` (2^31 − 1 for "now"); response
  `messages[]` with `accountid` (sender), `timestamp`, `ordinal`,
  `message`, and `more_available`.
- **Source:** node-steam-user `components/chatroom.js`
  (`getFriendMessageHistory`: these request fields; reads `accountid`,
  `timestamp`, `ordinal`, `message`, `more_available`).
- **Verified:** yes (2026-11-20, second test account and the maintainer's
  main account; fixture `crates/cm/tests/data/chat_history.bin`).

### Sending

- **Method:** `FriendMessages.SendMessage#1` with `steamid`,
  `chat_entry_type` 1, `message`, `contains_bbcode` true; `[` in the text
  is escaped as `\[`. Response: `server_timestamp`, `ordinal`,
  `modified_message`, `message_without_bb_code`.
- **Values:** `EChatEntryType`: ChatMsg 1, Typing 2, LeftConversation 6.
- **Source:** node-steam-user `components/chatroom.js`
  (`sendFriendMessage`: these fields, BBCode on by default with `[`
  escaped); SteamKit2 `enums.steamd` (`EChatEntryType`).
- **Verified:** yes (2026-11-20: messages sent from Reborn arrived in the
  official client).

### Receiving

- **Notification:** `FriendMessagesClient.IncomingMessage#1`,
  `CFriendMessages_IncomingMessage_Notification` with `steamid_friend`,
  `chat_entry_type`, `message`, `message_no_bbcode`,
  `rtime32_server_timestamp`, `ordinal`, `local_echo`.
- **Meaning:** type 1 is a message, 2 typing, 6 left the conversation.
  `local_echo` is the user's own message sent from another client.
  `message_no_bbcode` falls back to `message` when missing.
- **Source:** node-steam-user `components/chatroom.js` (handler for
  `FriendMessagesClient.IncomingMessage#1`: these fields, the routing by
  `chat_entry_type`, `local_echo` as an echo event).
- **Verified:** yes (2026-11-20: replies arrived live, a typing notice came
  first; fixtures `chat_incoming_typing.bin`, `chat_incoming_message.bin`).

### Typing

- **Sending:** `FriendMessages.SendMessage#1` with `steamid`,
  `chat_entry_type` 2 (Typing), `message` empty, `contains_bbcode` true.
  Reborn sends at most one per friend every 10 seconds (ADR 0026).
- **Receiving:** `FriendMessagesClient.IncomingMessage#1` with type 2 and
  an empty message; `local_echo` is the user typing in another client.
  There is no "stopped typing".
- **Source:** node-steam-user `components/chatroom.js` (`sendFriendTyping`
  calls `sendFriendMessage` with `chatEntryType` Typing and `''`; the
  incoming handler maps type 2 to `friendTyping`, no expiry).
- **Verified:** yes (2026-11-20, both test accounts: typing in Reborn
  showed in the official client and the other way round; no flicker
  during 30 seconds of typing. In a recording, the official client sent a
  notice when typing started and again after each sent message).
  Fixture `chat_incoming_typing.bin`.

### Read markers

- **Method:** `FriendMessages.AckMessage#1`,
  `CFriendMessages_AckMessage_Notification` with `steamid_partner` (the
  friend) and `timestamp` (server time of the newest message read). Sent
  as `ServiceMethodCallFromClient` with the method as `target_job_name`
  and no source job; Steam does not answer.
- **Source:** node-steam-user `components/chatroom.js`
  (`ackFriendMessage`: these fields, `_sendUnified` without callback) and
  `components/03-messages.js` (`_sendUnified`: without a callback no
  source job id).
- **Verified:** yes (2026-11-20: after opening the conversation in Reborn,
  the Steam web chat no longer showed it as unread after a reload; the
  same for a message received while the conversation was open). No
  fixture: Steam sends no answer.

### Unread counts

- **Method:** `FriendMessages.GetActiveMessageSessions#1`, request without
  fields; response `message_sessions[]` with `accountid_friend`,
  `last_message`, `last_view`, `unread_message_count`, and `timestamp`.
  The friend's SteamID64 is the individual account ID in the public
  universe.
- **Source:** node-steam-user `components/chatroom.js`
  (`getActiveFriendMessageSessions`: `lastmessage_since` only when asked
  for, these response fields, `SteamID.fromIndividualAccountID`).
- **Verified:** yes (2026-11-20: three unread messages showed as "3 new"
  after starting Reborn; fixture `chat_sessions.bin`, where `last_view` is
  0 for a conversation never viewed).

### Sessions of the same account

- Signing in with the same account in a second Reborn process (the CLI
  while the daemon ran) closed the daemon's connection: the next request
  failed with "Trying to work with closed connection" (live, 2026-11-20).
  The official client running at the same time did not do this. Reborn
  treats such errors as a lost connection: the session is dropped and the
  next request signs in again; reading the history is retried once,
  sending is not (Steam may already have the message).

