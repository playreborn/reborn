//! Achievements of the signed-in account for one game (ADR 0029). Reading
//! only: Reborn never writes stats or achievements (red line 4).
//!
//! Steam answers `ClientGetUserStats` with the game's stats schema (binary
//! KeyValues) and the account's unlock times. Details and sources:
//! `docs/protocol/achievements.md`.

use std::io::{Read, Write};

use reborn_formats::binary;
use reborn_formats::vdf::{Object, Value};
use steam_vent::ConnectionTrait;
use steam_vent_proto_common::protobuf::Message;
use steam_vent_proto_common::{RpcMessage, RpcMessageWithKind};
use steam_vent_proto_steam::enums_clientserver::EMsg;
use steam_vent_proto_steam::steammessages_clientserver_userstats::{
    CMsgClientGetUserStats, CMsgClientGetUserStatsResponse,
};

use crate::{Error, Session};

/// `EResult::OK`.
const RESULT_OK: i32 = 1;

/// Language used when the schema has no text in the asked language.
pub const FALLBACK_LANGUAGE: &str = "english";

/// One achievement of a game, with the account's unlock state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Achievement {
    /// The game's internal name (API name), unique within the game.
    pub name: String,
    /// Display name in the asked language.
    pub display_name: String,
    /// Description in the asked language; may be empty.
    pub description: String,
    /// Whether the game hides this achievement until it is unlocked.
    pub hidden: bool,
    /// Unlock time in seconds since the Unix epoch, `None` while locked.
    pub unlocked_at: Option<u32>,
}

impl Session {
    /// The achievements of an app for the signed-in account, texts in
    /// `language` (Steam's language names, e.g. `german`, `english`).
    ///
    /// An app without achievements gives an empty list.
    pub async fn achievements(
        &self,
        app_id: u32,
        language: &str,
    ) -> Result<Vec<Achievement>, Error> {
        let response = self.user_stats_response(app_id).await?;
        achievements_from(&response, app_id, language)
    }

    /// Steam's answer for an app, protobuf-encoded, for a fixture. It holds
    /// the game's schema and the account's unlock times, but no SteamID.
    pub async fn record_achievements(&self, app_id: u32) -> Result<Vec<u8>, Error> {
        let response = self.user_stats_response(app_id).await?;
        response
            .write_to_bytes()
            .map_err(|_| Error::IncompleteResponse("user stats"))
    }

    async fn user_stats_response(
        &self,
        app_id: u32,
    ) -> Result<CMsgClientGetUserStatsResponse, Error> {
        let request = CMsgClientGetUserStats {
            // A plain app's game ID is its app ID.
            game_id: Some(u64::from(app_id)),
            // No local schema and no local stats: Steam sends both.
            schema_local_version: Some(-1),
            crc_stats: Some(0),
            steam_id_for_user: Some(self.steam_id()),
            ..Default::default()
        };
        let response = self
            .connection
            .job::<_, UserStatsResponse>(UserStatsRequest(request))
            .await?;
        Ok(response.0)
    }
}

/// The achievements from Steam's answer: every entry under `bits` of every
/// stat in the schema, in schema order, unlocked when its unlock time in
/// `achievement_blocks` is not zero.
pub fn achievements_from(
    response: &CMsgClientGetUserStatsResponse,
    app_id: u32,
    language: &str,
) -> Result<Vec<Achievement>, Error> {
    let code = response
        .eresult
        .ok_or(Error::IncompleteResponse("user stats"))?;
    if code != RESULT_OK {
        return Err(Error::Refused {
            what: "user stats",
            code,
        });
    }
    if response.game_id != Some(u64::from(app_id)) {
        return Err(Error::IncompleteResponse("user stats"));
    }
    let Some(schema) = response.schema.as_deref().filter(|bytes| !bytes.is_empty()) else {
        return Ok(Vec::new());
    };
    let document =
        binary::parse(schema).map_err(|error| Error::StatsSchema(app_id, error.to_string()))?;
    let Some(stats) = game_object(&document, app_id).and_then(|game| game.get_object("stats"))
    else {
        return Ok(Vec::new());
    };

    let mut achievements = Vec::new();
    for (stat_id, stat) in stats.iter() {
        let Some(bits) = stat.as_object().and_then(|stat| stat.get_object("bits")) else {
            continue;
        };
        let stat_id: u32 = stat_id
            .parse()
            .map_err(|_| Error::StatsSchema(app_id, format!("stat ID `{stat_id}`")))?;
        for (bit, entry) in bits.iter() {
            let bit: usize = bit.parse().map_err(|_| {
                Error::StatsSchema(app_id, format!("bit `{bit}` of stat {stat_id}"))
            })?;
            let Some(entry) = entry.as_object() else {
                continue;
            };
            achievements.push(achievement(
                entry,
                unlock_time(response, stat_id, bit),
                language,
            ));
        }
    }
    Ok(achievements)
}

/// The object for the app below the root. The root holds one object named
/// after the app ID.
fn game_object(document: &Object, app_id: u32) -> Option<&Object> {
    document.get_object(&app_id.to_string()).or_else(|| {
        let mut objects = document.iter().filter_map(|(_, value)| value.as_object());
        match (objects.next(), objects.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    })
}

fn unlock_time(response: &CMsgClientGetUserStatsResponse, stat_id: u32, bit: usize) -> Option<u32> {
    response
        .achievement_blocks
        .iter()
        .find(|block| block.achievement_id == Some(stat_id))
        .and_then(|block| block.unlock_time.get(bit).copied())
        .filter(|&time| time != 0)
}

fn achievement(entry: &Object, unlocked_at: Option<u32>, language: &str) -> Achievement {
    let name = entry.get_str("name").unwrap_or_default().to_owned();
    let display = entry.get_object("display");
    let text = |key: &str| display.and_then(|display| localized(display.get(key)?, language));
    Achievement {
        display_name: text("name").unwrap_or_else(|| name.clone()),
        description: text("desc").unwrap_or_default(),
        hidden: display.and_then(|display| display.get_str("hidden")) == Some("1"),
        unlocked_at,
        name,
    }
}

/// A text that is either a plain string or one string per language: the
/// asked language, then English.
fn localized(value: &Value, language: &str) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(languages) => languages
            .get_str(language)
            .or_else(|| languages.get_str(FALLBACK_LANGUAGE))
            .map(str::to_owned),
    }
}

/// `CMsgClientGetUserStats` with its EMsg. steam-vent-proto-steam does not
/// tie the user stats messages to EMsg numbers, so Reborn does (EMsg 818
/// and 819 from `enums_clientserver.rs`).
#[derive(Debug)]
struct UserStatsRequest(CMsgClientGetUserStats);

impl RpcMessage for UserStatsRequest {
    fn parse(reader: &mut dyn Read) -> steam_vent_proto_common::protobuf::Result<Self> {
        CMsgClientGetUserStats::parse_from_reader(reader).map(Self)
    }

    fn write(&self, writer: &mut dyn Write) -> steam_vent_proto_common::protobuf::Result<()> {
        self.0.write_to_writer(writer)
    }

    fn encode_size(&self) -> usize {
        self.0.compute_size() as usize
    }
}

impl RpcMessageWithKind for UserStatsRequest {
    type KindEnum = EMsg;
    const KIND: Self::KindEnum = EMsg::k_EMsgClientGetUserStats;
}

/// `CMsgClientGetUserStatsResponse` with its EMsg (819).
#[derive(Debug)]
struct UserStatsResponse(CMsgClientGetUserStatsResponse);

impl RpcMessage for UserStatsResponse {
    fn parse(reader: &mut dyn Read) -> steam_vent_proto_common::protobuf::Result<Self> {
        CMsgClientGetUserStatsResponse::parse_from_reader(reader).map(Self)
    }

    fn write(&self, writer: &mut dyn Write) -> steam_vent_proto_common::protobuf::Result<()> {
        self.0.write_to_writer(writer)
    }

    fn encode_size(&self) -> usize {
        self.0.compute_size() as usize
    }
}

impl RpcMessageWithKind for UserStatsResponse {
    type KindEnum = EMsg;
    const KIND: Self::KindEnum = EMsg::k_EMsgClientGetUserStatsResponse;
}

#[cfg(test)]
mod tests {
    use steam_vent_proto_steam::steammessages_clientserver_userstats::cmsg_client_get_user_stats_response::Achievement_Blocks;

    use super::*;

    /// Binary KeyValues writer for test schemas (types 0, 1, 2 and 8, see
    /// `reborn_formats::binary`).
    enum Kv<'a> {
        Object(&'a str, Vec<Kv<'a>>),
        Str(&'a str, &'a str),
        Int(&'a str, i32),
    }

    fn encode(entries: &[Kv<'_>], out: &mut Vec<u8>) {
        fn key(out: &mut Vec<u8>, key: &str) {
            out.extend_from_slice(key.as_bytes());
            out.push(0);
        }
        for entry in entries {
            match entry {
                Kv::Object(name, children) => {
                    out.push(0);
                    key(out, name);
                    encode(children, out);
                    out.push(8);
                }
                Kv::Str(name, value) => {
                    out.push(1);
                    key(out, name);
                    key(out, value);
                }
                Kv::Int(name, value) => {
                    out.push(2);
                    key(out, name);
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
    }

    fn schema() -> Vec<u8> {
        use Kv::*;
        let tree = Object(
            "480",
            vec![
                Str("gamename", "Spacewar"),
                Int("version", 3),
                Object(
                    "stats",
                    vec![
                        Object("1", vec![Str("name", "NumGames"), Int("type", 1)]),
                        Object(
                            "2",
                            vec![
                                Int("type", 4),
                                Object(
                                    "bits",
                                    vec![
                                        Object(
                                            "0",
                                            vec![
                                                Str("name", "ACH_WIN_ONE_GAME"),
                                                Object(
                                                    "display",
                                                    vec![
                                                        Object(
                                                            "name",
                                                            vec![
                                                                Str("english", "Winner"),
                                                                Str("german", "Gewinner"),
                                                            ],
                                                        ),
                                                        Object(
                                                            "desc",
                                                            vec![Str("english", "Win a game")],
                                                        ),
                                                        Int("hidden", 0),
                                                    ],
                                                ),
                                            ],
                                        ),
                                        Object(
                                            "1",
                                            vec![
                                                Str("name", "ACH_SECRET"),
                                                Object(
                                                    "display",
                                                    vec![Str("name", "Secret"), Int("hidden", 1)],
                                                ),
                                            ],
                                        ),
                                    ],
                                ),
                            ],
                        ),
                    ],
                ),
            ],
        );
        let mut out = Vec::new();
        encode(&[tree], &mut out);
        out.push(8);
        out
    }

    fn response(blocks: Vec<(u32, Vec<u32>)>) -> CMsgClientGetUserStatsResponse {
        CMsgClientGetUserStatsResponse {
            game_id: Some(480),
            eresult: Some(1),
            schema: Some(schema()),
            achievement_blocks: blocks
                .into_iter()
                .map(|(id, times)| Achievement_Blocks {
                    achievement_id: Some(id),
                    unlock_time: times,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn reads_achievements_and_unlock_times() {
        let achievements =
            achievements_from(&response(vec![(2, vec![1_700_000_000, 0])]), 480, "german").unwrap();
        assert_eq!(
            achievements,
            vec![
                Achievement {
                    name: "ACH_WIN_ONE_GAME".into(),
                    display_name: "Gewinner".into(),
                    description: "Win a game".into(),
                    hidden: false,
                    unlocked_at: Some(1_700_000_000),
                },
                Achievement {
                    name: "ACH_SECRET".into(),
                    display_name: "Secret".into(),
                    description: String::new(),
                    hidden: true,
                    unlocked_at: None,
                },
            ]
        );
    }

    #[test]
    fn missing_blocks_mean_locked() {
        let achievements = achievements_from(&response(Vec::new()), 480, "english").unwrap();
        assert_eq!(achievements[0].display_name, "Winner");
        assert!(achievements.iter().all(|a| a.unlocked_at.is_none()));
    }

    #[test]
    fn no_schema_means_no_achievements() {
        let mut answer = response(Vec::new());
        answer.schema = None;
        assert_eq!(
            achievements_from(&answer, 480, "english").unwrap(),
            Vec::new()
        );
    }

    #[test]
    fn refusals_and_other_apps_are_errors() {
        let mut refused = response(Vec::new());
        refused.eresult = Some(15);
        assert!(matches!(
            achievements_from(&refused, 480, "english"),
            Err(Error::Refused { code: 15, .. })
        ));
        assert!(achievements_from(&response(Vec::new()), 70, "english").is_err());
    }

    /// Recorded with the first test account (see `tests/data/README.md`).
    const RECORDED: &[u8] = include_bytes!("../tests/data/user_stats.bin");

    #[test]
    fn replays_recorded_user_stats() {
        let response = CMsgClientGetUserStatsResponse::parse_from_bytes(RECORDED).unwrap();
        // Bongo Cat has no German texts, so English is used.
        let achievements = achievements_from(&response, 3_419_430, "german").unwrap();
        assert_eq!(achievements.len(), 28);
        assert_eq!(achievements[0].name, "PET_1");
        assert_eq!(achievements[0].display_name, "Bongo Beat 1");
        assert_eq!(achievements[0].description, "1 bongo beat");
        assert_eq!(achievements[27].display_name, "Merchant");
        let unlocked: Vec<Option<u32>> = achievements[..5].iter().map(|a| a.unlocked_at).collect();
        assert_eq!(
            unlocked,
            [
                Some(1_795_133_048),
                Some(1_795_133_048),
                Some(1_795_133_591),
                Some(1_795_140_773),
                None
            ]
        );
        assert_eq!(
            achievements
                .iter()
                .filter(|a| a.unlocked_at.is_some())
                .count(),
            4
        );
        assert!(achievements.iter().all(|a| !a.hidden));
    }

    #[test]
    fn recorded_unlock_times_match_the_stat_bits() {
        // Observed: the value of the achievement stat has a bit set for
        // exactly the achievements with an unlock time.
        let response = CMsgClientGetUserStatsResponse::parse_from_bytes(RECORDED).unwrap();
        for block in &response.achievement_blocks {
            let bits = response
                .stats
                .iter()
                .find(|stat| stat.stat_id == block.achievement_id)
                .and_then(|stat| stat.stat_value)
                .unwrap_or(0);
            for (bit, &time) in block.unlock_time.iter().enumerate() {
                assert_eq!(bits >> bit & 1 == 1, time != 0, "bit {bit}");
            }
        }
    }

    #[test]
    fn broken_schema_is_an_error() {
        let mut answer = response(Vec::new());
        answer.schema = Some(vec![0, b'4']);
        assert!(matches!(
            achievements_from(&answer, 480, "english"),
            Err(Error::StatsSchema(480, _))
        ));
    }

    #[test]
    fn request_and_response_carry_their_emsg() {
        assert_eq!(UserStatsRequest::KIND, EMsg::k_EMsgClientGetUserStats);
        assert_eq!(
            UserStatsResponse::KIND,
            EMsg::k_EMsgClientGetUserStatsResponse
        );
        let request = UserStatsRequest(CMsgClientGetUserStats {
            game_id: Some(480),
            ..Default::default()
        });
        let mut bytes = Vec::new();
        request.write(&mut bytes).unwrap();
        assert_eq!(bytes.len(), request.encode_size());
        let back = UserStatsRequest::parse(&mut bytes.as_slice()).unwrap();
        assert_eq!(back.0.game_id, Some(480));
    }
}
