//! One-to-one chat with friends (ADR 0024, ADR 0026).
//!
//! Methods and fields, see `docs/protocol/chat.md`; source: node-steam-user
//! `components/chatroom.js` (MIT), values from SteamKit2 `enums.steamd`.
//! Message text is personal data: this module never logs it.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use steam_vent::message::EncodableMessage;
use steam_vent::{ConnectionTrait, NetMessage, NetMessageHeader, ServiceMethodRequest};
use steam_vent_proto_common::protobuf::Message;
use steam_vent_proto_steam::enums_clientserver::EMsg;
use steam_vent_proto_steam::steammessages_friendmessages_steamclient::{
    CFriendMessages_AckMessage_Notification, CFriendMessages_GetRecentMessages_Request,
    CFriendMessages_GetRecentMessages_Response, CFriendMessages_IncomingMessage_Notification,
    CFriendMessages_SendMessage_Request, CFriendsMessages_GetActiveMessageSessions_Request,
    CFriendsMessages_GetActiveMessageSessions_Response,
    cfriend_messages_get_recent_messages_response::FriendMessage,
    cfriends_messages_get_active_message_sessions_response::FriendMessageSession,
};

use crate::{Error, Session};

/// `EChatEntryType::ChatMsg`.
const CHAT_MSG: i32 = 1;

/// `EChatEntryType::Typing`.
const TYPING: i32 = 2;

/// The SteamID64 of an individual account from its account ID (universe
/// public, type individual, instance 1; SteamKit2 `Types/SteamID.cs`), as
/// node-steam-user's `SteamID.fromIndividualAccountID`.
fn individual_steam_id(account: u32) -> u64 {
    (1 << 56) | (1 << 52) | (1 << 32) | u64::from(account)
}

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

/// Something that happened in a conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatEvent {
    /// A message arrived, or the user wrote one in another client.
    Message(ChatMessage),
    /// The friend is typing.
    Typing {
        /// The friend (SteamID64).
        friend: u64,
    },
}

/// A conversation with unread messages, from
/// `FriendMessages.GetActiveMessageSessions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnreadChat {
    /// The friend (SteamID64).
    pub friend: u64,
    /// How many of the friend's messages the user has not read yet.
    pub count: u32,
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
    /// `CFriendsMessages_GetActiveMessageSessions_Response`, protobuf-encoded.
    pub sessions: Vec<u8>,
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

    /// Tells the friend that the user is typing, like node-steam-user's
    /// `sendFriendTyping`: a `SendMessage` with entry type Typing and no
    /// text. Callers limit how often (ADR 0026).
    pub async fn send_typing(&self, friend: u64) -> Result<(), Error> {
        let mut request = send_request(friend, "");
        request.chat_entry_type = Some(TYPING);
        self.connection.service_method(request).await?;
        Ok(())
    }

    /// Marks the conversation with `friend` as read up to `timestamp` (the
    /// newest message's server time), so other clients clear their unread
    /// count. A notification without an answer, like node-steam-user's
    /// `ackFriendMessage`.
    pub async fn mark_read(&self, friend: u64, timestamp: u32) -> Result<(), Error> {
        self.connection
            .send(Notification(ack_request(friend, timestamp)))
            .await?;
        Ok(())
    }

    /// Conversations with unread messages.
    pub async fn unread_chats(&self) -> Result<Vec<UnreadChat>, Error> {
        let response = self.active_sessions().await?;
        Ok(unread_from(&response))
    }

    async fn active_sessions(
        &self,
    ) -> Result<CFriendsMessages_GetActiveMessageSessions_Response, Error> {
        // node-steam-user `getActiveFriendMessageSessions` sets no field
        // without `conversationsSince`.
        let request = CFriendsMessages_GetActiveMessageSessions_Request::default();
        Ok(self.connection.service_method(request).await?)
    }

    /// Incoming messages, the user's own messages from other clients, and
    /// typing notices, for as long as the connection lasts.
    pub fn chat_events(&self) -> impl Stream<Item = ChatEvent> + 'static {
        self.connection
            .on_notification::<CFriendMessages_IncomingMessage_Notification>()
            .filter_map(|notification| async move { notification.ok().and_then(chat_event_from) })
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
        let sessions = redactor
            .sessions(&self.active_sessions().await?)
            .write_to_bytes()
            .map_err(|_| Error::IncompleteResponse("chat sessions"))?;
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
            sessions,
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

/// The read notice for `FriendMessages.AckMessage#1`: `steamid_partner` and
/// `timestamp`, as node-steam-user's `ackFriendMessage`.
fn ack_request(friend: u64, timestamp: u32) -> CFriendMessages_AckMessage_Notification {
    CFriendMessages_AckMessage_Notification {
        steamid_partner: Some(friend),
        timestamp: Some(timestamp),
        ..Default::default()
    }
}

/// A service method sent as a notification: `ServiceMethodCallFromClient`
/// with the method as target job name and no source job, so Steam sends no
/// answer. node-steam-user does this when `_sendUnified` gets no callback
/// (`components/03-messages.js`); steam-vent's own wrapper always waits for
/// an answer. Only ever sent, so reading keeps steam-vent's default.
#[derive(Debug)]
struct Notification<T>(T);

impl<T: ServiceMethodRequest + Send> EncodableMessage for Notification<T> {
    fn write_body<W: std::io::Write>(&self, mut writer: W) -> Result<(), std::io::Error> {
        self.0
            .write(&mut writer)
            .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidData))
    }

    fn encode_size(&self) -> usize {
        ServiceMethodRequest::encode_size(&self.0)
    }

    fn process_header(&self, header: &mut NetMessageHeader) {
        header.target_job_name = Some(T::REQ_NAME.into());
    }
}

impl<T: ServiceMethodRequest + Send> NetMessage for Notification<T> {
    type KindEnum = EMsg;
    const KIND: Self::KindEnum = EMsg::k_EMsgServiceMethodCallFromClient;
    const IS_PROTOBUF: bool = true;
}

/// Conversations with unread messages, from the active sessions.
pub fn unread_from(
    response: &CFriendsMessages_GetActiveMessageSessions_Response,
) -> Vec<UnreadChat> {
    response
        .message_sessions
        .iter()
        .filter_map(|session: &FriendMessageSession| {
            let count = session.unread_message_count.unwrap_or(0);
            let friend = individual_steam_id(session.accountid_friend?);
            (count > 0).then_some(UnreadChat { friend, count })
        })
        .collect()
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

/// A message or typing notice, or `None` for other entry types and for the
/// user's own typing in another client.
pub fn chat_event_from(
    notification: CFriendMessages_IncomingMessage_Notification,
) -> Option<ChatEvent> {
    if notification.chat_entry_type == Some(TYPING) {
        if notification.local_echo.unwrap_or(false) {
            return None;
        }
        return Some(ChatEvent::Typing {
            friend: notification.steamid_friend?,
        });
    }
    incoming_from(notification).map(ChatEvent::Message)
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

    /// Keeps only the friend (as placeholder) and the counts and times.
    fn sessions(
        &self,
        response: &CFriendsMessages_GetActiveMessageSessions_Response,
    ) -> CFriendsMessages_GetActiveMessageSessions_Response {
        CFriendsMessages_GetActiveMessageSessions_Response {
            timestamp: response.timestamp,
            message_sessions: response
                .message_sessions
                .iter()
                .map(|session| FriendMessageSession {
                    accountid_friend: session.accountid_friend.map(|a| self.account(a)),
                    last_message: session.last_message,
                    last_view: session.last_view,
                    unread_message_count: session.unread_message_count,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
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
    fn typing_is_sent_as_an_empty_typing_entry() {
        let mut request = send_request(FRIEND, "");
        request.chat_entry_type = Some(super::TYPING);
        assert_eq!(request.chat_entry_type, Some(2));
        assert_eq!(request.message.as_deref(), Some(""));
    }

    #[test]
    fn typing_notices_become_events_but_not_own_ones() {
        assert_eq!(
            chat_event_from(incoming(2, "", false)),
            Some(ChatEvent::Typing { friend: FRIEND })
        );
        assert_eq!(chat_event_from(incoming(2, "", true)), None, "own typing");
        assert!(matches!(
            chat_event_from(incoming(1, "hi", false)),
            Some(ChatEvent::Message(_))
        ));
        assert_eq!(chat_event_from(incoming(6, "", false)), None, "left");
    }

    #[test]
    fn read_notices_are_sent_without_a_job() {
        let notification = Notification(ack_request(FRIEND, 1_700_000_100));
        let mut header = NetMessageHeader::default();
        notification.process_header(&mut header);
        assert_eq!(
            header.target_job_name.as_deref(),
            Some("FriendMessages.AckMessage#1")
        );
        assert_eq!(
            <Notification<CFriendMessages_AckMessage_Notification> as NetMessage>::KIND,
            EMsg::k_EMsgServiceMethodCallFromClient
        );
        let mut body = Vec::new();
        notification.write_body(&mut body).unwrap();
        assert_eq!(body.len(), notification.encode_size());
        let decoded = CFriendMessages_AckMessage_Notification::parse_from_bytes(&body).unwrap();
        assert_eq!(decoded.steamid_partner, Some(FRIEND));
        assert_eq!(decoded.timestamp, Some(1_700_000_100));
    }

    #[test]
    fn unread_counts_skip_read_conversations() {
        let session = |account, unread| FriendMessageSession {
            accountid_friend: Some(account),
            unread_message_count: Some(unread),
            ..Default::default()
        };
        let response = CFriendsMessages_GetActiveMessageSessions_Response {
            message_sessions: vec![session(2001, 3), session(2002, 0)],
            ..Default::default()
        };
        assert_eq!(
            unread_from(&response),
            [UnreadChat {
                friend: 0x0110_0001_0000_07D1,
                count: 3
            }]
        );
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
    const SESSIONS: &[u8] = include_bytes!("../tests/data/chat_sessions.bin");

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
        assert_eq!(
            chat_event_from(typing.clone()),
            Some(ChatEvent::Typing {
                friend: RECORDED_FRIEND
            })
        );
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
    fn replays_recorded_unread_counts() {
        let response =
            CFriendsMessages_GetActiveMessageSessions_Response::parse_from_bytes(SESSIONS).unwrap();
        // Three conversations; only the friend's (account 2) has unread
        // messages, and the user never viewed it.
        assert_eq!(response.message_sessions.len(), 3);
        assert_eq!(response.message_sessions[0].last_view, Some(0));
        assert_eq!(
            unread_from(&response),
            [UnreadChat {
                friend: individual_steam_id(2),
                count: 3
            }]
        );
    }

    #[test]
    fn redaction_keeps_only_counts_and_times_of_sessions() {
        let redactor = ChatRedactor::new(OWN, account_id(FRIEND));
        let response = CFriendsMessages_GetActiveMessageSessions_Response {
            message_sessions: vec![FriendMessageSession {
                accountid_friend: Some(2001),
                last_message: Some(100),
                last_view: Some(90),
                unread_message_count: Some(1),
                ..Default::default()
            }],
            timestamp: Some(101),
            ..Default::default()
        };
        let redacted = redactor.sessions(&response);
        assert_eq!(redacted.message_sessions[0].accountid_friend, Some(2));
        assert_eq!(redacted.message_sessions[0].unread_message_count, Some(1));
        assert_eq!(redacted.timestamp, Some(101));
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
