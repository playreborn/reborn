//! Friends and their persona (name, status, game), read-only (ADR 0020).
//!
//! Flow and fields, see `docs/protocol/friends.md`: Steam sends
//! `CMsgClientFriendsList` after logon; the client asks for persona data with
//! `CMsgClientRequestFriendData` and receives `CMsgClientPersonaState`, also
//! later whenever a friend changes. Sources: node-steam-user
//! `components/friends.js` (MIT) and SteamKit2 `enums.steamd` (values).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use steam_vent::ConnectionTrait;
use steam_vent_proto_common::protobuf::Message;
use steam_vent_proto_steam::steammessages_clientserver_friends::{
    CMsgClientAddFriend, CMsgClientAddFriendResponse, CMsgClientChangeStatus,
    CMsgClientFriendsList, CMsgClientPersonaState, CMsgClientRemoveFriend,
    CMsgClientRequestFriendData, cmsg_client_persona_state,
};
use tokio::sync::Notify;
use tracing::debug;

use crate::{Error, Session};

/// How long to wait for the friends list after logon.
const FRIENDS_LIST_TIMEOUT: Duration = Duration::from_secs(20);

/// `EClientPersonaStateFlag` Status 1 | PlayerName 2 | Presence 16 |
/// LastSeen 64 | GameExtraInfo 256 (SteamKit2 `enums.steamd`).
const PERSONA_FLAGS: u32 = 1 | 2 | 16 | 64 | 256;

/// Account type of individual users in a SteamID64 (bits 52-55; SteamKit2
/// `Types/SteamID.cs`, `EAccountType.Individual` = 1).
const INDIVIDUAL: u64 = 1;

/// SteamID64 of account ID 0 of an individual account in the public
/// universe, desktop instance: (1 << 56) | (1 << 52) | (1 << 32)
/// (SteamKit2 `Types/SteamID.cs`).
const INDIVIDUAL_BASE: u64 = 0x0110_0001_0000_0000;

/// `EResult::OK`.
const RESULT_OK: i32 = 1;

/// How the account relates to another (`EFriendRelationship`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relationship {
    /// No relationship (any more).
    None,
    /// Blocked by the user.
    Blocked,
    /// The other person sent a friend request.
    RequestRecipient,
    /// Friends.
    Friend,
    /// The user sent a friend request.
    RequestInitiator,
    /// Ignored.
    Ignored,
    /// A friend who is ignored.
    IgnoredFriend,
    /// A value this version does not know.
    Unknown(u32),
}

impl Relationship {
    fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Blocked,
            2 => Self::RequestRecipient,
            3 => Self::Friend,
            4 => Self::RequestInitiator,
            5 => Self::Ignored,
            6 => Self::IgnoredFriend,
            other => Self::Unknown(other),
        }
    }
}

/// Online status (`EPersonaState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PersonaState {
    /// Offline (or invisible to others).
    Offline,
    /// Online.
    Online,
    /// Busy.
    Busy,
    /// Away.
    Away,
    /// Snooze.
    Snooze,
    /// Looking to trade.
    LookingToTrade,
    /// Looking to play.
    LookingToPlay,
    /// Invisible (only reported for the own account).
    Invisible,
    /// A value this version does not know.
    Unknown(u32),
}

impl PersonaState {
    /// The `EPersonaState` value.
    pub fn to_raw(self) -> u32 {
        match self {
            Self::Offline => 0,
            Self::Online => 1,
            Self::Busy => 2,
            Self::Away => 3,
            Self::Snooze => 4,
            Self::LookingToTrade => 5,
            Self::LookingToPlay => 6,
            Self::Invisible => 7,
            Self::Unknown(value) => value,
        }
    }

    fn from_raw(value: u32) -> Self {
        match value {
            0 => Self::Offline,
            1 => Self::Online,
            2 => Self::Busy,
            3 => Self::Away,
            4 => Self::Snooze,
            5 => Self::LookingToTrade,
            6 => Self::LookingToPlay,
            7 => Self::Invisible,
            other => Self::Unknown(other),
        }
    }
}

/// An entry of the friends list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Friend {
    /// SteamID64.
    pub steam_id: u64,
    /// Relationship to the account.
    pub relationship: Relationship,
}

/// What Steam reported about a friend. Fields Steam left out are `None`;
/// callers merge them with what they knew before.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Persona {
    /// SteamID64.
    pub steam_id: u64,
    /// Profile name.
    pub name: Option<String>,
    /// Online status.
    pub state: Option<PersonaState>,
    /// The Steam app being played (`0` from Steam means none).
    pub game_app_id: Option<u32>,
    /// Name of a non-Steam game, if Steam sent one.
    pub game_name: Option<String>,
    /// Last logoff, seconds since 1970.
    pub last_logoff: Option<u32>,
}

/// The friends list as received after logon, filled in by a background task.
#[derive(Default)]
pub(crate) struct FriendsSlot {
    list: Mutex<Option<CMsgClientFriendsList>>,
    ready: Notify,
}

impl FriendsSlot {
    pub(crate) fn store(&self, list: CMsgClientFriendsList) {
        *self
            .list
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(list);
        self.ready.notify_waiters();
    }

    fn get(&self) -> Option<CMsgClientFriendsList> {
        self.list
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Takes the first full (not incremental) list from `lists` in the
    /// background. `lists` must be a subscription (`on`), not a one-time
    /// wait (`one`): steam-vent hands the next message of a kind to a
    /// pending one-time wait before any subscription, so an unused one would
    /// swallow the first incremental change (live, 2026-11-20).
    pub(crate) fn wait_in_background(
        self: &Arc<Self>,
        lists: impl Stream<Item = Result<CMsgClientFriendsList, steam_vent::NetworkError>>
        + Send
        + 'static,
    ) {
        let slot = Arc::clone(self);
        tokio::spawn(async move {
            let mut lists = Box::pin(lists);
            let full = async {
                while let Some(list) = lists.next().await {
                    match list {
                        Ok(list) if !list.bincremental.unwrap_or(false) => return Some(list),
                        Ok(_) => {}
                        Err(error) => debug!(%error, "unreadable friends list"),
                    }
                }
                None
            };
            match tokio::time::timeout(FRIENDS_LIST_TIMEOUT, full).await {
                Ok(Some(list)) => slot.store(list),
                Ok(None) => debug!("no friends list"),
                Err(_) => debug!("Steam sent no friends list in time"),
            }
        });
    }
}

/// Redacted fixture data, see [`Session::record_add_friend`].
#[derive(Debug)]
pub struct FriendRequestRecording {
    /// `CMsgClientAddFriendResponse`, protobuf-encoded.
    pub response: Vec<u8>,
    /// Incremental `CMsgClientFriendsList` messages received afterwards.
    pub changes: Vec<Vec<u8>>,
    /// Whether Steam accepted the request: the profile name, or the refusal.
    pub result: Result<Option<String>, Error>,
}

/// Redacted fixture data, see [`Session::record_friends`].
#[derive(Clone, Debug, Default)]
pub struct FriendsRecording {
    /// `CMsgClientFriendsList`, protobuf-encoded.
    pub friends_list: Vec<u8>,
    /// Every `CMsgClientPersonaState` received, protobuf-encoded.
    pub persona_states: Vec<Vec<u8>>,
}

impl Session {
    /// The account's friends (individual accounts with any relationship but
    /// `None`), as sent by Steam after logon.
    pub async fn friends(&self) -> Result<Vec<Friend>, Error> {
        Ok(friends_from(&self.friends_list().await?))
    }

    async fn friends_list(&self) -> Result<CMsgClientFriendsList, Error> {
        let waiting = self.friends.ready.notified();
        if let Some(list) = self.friends.get() {
            return Ok(list);
        }
        if tokio::time::timeout(FRIENDS_LIST_TIMEOUT, waiting)
            .await
            .is_err()
        {
            return self.friends.get().ok_or(Error::FriendsListTimeout);
        }
        self.friends.get().ok_or(Error::FriendsListTimeout)
    }

    /// Asks Steam for name, status and game of these accounts. The answers
    /// arrive on [`Session::persona_updates`].
    pub async fn request_personas(&self, steam_ids: &[u64]) -> Result<(), Error> {
        if steam_ids.is_empty() {
            return Ok(());
        }
        let request = CMsgClientRequestFriendData {
            persona_state_requested: Some(PERSONA_FLAGS),
            friends: steam_ids.to_vec(),
            ..CMsgClientRequestFriendData::default()
        };
        self.connection.send(request).await?;
        Ok(())
    }

    /// Sets the account's online status (ADR 0021). Visible to friends.
    pub async fn set_persona_state(&self, state: PersonaState) -> Result<(), Error> {
        self.connection.send(change_status(state)).await?;
        Ok(())
    }

    /// Sends a friend request, or accepts one the person sent (ADR 0022).
    /// Returns the person's profile name if Steam sent it. Visible to the
    /// other person.
    pub async fn add_friend(&self, steam_id: u64) -> Result<Option<String>, Error> {
        let response = self.add_friend_response(steam_id).await?;
        check_added(&response)?;
        Ok(response.persona_name_added)
    }

    async fn add_friend_response(
        &self,
        steam_id: u64,
    ) -> Result<CMsgClientAddFriendResponse, Error> {
        let request = CMsgClientAddFriend {
            steamid_to_add: Some(steam_id),
            ..CMsgClientAddFriend::default()
        };
        Ok(self
            .connection
            .job::<CMsgClientAddFriend, CMsgClientAddFriendResponse>(request)
            .await?)
    }

    /// Declines a friend request (ADR 0022). Steam sends no answer; the
    /// change arrives as an incremental friends list.
    pub async fn decline_friend(&self, steam_id: u64) -> Result<(), Error> {
        let request = CMsgClientRemoveFriend {
            friendid: Some(steam_id),
            ..CMsgClientRemoveFriend::default()
        };
        self.connection.send(request).await?;
        Ok(())
    }

    /// Relationship changes (incremental friends lists) for as long as the
    /// connection lasts. Entries with [`Relationship::None`] ended.
    pub fn friend_changes(&self) -> impl Stream<Item = Vec<Friend>> + 'static {
        self.connection
            .on::<CMsgClientFriendsList>()
            .filter_map(|message| async move {
                message
                    .ok()
                    .filter(|list| list.bincremental.unwrap_or(false))
                    .map(|list| relationship_changes(&list))
            })
    }

    /// Sends a friend request (or accepts one) and records the answer and
    /// the incremental lists that follow within `wait` as redacted
    /// fixtures, like [`Session::record_friends`].
    pub async fn record_add_friend(
        &self,
        steam_id: u64,
        wait: Duration,
    ) -> Result<FriendRequestRecording, Error> {
        let mut lists = Box::pin(self.connection.on::<CMsgClientFriendsList>());
        let response = self.add_friend_response(steam_id).await?;
        let mut redactor = Redactor::default();
        let mut redacted = CMsgClientAddFriendResponse {
            eresult: response.eresult,
            steam_id_added: response.steam_id_added.map(|id| redactor.id(id)),
            ..CMsgClientAddFriendResponse::default()
        };
        if response.persona_name_added.is_some() {
            let number = redactor.number(response.steam_id_added.unwrap_or(steam_id));
            redacted.persona_name_added = Some(format!("Friend {number}"));
        }
        let mut changes = Vec::new();
        let deadline = tokio::time::Instant::now() + wait;
        while let Ok(Some(message)) = tokio::time::timeout_at(deadline, lists.next()).await {
            let Ok(list) = message else { continue };
            changes.push(
                redact_friends_list(&list, &mut redactor)
                    .write_to_bytes()
                    .map_err(|_| Error::IncompleteResponse("friends list"))?,
            );
        }
        Ok(FriendRequestRecording {
            response: redacted
                .write_to_bytes()
                .map_err(|_| Error::IncompleteResponse("add friend"))?,
            changes,
            result: check_added(&response).map(|()| response.persona_name_added),
        })
    }

    /// Persona updates for as long as the connection lasts.
    pub fn persona_updates(&self) -> impl Stream<Item = Vec<Persona>> + 'static {
        self.connection
            .on::<CMsgClientPersonaState>()
            .filter_map(|message| async move { message.ok().map(personas_from) })
    }

    /// Records the friends list and the persona answers for `wait` as test
    /// fixtures. Every SteamID becomes a placeholder in the dev universe
    /// (never a real account), names become `Friend <n>`, and only the
    /// fields Reborn reads are kept (ADR 0020).
    pub async fn record_friends(&self, wait: Duration) -> Result<FriendsRecording, Error> {
        let list = self.friends_list().await?;
        let mut updates = Box::pin(self.connection.on::<CMsgClientPersonaState>());
        let ids: Vec<u64> = friends_from(&list)
            .into_iter()
            .map(|friend| friend.steam_id)
            .collect();
        self.request_personas(&ids).await?;

        let mut redactor = Redactor::default();
        let friends_list = redact_friends_list(&list, &mut redactor)
            .write_to_bytes()
            .map_err(|_| Error::IncompleteResponse("friends list"))?;
        let mut persona_states = Vec::new();
        let deadline = tokio::time::Instant::now() + wait;
        while let Ok(Some(message)) = tokio::time::timeout_at(deadline, updates.next()).await {
            let Ok(message) = message else { continue };
            let bytes = redact_persona_state(&message, &mut redactor)
                .write_to_bytes()
                .map_err(|_| Error::IncompleteResponse("persona state"))?;
            persona_states.push(bytes);
        }
        Ok(FriendsRecording {
            friends_list,
            persona_states,
        })
    }
}

/// `CMsgClientChangeStatus` with only `persona_state`: `player_name` would
/// rename the profile (node-steam-user `components/friends.js`,
/// `setPersona`).
fn change_status(state: PersonaState) -> CMsgClientChangeStatus {
    CMsgClientChangeStatus {
        persona_state: Some(state.to_raw()),
        ..CMsgClientChangeStatus::default()
    }
}

/// Turns Steam's answer to a friend request into an error unless it is OK.
fn check_added(response: &CMsgClientAddFriendResponse) -> Result<(), Error> {
    match response.eresult {
        Some(RESULT_OK) => Ok(()),
        Some(code) => Err(Error::Refused {
            what: "friend",
            code,
        }),
        None => Err(Error::IncompleteResponse("friend request")),
    }
}

/// Reads who to add: a SteamID64 of an individual account, a profile URL
/// `…/profiles/<SteamID64>`, or a friend code (the account ID; ADR 0022).
/// Custom profile URLs (`/id/<name>`) are not resolved.
pub fn steam_id_from_input(input: &str) -> Option<u64> {
    let input = input.trim().trim_end_matches('/');
    let number = match input.rfind("/profiles/") {
        Some(at) => &input[at + "/profiles/".len()..],
        None if input.contains('/') => return None,
        None => input,
    };
    let value: u64 = number.parse().ok()?;
    if value > u64::from(u32::MAX) {
        return is_individual(value).then_some(value);
    }
    (value != 0).then_some(INDIVIDUAL_BASE + value)
}

fn is_individual(steam_id: u64) -> bool {
    (steam_id >> 52) & 0xF == INDIVIDUAL
}

/// Friends of a list message: individual accounts only, entries with
/// relationship `None` dropped.
pub fn friends_from(list: &CMsgClientFriendsList) -> Vec<Friend> {
    list.friends
        .iter()
        .filter_map(|entry| {
            let steam_id = entry.ulfriendid?;
            let relationship = Relationship::from_raw(entry.efriendrelationship.unwrap_or(0));
            (is_individual(steam_id) && relationship != Relationship::None).then_some(Friend {
                steam_id,
                relationship,
            })
        })
        .collect()
}

/// Relationship changes of a list message, including ended relationships
/// ([`Relationship::None`]); individual accounts only.
pub fn relationship_changes(list: &CMsgClientFriendsList) -> Vec<Friend> {
    list.friends
        .iter()
        .filter_map(|entry| {
            let steam_id = entry.ulfriendid?;
            is_individual(steam_id).then_some(Friend {
                steam_id,
                relationship: Relationship::from_raw(entry.efriendrelationship.unwrap_or(0)),
            })
        })
        .collect()
}

/// Personas of a persona state message.
///
/// If the message answers the status (`status_flags` has the Status bit)
/// but a friend has no `persona_state`, the friend is offline: a missing
/// optional protobuf field means its default, 0 = Offline. Steam leaves the
/// field out for offline friends (recorded fixture `persona_state_0.bin`).
pub fn personas_from(message: CMsgClientPersonaState) -> Vec<Persona> {
    let answers_status = message.status_flags.unwrap_or(0) & 1 != 0;
    message
        .friends
        .into_iter()
        .filter_map(|friend| {
            let state = match friend.persona_state {
                Some(raw) => Some(PersonaState::from_raw(raw)),
                None if answers_status => Some(PersonaState::Offline),
                None => None,
            };
            Some(Persona {
                steam_id: friend.friendid?,
                name: friend.player_name,
                state,
                game_app_id: friend.game_played_app_id,
                game_name: friend.game_name.filter(|name| !name.is_empty()),
                last_logoff: friend.last_logoff,
            })
        })
        .collect()
}

/// Stable placeholders for real SteamIDs, numbered in order of appearance.
#[derive(Default)]
struct Redactor {
    ids: HashMap<u64, u32>,
}

impl Redactor {
    /// Universe 4 (dev), account type 1 (individual), instance 1: such IDs
    /// never belong to a public account (SteamKit2 `Types/SteamID.cs`,
    /// `EUniverse.Dev` = 4).
    fn id(&mut self, real: u64) -> u64 {
        let next = u32::try_from(self.ids.len() + 1).unwrap_or(u32::MAX);
        let number = *self.ids.entry(real).or_insert(next);
        let account_type = (real >> 52) & 0xF;
        (4 << 56) | (account_type << 52) | (1 << 32) | u64::from(number)
    }

    fn number(&mut self, real: u64) -> u32 {
        self.id(real) as u32
    }
}

fn redact_friends_list(
    list: &CMsgClientFriendsList,
    redactor: &mut Redactor,
) -> CMsgClientFriendsList {
    let mut redacted = CMsgClientFriendsList {
        bincremental: list.bincremental,
        ..CMsgClientFriendsList::default()
    };
    for entry in &list.friends {
        let mut kept = entry.clone();
        kept.special_fields = Default::default();
        kept.ulfriendid = entry.ulfriendid.map(|id| redactor.id(id));
        redacted.friends.push(kept);
    }
    redacted
}

fn redact_persona_state(
    message: &CMsgClientPersonaState,
    redactor: &mut Redactor,
) -> CMsgClientPersonaState {
    let mut redacted = CMsgClientPersonaState {
        status_flags: message.status_flags,
        ..CMsgClientPersonaState::default()
    };
    for friend in &message.friends {
        let Some(real) = friend.friendid else {
            continue;
        };
        let number = redactor.number(real);
        redacted.friends.push(cmsg_client_persona_state::Friend {
            friendid: Some(redactor.id(real)),
            persona_state: friend.persona_state,
            player_name: friend
                .player_name
                .as_ref()
                .map(|_| format!("Friend {number}")),
            game_played_app_id: friend.game_played_app_id,
            gameid: friend.gameid,
            // A name of a non-Steam game could be anything the user typed.
            game_name: friend.game_name.as_ref().map(|_| "Some Game".to_owned()),
            // A fixed time instead of when the friend was last seen.
            last_logoff: friend.last_logoff.map(|_| 1_700_000_000),
            ..cmsg_client_persona_state::Friend::default()
        });
    }
    redacted
}

#[cfg(test)]
mod tests {
    use super::*;
    use steam_vent_proto_steam::steammessages_clientserver_friends::cmsg_client_friends_list;

    const ALICE: u64 = 0x0110_0001_0000_0001;
    const BOB: u64 = 0x0110_0001_0000_0002;
    const CLAN: u64 = 0x0170_0000_0000_0003;

    fn entry(steam_id: u64, relationship: u32) -> cmsg_client_friends_list::Friend {
        cmsg_client_friends_list::Friend {
            ulfriendid: Some(steam_id),
            efriendrelationship: Some(relationship),
            ..Default::default()
        }
    }

    fn list() -> CMsgClientFriendsList {
        CMsgClientFriendsList {
            bincremental: Some(false),
            friends: vec![
                entry(ALICE, 3),
                entry(BOB, 2),
                entry(CLAN, 3),
                entry(BOB + 1, 0),
            ],
            ..Default::default()
        }
    }

    fn persona(
        steam_id: u64,
        name: &str,
        state: u32,
        app: u32,
    ) -> cmsg_client_persona_state::Friend {
        cmsg_client_persona_state::Friend {
            friendid: Some(steam_id),
            player_name: Some(name.to_owned()),
            persona_state: Some(state),
            game_played_app_id: Some(app),
            avatar_hash: Some(vec![1, 2, 3]),
            game_server_ip: Some(0x7f00_0001),
            last_logoff: Some(1_234_567),
            ..Default::default()
        }
    }

    #[test]
    fn reads_friends_and_skips_clans_and_ended_relationships() {
        assert_eq!(
            friends_from(&list()),
            [
                Friend {
                    steam_id: ALICE,
                    relationship: Relationship::Friend
                },
                Friend {
                    steam_id: BOB,
                    relationship: Relationship::RequestRecipient
                },
            ]
        );
    }

    #[test]
    fn reads_personas() {
        let message = CMsgClientPersonaState {
            friends: vec![persona(ALICE, "Alice", 1, 480)],
            ..Default::default()
        };
        let personas = personas_from(message);
        assert_eq!(personas.len(), 1);
        let alice = &personas[0];
        assert_eq!(alice.steam_id, ALICE);
        assert_eq!(alice.name.as_deref(), Some("Alice"));
        assert_eq!(alice.state, Some(PersonaState::Online));
        assert_eq!(alice.game_app_id, Some(480));
        assert_eq!(PersonaState::from_raw(42), PersonaState::Unknown(42));
    }

    /// Recorded with the test account (see `tests/data/README.md`).
    const FRIENDS_LIST: &[u8] = include_bytes!("../tests/data/friends_list.bin");
    const PERSONA_STATE: &[u8] = include_bytes!("../tests/data/persona_state_0.bin");

    #[test]
    fn replays_the_recorded_friends_list_and_persona() {
        let list = CMsgClientFriendsList::parse_from_bytes(FRIENDS_LIST).unwrap();
        let friends = friends_from(&list);
        assert_eq!(friends.len(), 1);
        assert_eq!(friends[0].relationship, Relationship::Friend);
        assert_eq!(
            friends[0].steam_id >> 56,
            4,
            "placeholder in the dev universe"
        );

        let message = CMsgClientPersonaState::parse_from_bytes(PERSONA_STATE).unwrap();
        assert_eq!(
            message.status_flags,
            Some(PERSONA_FLAGS),
            "answers our request"
        );
        let personas = personas_from(message);
        assert_eq!(personas.len(), 1);
        let friend = &personas[0];
        assert_eq!(friend.steam_id, friends[0].steam_id);
        assert_eq!(friend.name.as_deref(), Some("Friend 1"));
        assert_eq!(
            friend.state,
            Some(PersonaState::Offline),
            "no persona_state sent"
        );
        assert_eq!(friend.game_app_id, None);
    }

    #[tokio::test]
    async fn background_wait_takes_the_first_full_list() {
        let incremental = CMsgClientFriendsList {
            bincremental: Some(true),
            friends: vec![entry(BOB, 2)],
            ..Default::default()
        };
        let slot = Arc::new(FriendsSlot::default());
        let ready = slot.ready.notified();
        slot.wait_in_background(futures_util::stream::iter([Ok(incremental), Ok(list())]));
        tokio::time::timeout(Duration::from_secs(1), ready)
            .await
            .unwrap();
        let stored = slot.get().unwrap();
        assert_eq!(
            stored.bincremental,
            Some(false),
            "the full list, not the change"
        );
        assert_eq!(friends_from(&stored).len(), 2);
    }

    #[test]
    fn reads_who_to_add() {
        let id = 0x0110_0001_0000_002A; // account ID 42
        assert_eq!(steam_id_from_input("42"), Some(id), "friend code");
        assert_eq!(steam_id_from_input(&id.to_string()), Some(id));
        assert_eq!(
            steam_id_from_input(&format!(" https://steamcommunity.com/profiles/{id}/ ")),
            Some(id)
        );
        assert_eq!(INDIVIDUAL_BASE, 76_561_197_960_265_728);
        for bad in [
            "",
            "0",
            "abc",
            "https://steamcommunity.com/id/someone",
            "-5",
        ] {
            assert_eq!(steam_id_from_input(bad), None, "{bad}");
        }
        // A clan is not a person.
        assert_eq!(steam_id_from_input(&CLAN.to_string()), None);
    }

    #[test]
    fn incremental_lists_report_ended_relationships() {
        let list = CMsgClientFriendsList {
            bincremental: Some(true),
            friends: vec![entry(ALICE, 0), entry(BOB, 3), entry(CLAN, 3)],
            ..Default::default()
        };
        assert_eq!(
            relationship_changes(&list),
            [
                Friend {
                    steam_id: ALICE,
                    relationship: Relationship::None
                },
                Friend {
                    steam_id: BOB,
                    relationship: Relationship::Friend
                },
            ]
        );
    }

    /// Recorded with the test account (see `tests/data/README.md`).
    const ADD_FRIEND_REFUSED: &[u8] = include_bytes!("../tests/data/add_friend_refused.bin");

    #[test]
    fn replays_a_refused_friend_request() {
        let response = CMsgClientAddFriendResponse::parse_from_bytes(ADD_FRIEND_REFUSED).unwrap();
        assert_eq!(
            response.steam_id_added.map(|id| id >> 56),
            Some(4),
            "placeholder"
        );
        let error = check_added(&response).unwrap_err();
        assert!(matches!(error, Error::Refused { code: 24, .. }));
        assert_eq!(
            error.to_string(),
            "Steam refused the friend request (EResult 24)"
        );
    }

    #[test]
    fn refused_requests_carry_the_eresult() {
        let ok = CMsgClientAddFriendResponse {
            eresult: Some(1),
            ..Default::default()
        };
        assert!(check_added(&ok).is_ok());
        let refused = CMsgClientAddFriendResponse {
            eresult: Some(25),
            ..Default::default()
        };
        assert!(matches!(
            check_added(&refused),
            Err(Error::Refused { code: 25, .. })
        ));
    }

    #[test]
    fn status_change_sends_only_the_state() {
        // Field 1 (persona_state) as a varint, nothing else; in particular
        // no player_name, which would rename the profile.
        assert_eq!(
            change_status(PersonaState::Online)
                .write_to_bytes()
                .unwrap(),
            [0x08, 0x01]
        );
        assert_eq!(
            change_status(PersonaState::Invisible)
                .write_to_bytes()
                .unwrap(),
            [0x08, 0x07]
        );
        for raw in 0..8 {
            assert_eq!(PersonaState::from_raw(raw).to_raw(), raw);
        }
    }

    #[test]
    fn a_missing_state_is_unknown_without_the_status_flag() {
        let message = CMsgClientPersonaState {
            status_flags: Some(2),
            friends: vec![cmsg_client_persona_state::Friend {
                friendid: Some(ALICE),
                player_name: Some("Alice".to_owned()),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(personas_from(message)[0].state, None);
    }

    #[test]
    fn redaction_keeps_structure_but_no_personal_data() {
        let mut redactor = Redactor::default();
        let redacted_list = redact_friends_list(&list(), &mut redactor);
        let message = CMsgClientPersonaState {
            friends: vec![persona(ALICE, "Alice", 1, 480), persona(BOB, "Bob", 0, 0)],
            ..Default::default()
        };
        let redacted = redact_persona_state(&message, &mut redactor);

        let bytes = [
            redacted_list.write_to_bytes().unwrap(),
            redacted.write_to_bytes().unwrap(),
        ]
        .concat();
        for secret in [ALICE.to_le_bytes(), BOB.to_le_bytes()] {
            assert!(!bytes.windows(8).any(|window| window == secret));
        }
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("Alice") && !text.contains("Bob"));

        // The same friend gets the same placeholder in both messages.
        let friends = friends_from(&redacted_list);
        let personas = personas_from(redacted);
        assert_eq!(friends[0].steam_id, personas[0].steam_id);
        assert_eq!(friends.len(), 2, "relationships and clan filtering stay");
        assert_eq!(personas[0].name.as_deref(), Some("Friend 1"));
        assert_eq!(personas[0].game_app_id, Some(480));
        assert_eq!(personas[0].last_logoff, Some(1_700_000_000));
        let alice = &redacted_persona_fields(&message, &mut Redactor::default())[0];
        assert!(alice.avatar_hash.is_none() && alice.game_server_ip.is_none());
        // Placeholders are in the dev universe.
        assert!(friends.iter().all(|friend| friend.steam_id >> 56 == 4));
    }

    fn redacted_persona_fields(
        message: &CMsgClientPersonaState,
        redactor: &mut Redactor,
    ) -> Vec<cmsg_client_persona_state::Friend> {
        redact_persona_state(message, redactor).friends
    }
}
