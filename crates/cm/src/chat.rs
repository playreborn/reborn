//! One-to-one chat with friends (ADR 0024).
//!
//! Methods and fields, see `docs/protocol/chat.md`; source: node-steam-user
//! `components/chatroom.js` (MIT), values from SteamKit2 `enums.steamd`.
//! Message text is personal data: this module never logs it.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use steam_vent::ConnectionTrait;
use steam_vent_proto_common::protobuf::Message;
use steam_vent_proto_steam::steammessages_friendmessages_steamclient::{
    CFriendMessages_GetRecentMessages_Request, CFriendMessages_GetRecentMessages_Response,
    CFriendMessages_IncomingMessage_Notification, CFriendMessages_SendMessage_Request,
    cfriend_messages_get_recent_messages_response::FriendMessage,
};

use crate::{Error, Session};

/// `EChatEntryType::ChatMsg`.
const CHAT_MSG: i32 = 1;

/// "No upper bound" for `time_last` in history requests (node-steam-user
/// `getFriendMessageHistory` default, 2^31 − 1).
const TIME_LAST_NOW: u32 = 0x7FFF_FFFF;

/// The low 32 bits of a SteamID64: the account ID.
fn account_id(steam_id: u64) -> u32 {
    (steam_id & 0xFFFF_FFFF) as u32
}

/// A message of a one-to-one conversation.
#[derive(Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// The friend the conversation is with (SteamID64).
    pub friend: u64,
    /// Whether the user wrote it (in Reborn or another client).
    pub from_me: bool,
    /// The text, without BBCode.
    pub text: String,
    /// Server time, seconds since 1970.
    pub timestamp: u32,
    /// Orders messages with the same timestamp.
    pub ordinal: u32,
}

impl std::fmt::Debug for ChatMessage {
    /// Leaves the text out, so it cannot end up in logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatMessage")
            .field("friend", &self.friend)
            .field("from_me", &self.from_me)
            .field(
                "text",
                &format_args!("<{} chars>", self.text.chars().count()),
            )
            .field("timestamp", &self.timestamp)
            .field("ordinal", &self.ordinal)
            .finish()
    }
}

/// Redacted fixture data, see [`Session::record_chat`].
#[derive(Clone, Debug, Default)]
pub struct ChatRecording {
    /// `CFriendMessages_GetRecentMessages_Response`, protobuf-encoded.
    pub history: Vec<u8>,
    /// Every `CFriendMessages_IncomingMessage_Notification` received.
    pub incoming: Vec<Vec<u8>>,
}

impl Session {
    /// The last `count` messages with a friend, oldest first.
    pub async fn recent_messages(
        &self,
        friend: u64,
        count: u32,
    ) -> Result<Vec<ChatMessage>, Error> {
        let response = self.recent_messages_response(friend, count).await?;
        Ok(history_from(&response, friend, account_id(self.steam_id())))
    }

    async fn recent_messages_response(
        &self,
        friend: u64,
        count: u32,
    ) -> Result<CFriendMessages_GetRecentMessages_Response, Error> {
        let request = CFriendMessages_GetRecentMessages_Request {
            steamid1: Some(self.steam_id()),
            steamid2: Some(friend),
            count: Some(count),
            most_recent_conversation: Some(false),
            bbcode_format: Some(false),
            time_last: Some(TIME_LAST_NOW),
            ..Default::default()
        };
        Ok(self.connection.service_method(request).await?)
    }

    /// Sends a message to a friend (visible to them). Returns it as Steam
    /// stored it.
    pub async fn send_message(&self, friend: u64, text: &str) -> Result<ChatMessage, Error> {
        let response = self
            .connection
            .service_method(send_request(friend, text))
            .await?;
        Ok(ChatMessage {
            friend,
            from_me: true,
            text: text.to_owned(),
            timestamp: response.server_timestamp.unwrap_or(0),
            ordinal: response.ordinal.unwrap_or(0),
        })
    }

    /// Incoming messages, and the user's own messages from other clients,
    /// for as long as the connection lasts. Typing notices are dropped.
    pub fn chat_messages(&self) -> impl Stream<Item = ChatMessage> + 'static {
        self.connection
            .on_notification::<CFriendMessages_IncomingMessage_Notification>()
            .filter_map(|notification| async move { notification.ok().and_then(incoming_from) })
    }

    /// Records the recent history with `friend` and the incoming
    /// notifications during `wait` as fixtures. SteamIDs become dev-universe
    /// placeholders, every text becomes `Message <n>`, and only the fields
    /// Reborn reads are kept (ADR 0024).
    pub async fn record_chat(&self, friend: u64, wait: Duration) -> Result<ChatRecording, Error> {
        let mut incoming = Box::pin(
            self.connection
                .on_notification::<CFriendMessages_IncomingMessage_Notification>(),
        );
        let own = account_id(self.steam_id());
        let response = self.recent_messages_response(friend, 20).await?;
        let mut redactor = ChatRedactor::new(own, account_id(friend));
        let history = redactor
            .history(&response)
            .write_to_bytes()
            .map_err(|_| Error::IncompleteResponse("chat history"))?;
        let mut recorded = Vec::new();
        let deadline = tokio::time::Instant::now() + wait;
        while let Ok(Some(notification)) = tokio::time::timeout_at(deadline, incoming.next()).await
        {
            let Ok(notification) = notification else {
                continue;
            };
            recorded.push(
                redactor
                    .incoming(&notification)
                    .write_to_bytes()
                    .map_err(|_| Error::IncompleteResponse("chat message"))?,
            );
        }
        Ok(ChatRecording {
            history,
            incoming: recorded,
        })
    }
}

/// The send request, like node-steam-user's `sendFriendMessage`: BBCode on,
/// with `[` escaped so the text is shown as typed.
fn send_request(friend: u64, text: &str) -> CFriendMessages_SendMessage_Request {
    CFriendMessages_SendMessage_Request {
        steamid: Some(friend),
        chat_entry_type: Some(CHAT_MSG),
        message: Some(text.replace('[', "\\[")),
        contains_bbcode: Some(true),
        ..Default::default()
    }
}

/// History messages, oldest first. `own` is the user's account ID.
pub fn history_from(
    response: &CFriendMessages_GetRecentMessages_Response,
    friend: u64,
    own: u32,
) -> Vec<ChatMessage> {
    let mut messages: Vec<ChatMessage> = response
        .messages
        .iter()
        .map(|message: &FriendMessage| ChatMessage {
            friend,
            from_me: message.accountid == Some(own),
            text: message.message.clone().unwrap_or_default(),
            timestamp: message.timestamp.unwrap_or(0),
            ordinal: message.ordinal.unwrap_or(0),
        })
        .collect();
    messages.sort_by_key(|message| (message.timestamp, message.ordinal));
    messages
}

/// A received chat message, or `None` for typing notices and other entry
/// types.
pub fn incoming_from(
    notification: CFriendMessages_IncomingMessage_Notification,
) -> Option<ChatMessage> {
    if notification.chat_entry_type != Some(CHAT_MSG) {
        return None;
    }
    let text = notification
        .message_no_bbcode
        .or(notification.message)
        .unwrap_or_default();
    Some(ChatMessage {
        friend: notification.steamid_friend?,
        from_me: notification.local_echo.unwrap_or(false),
        text,
        timestamp: notification.rtime32_server_timestamp.unwrap_or(0),
        ordinal: notification.ordinal.unwrap_or(0),
    })
}

/// Replaces IDs and texts in chat fixtures.
struct ChatRedactor {
    own: u32,
    friend: u32,
    texts: HashMap<String, u32>,
}

impl ChatRedactor {
    fn new(own: u32, friend: u32) -> Self {
        Self {
            own,
            friend,
            texts: HashMap::new(),
        }
    }

    /// Placeholder account IDs: 1 for the user, 2 for the friend, 3 for
    /// anyone else.
    fn account(&self, account: u32) -> u32 {
        if account == self.own {
            1
        } else if account == self.friend {
            2
        } else {
            3
        }
    }

    /// The friend as a dev-universe SteamID64 (universe 4, never a public
    /// account; SteamKit2 `Types/SteamID.cs`).
    fn steam_id(&self, steam_id: u64) -> u64 {
        (4 << 56) | (1 << 52) | (1 << 32) | u64::from(self.account(account_id(steam_id)))
    }

    fn text(&mut self, text: &str) -> String {
        // Empty texts (typing notices) carry nothing personal.
        if text.is_empty() {
            return String::new();
        }
        let next = u32::try_from(self.texts.len() + 1).unwrap_or(u32::MAX);
        format!(
            "Message {}",
            self.texts.entry(text.to_owned()).or_insert(next)
        )
    }

    fn history(
        &mut self,
        response: &CFriendMessages_GetRecentMessages_Response,
    ) -> CFriendMessages_GetRecentMessages_Response {
        let mut redacted = CFriendMessages_GetRecentMessages_Response {
            more_available: response.more_available,
            ..Default::default()
        };
        for message in &response.messages {
            redacted.messages.push(FriendMessage {
                accountid: message.accountid.map(|account| self.account(account)),
                timestamp: message.timestamp,
                ordinal: message.ordinal,
                message: message.message.as_deref().map(|text| self.text(text)),
                ..Default::default()
            });
        }
        redacted
    }

    fn incoming(
        &mut self,
        notification: &CFriendMessages_IncomingMessage_Notification,
    ) -> CFriendMessages_IncomingMessage_Notification {
        let text = notification
            .message_no_bbcode
            .as_deref()
            .or(notification.message.as_deref())
            .map(|text| self.text(text));
        CFriendMessages_IncomingMessage_Notification {
            steamid_friend: notification.steamid_friend.map(|id| self.steam_id(id)),
            chat_entry_type: notification.chat_entry_type,
            message: text.clone(),
            message_no_bbcode: text,
            rtime32_server_timestamp: notification.rtime32_server_timestamp,
            ordinal: notification.ordinal,
            local_echo: notification.local_echo,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWN: u32 = 1000;
    const FRIEND: u64 = 0x0110_0001_0000_07D1; // account 2001

    fn history_message(account: u32, timestamp: u32, ordinal: u32, text: &str) -> FriendMessage {
        FriendMessage {
            accountid: Some(account),
            timestamp: Some(timestamp),
            ordinal: Some(ordinal),
            message: Some(text.to_owned()),
            ..Default::default()
        }
    }

    fn incoming(
        entry_type: i32,
        text: &str,
        echo: bool,
    ) -> CFriendMessages_IncomingMessage_Notification {
        CFriendMessages_IncomingMessage_Notification {
            steamid_friend: Some(FRIEND),
            chat_entry_type: Some(entry_type),
            message: Some(format!("[b]{text}[/b]")),
            message_no_bbcode: Some(text.to_owned()),
            rtime32_server_timestamp: Some(1_700_000_100),
            ordinal: Some(0),
            local_echo: Some(echo),
            ..Default::default()
        }
    }

    #[test]
    fn history_is_sorted_and_tells_own_messages_apart() {
        let response = CFriendMessages_GetRecentMessages_Response {
            messages: vec![
                history_message(2001, 200, 0, "second"),
                history_message(OWN, 100, 1, "first, later ordinal"),
                history_message(OWN, 100, 0, "first"),
            ],
            ..Default::default()
        };
        let messages = history_from(&response, FRIEND, OWN);
        let texts: Vec<&str> = messages.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["first", "first, later ordinal", "second"]);
        assert!(messages[0].from_me && !messages[2].from_me);
        assert!(messages.iter().all(|m| m.friend == FRIEND));
    }

    #[test]
    fn incoming_messages_use_the_plain_text_and_skip_typing() {
        let message = incoming_from(incoming(1, "hello", false)).unwrap();
        assert_eq!(message.text, "hello");
        assert!(!message.from_me);
        assert_eq!(message.friend, FRIEND);
        assert!(incoming_from(incoming(2, "", false)).is_none(), "typing");
        assert!(
            incoming_from(incoming(1, "mine", true)).unwrap().from_me,
            "local echo"
        );
    }

    #[test]
    fn sending_escapes_bbcode_and_marks_a_chat_message() {
        let request = send_request(FRIEND, "look at [this]");
        assert_eq!(request.message.as_deref(), Some("look at \\[this]"));
        assert_eq!(request.chat_entry_type, Some(1));
        assert_eq!(request.contains_bbcode, Some(true));
        assert_eq!(request.steamid, Some(FRIEND));
    }

    #[test]
    fn debug_output_hides_the_text() {
        let message = incoming_from(incoming(1, "secret plans", false)).unwrap();
        assert!(!format!("{message:?}").contains("secret"));
    }

    /// Recorded with a test account (see `tests/data/README.md`).
    const HISTORY: &[u8] = include_bytes!("../tests/data/chat_history.bin");
    const TYPING: &[u8] = include_bytes!("../tests/data/chat_incoming_typing.bin");
    const MESSAGE: &[u8] = include_bytes!("../tests/data/chat_incoming_message.bin");

    /// The friend's placeholder in the recordings (dev universe, account 2).
    const RECORDED_FRIEND: u64 = 0x0410_0001_0000_0002;

    #[test]
    fn replays_a_recorded_history() {
        let response =
            CFriendMessages_GetRecentMessages_Response::parse_from_bytes(HISTORY).unwrap();
        // The recorder maps the user to account 1.
        let messages = history_from(&response, RECORDED_FRIEND, 1);
        assert_eq!(messages.len(), 3);
        let order: Vec<(bool, &str)> = messages
            .iter()
            .map(|m| (m.from_me, m.text.as_str()))
            .collect();
        assert_eq!(
            order,
            [
                (true, "Message 2"),
                (false, "Message 1"),
                (false, "Message 1")
            ],
            "oldest first; the user wrote first"
        );
        assert!(
            messages
                .windows(2)
                .all(|w| w[0].timestamp <= w[1].timestamp)
        );
        assert_eq!(response.more_available, Some(false));
    }

    #[test]
    fn replays_recorded_incoming_notifications() {
        let typing =
            CFriendMessages_IncomingMessage_Notification::parse_from_bytes(TYPING).unwrap();
        assert_eq!(typing.chat_entry_type, Some(2), "a typing notice");
        assert!(incoming_from(typing).is_none());

        let message =
            CFriendMessages_IncomingMessage_Notification::parse_from_bytes(MESSAGE).unwrap();
        let message = incoming_from(message).unwrap();
        assert_eq!(message.friend, RECORDED_FRIEND);
        assert_eq!(message.text, "Message 4");
        assert!(!message.from_me);
        assert!(message.timestamp > 0);
    }

    #[test]
    fn redaction_replaces_ids_and_texts() {
        let mut redactor = ChatRedactor::new(OWN, account_id(FRIEND));
        let history = redactor.history(&CFriendMessages_GetRecentMessages_Response {
            messages: vec![
                history_message(OWN, 100, 0, "hi there"),
                history_message(2001, 101, 0, "hey"),
                history_message(OWN, 102, 0, "hi there"),
            ],
            ..Default::default()
        });
        let notification = redactor.incoming(&incoming(1, "hey", false));
        let bytes = [
            history.write_to_bytes().unwrap(),
            notification.write_to_bytes().unwrap(),
        ]
        .concat();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("hi there") && !text.contains("hey"));
        assert!(!bytes.windows(8).any(|w| w == FRIEND.to_le_bytes()));

        // Same text, same placeholder; the user is account 1, the friend 2.
        let messages = history_from(&history, FRIEND, 1);
        assert_eq!(messages[0].text, "Message 1");
        assert_eq!(messages[1].text, "Message 2");
        assert_eq!(messages[2].text, "Message 1");
        assert!(messages[0].from_me && !messages[1].from_me);
        let message = incoming_from(notification).unwrap();
        assert_eq!(message.text, "Message 2");
        assert_eq!(message.friend >> 56, 4);
    }
}
