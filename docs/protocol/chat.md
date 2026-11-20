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

### Sessions of the same account

- Signing in with the same account in a second Reborn process (the CLI
  while the daemon ran) closed the daemon's connection: the next request
  failed with "Trying to work with closed connection" (live, 2026-11-20).
  The official client running at the same time did not do this. Reborn
  treats such errors as a lost connection: the session is dropped and the
  next request signs in again; reading the history is retried once,
  sending is not (Steam may already have the message).

