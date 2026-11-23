//! Messages between the shim (Reborn's `steamclient` replacement inside a
//! game process) and `rebornd`, as decided in ADR 0047.
//!
//! One JSON object per line. The shim sends [`ShimMessage`]s, the daemon
//! answers with [`DaemonMessage`]s; a request and its response share an
//! `id`. The first exchange is [`Request::Hello`].
//!
//! Version 0.2 adds the account-data messages of ADR 0055 (stage 0):
//! [`Request::AppOwnership`], answered with [`Response::Ownership`] or
//! [`Response::Unavailable`], and [`Request::RequestStats`], answered with
//! [`Response::Accepted`] and later the event [`Event::StatsReceived`]
//! (in a [`DaemonMessage::Event`], which has no `id`). The cloud reads
//! [`Request::CloudFileSize`] and [`Request::CloudRead`] are answered from
//! the run's local cloud folder with [`Response::FileSize`] and
//! [`Response::FileData`] (file content as base64 text, [`Bytes`]). The
//! other messages of ADR 0055 (stores, cloud writes) follow in later
//! stage-0 changes under the same version, before any release.
//!
//! Shadow sessions (ADR 0055, amending ADR 0047): `reborn-observe arm
//! --app <id>` registers one app with [`Request::RegisterShadow`] (and
//! `disarm` drops it with [`Request::CancelShadow`]); the observation
//! proxy inside the game then says [`Request::ShadowHello`] with its
//! process ID and that app, and its connection becomes a read-only shadow
//! session for that one process. A shadow session gets only read answers;
//! every write is refused with [`UnavailableReason::NotAllowed`]
//! ([`Request::writes`]).
//!
//! Version 0.3 adds the persona name comparison below
//! ([`Request::ComparePersonaName`], [`Response::PersonaNameCompared`],
//! [`UnavailableReason::NotReceived`]). A 0.2 daemon does not know them and
//! closes the connection on such a line, so a peer sends them only after
//! the daemon's [`Session::version`] says 0.3 or later
//! ([`Version::has_persona_name_comparison`]).
//!
//! Version 0.4 adds each achievement's place in the stats schema to
//! [`Event::StatsReceived`] ([`AchievementName::position`],
//! [`SchemaPosition`]; ADR 0062): the stat ID and the `bits` key, schema
//! data, not account data. The field is optional and an older peer
//! ignores it (no message type is added), but only a daemon of 0.4 or
//! later sends it, so a peer reads it only after the daemon's
//! [`Session::version`] says 0.4 or later
//! ([`Version::has_schema_positions`]).
//!
//! Persona name comparison (ADR 0056, decision 2): a shadow session may
//! send [`Request::ComparePersonaName`] with the SHA-256 of the text the
//! official client returned at `SteamFriends017` position 0
//! ([`NameDigest`]); the daemon answers [`Response::PersonaNameCompared`]
//! with only whether it equals the account's persona name. The name never
//! crosses the channel, and the daemon never hands it to the game process.
//! A digest of a short name can be reversed by guessing, so it is treated
//! like the name: `Debug` never shows it and nobody logs it.
//!
//! This crate holds only message types and framing (MIT OR Apache-2.0), so
//! the shim never depends on GPL crates (AGENTS.md section 3).
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// The protocol version in [`Request::Hello`]. A daemon refuses a shim
/// with another major version.
pub const PROTOCOL_VERSION: Version = Version { major: 0, minor: 4 };

/// A protocol version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// Changes when old peers can no longer understand new messages.
    pub major: u32,
    /// Changes when messages are only added.
    pub minor: u32,
}

impl Version {
    /// Whether a daemon of this version knows
    /// [`Request::ComparePersonaName`] (added in 0.3).
    pub fn has_persona_name_comparison(self) -> bool {
        self.major == 0 && self.minor >= 3
    }

    /// Whether a daemon of this version sends [`AchievementName::position`]
    /// in [`Event::StatsReceived`] (added in 0.4, ADR 0062).
    pub fn has_schema_positions(self) -> bool {
        self.major == 0 && self.minor >= 4
    }
}

/// A line the shim sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShimMessage {
    /// Matches the response to the request.
    pub id: u64,
    /// What the shim asks for.
    pub request: Request,
}

/// What the shim asks the daemon.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// The first message: protocol version and the game's process ID, so
    /// the daemon can find the app it launched for that process.
    Hello {
        /// The shim's protocol version.
        version: Version,
        /// The game's process ID.
        pid: u32,
    },
    /// Whether the account owns an app (ADR 0055). Only the session's own
    /// app and the DLC its app info names are answered; any other app ID
    /// gets [`UnavailableReason::NotAllowed`].
    AppOwnership {
        /// The app asked about.
        app_id: u32,
    },
    /// The account's stats for the session's own app (ADR 0055; the shim
    /// sends it for `RequestCurrentStats`). Answered with
    /// [`Response::Accepted`] and later [`Event::StatsReceived`], or with
    /// [`Response::Unavailable`]. Only reads; nothing is written.
    RequestStats,
    /// The size of a file in the run's cloud folder (ADR 0055; the shim
    /// sends it for `GetFileSize`). Answered with [`Response::FileSize`] or
    /// [`Response::Unavailable`]. Only reads.
    CloudFileSize {
        /// The file name as the game passed it (untrusted; the daemon
        /// checks it like an install path).
        name: String,
    },
    /// The content of a file in the run's cloud folder (ADR 0055; the shim
    /// sends it for `FileRead`): at most `max` bytes from its start.
    /// Answered with [`Response::FileData`] or [`Response::Unavailable`].
    /// Only reads.
    CloudRead {
        /// The file name as the game passed it (untrusted).
        name: String,
        /// The size of the game's buffer: at most this many bytes come
        /// back.
        max: u32,
    },
    /// Sent by `reborn-observe arm --app <id>` (ADR 0055, stage 1): the
    /// next process that says [`Request::ShadowHello`] for this app gets a
    /// read-only shadow session. The game is not running yet when `arm`
    /// runs, so the process is bound at its `ShadowHello`, once. Answered
    /// with [`Response::Accepted`], or [`Response::Unavailable`]
    /// (`NotSignedIn` without a Valve session, `Offline` while the daemon
    /// reconnects or cannot load the licence list, `NotAllowed` on a
    /// connection bound to a process). A new registration replaces an
    /// unclaimed one.
    RegisterShadow {
        /// The app of the game about to run.
        app_id: u32,
    },
    /// Sent by `reborn-observe disarm`: drops a registration nobody has
    /// claimed. Answered with [`Response::Accepted`], or `NotAllowed` on a
    /// connection bound to a process.
    CancelShadow,
    /// The observation proxy's first message (ADR 0055): like
    /// [`Request::Hello`], for a process `rebornd` did not launch, which
    /// asks for the shadow session registered for `app_id`. Answered with
    /// [`Response::Session`], or [`Response::Refused`] with
    /// [`RefusalReason::UnknownProcess`] when no such registration is
    /// open.
    ShadowHello {
        /// The proxy's protocol version.
        version: Version,
        /// The game's process ID.
        pid: u32,
        /// The app `reborn-observe arm --app` was given.
        app_id: u32,
    },
    /// Whether the account's persona name has this SHA-256 (ADR 0056,
    /// decision 2). Only on a shadow session; any other connection gets
    /// [`UnavailableReason::NotAllowed`]. Answered with
    /// [`Response::PersonaNameCompared`], or [`Response::Unavailable`]
    /// (`NotReceived` when Valve sent no persona name in this session).
    ComparePersonaName {
        /// SHA-256 of the UTF-8 bytes of the official client's text (up to
        /// its NUL, without it).
        sha256: NameDigest,
    },
}

/// A SHA-256 as 64 lowercase hex digits, of text that is account data
/// (ADR 0056). `Debug` never shows it; there is no `Display`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NameDigest(String);

impl NameDigest {
    /// The digest from 64 hex digits (upper case is folded to lower), or
    /// `None` for anything else.
    pub fn from_hex(hex: &str) -> Option<Self> {
        (hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| Self(hex.to_ascii_lowercase()))
    }

    /// The 64 lowercase hex digits, only to compare them.
    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for NameDigest {
    type Error = &'static str;

    fn try_from(hex: String) -> Result<Self, Self::Error> {
        Self::from_hex(&hex).ok_or("not a SHA-256 in hex")
    }
}

impl From<NameDigest> for String {
    fn from(digest: NameDigest) -> Self {
        digest.0
    }
}

impl std::fmt::Debug for NameDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NameDigest(<redacted>)")
    }
}

impl Request {
    /// Whether the request would write something (stats, achievements,
    /// cloud files). A shadow session is refused every write (ADR 0055:
    /// writes are never shadowed). No message of protocol 0.2 writes yet;
    /// the match has no catch-all, so each new message must be decided
    /// here.
    pub fn writes(&self) -> bool {
        match self {
            Request::Hello { .. }
            | Request::AppOwnership { .. }
            | Request::RequestStats
            | Request::CloudFileSize { .. }
            | Request::CloudRead { .. }
            | Request::RegisterShadow { .. }
            | Request::CancelShadow
            | Request::ShadowHello { .. }
            | Request::ComparePersonaName { .. } => false,
        }
    }
}

/// A line the daemon sends: a response to a request, or an event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DaemonMessage {
    /// The answer to the request with the same `id`.
    Response {
        /// The request's `id`.
        id: u64,
        /// The answer.
        response: Response,
    },
    /// Something the daemon tells the shim without a request of its own,
    /// such as the result of an accepted request (ADR 0047: events have no
    /// `id`).
    Event {
        /// What happened.
        event: Event,
    },
}

/// What the daemon tells the shim on its own (ADR 0055). The shim, not the
/// daemon, turns an event into callback bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Valve answered the stats request of an accepted
    /// [`Request::RequestStats`] (`ClientGetUserStats`). Stat values and
    /// unlock state are not sent while the readers are candidates
    /// (ADR 0055).
    StatsReceived {
        /// The `EResult` Valve sent; 1 is OK.
        result: i32,
        /// For `GetAchievementDisplayAttribute(name, "name")`: each
        /// achievement's display name, in English until Reborn has a
        /// language setting (ADR 0055). Empty when `result` is not OK.
        achievement_names: Vec<AchievementName>,
    },
}

/// One achievement's display name.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AchievementName {
    /// The game's internal (API) name of the achievement.
    pub name: String,
    /// Its display name.
    pub display_name: String,
    /// Where the schema holds it (protocol 0.4, ADR 0062); `None` from an
    /// older daemon, which does not send the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<SchemaPosition>,
}

/// An achievement's place in the game's stats schema (ADR 0062): the stat
/// whose `bits` object holds it and its key there
/// (`docs/protocol/achievements.md`). Schema data: the same for every
/// account that gets the schema, with no unlock state, value or time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaPosition {
    /// The stat ID, the key of the stat in the schema's `stats` object.
    pub stat_id: u32,
    /// The key in that stat's `bits` object, as a number.
    pub bit: u32,
}

/// The daemon's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// The session for this process: values the shim may cache and answer
    /// locally (ADR 0047). Only confirmed meanings get a field here.
    Session(Session),
    /// No session: why, for the shim's log.
    Refused {
        /// Why the daemon refused.
        reason: RefusalReason,
    },
    /// The answer to [`Request::AppOwnership`]: whether the account owns
    /// the app, from the licence list of the current Valve session.
    Ownership {
        /// Whether the app is owned.
        owned: bool,
    },
    /// The request is taken; for [`Request::RequestStats`] its result
    /// follows as the event [`Event::StatsReceived`]. For
    /// [`Request::RegisterShadow`] and [`Request::CancelShadow`] nothing
    /// follows.
    Accepted,
    /// The answer to [`Request::CloudFileSize`]: the file's size in bytes,
    /// `None` when the run's cloud folder has no such file. What the
    /// official client answers for a missing file is not recorded, so the
    /// shim stops the game on `None` (ADR 0055).
    FileSize {
        /// The size, or `None` for a missing file.
        size: Option<u32>,
    },
    /// The answer to [`Request::CloudRead`]: the file's first bytes (at
    /// most `max`), `None` when the run's cloud folder has no such file.
    FileData {
        /// The bytes, or `None` for a missing file.
        data: Option<Bytes>,
    },
    /// The answer to [`Request::ComparePersonaName`]: only whether the
    /// digest is the one of the account's persona name (ADR 0056).
    PersonaNameCompared {
        /// Whether they are the same.
        matches: bool,
    },
    /// The daemon cannot give the real answer to a request. The shim never
    /// makes one up (ADR 0055, "Honest failure").
    Unavailable {
        /// Why, for the shim's log.
        reason: UnavailableReason,
    },
}

/// Why the daemon cannot answer a request (ADR 0055).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnavailableReason {
    /// No Valve session.
    NotSignedIn,
    /// The connection to Valve is lost and the answer needs it.
    Offline,
    /// Not this session's business: no session on this connection, or an
    /// app other than the session's own app and its DLC.
    NotAllowed,
    /// The data is larger than the protocol allows.
    TooLarge,
    /// Valve has not sent the data in the current session (for
    /// [`Request::ComparePersonaName`]: no persona name).
    NotReceived,
}

/// File content in a message: base64 text (RFC 4648, standard alphabet,
/// with padding) on the wire, bytes in memory. `Debug` shows only the
/// length: cloud files are the user's saves (account data, never logged).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Bytes(pub Vec<u8>);

impl std::fmt::Debug for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bytes(<{} bytes>)", self.0.len())
    }
}

impl Serialize for Bytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&base64::encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        base64::decode(&text)
            .map(Bytes)
            .ok_or_else(|| serde::de::Error::custom("not valid base64"))
    }
}

/// Base64 (RFC 4648 section 4) without a dependency: ADR 0055 keeps this
/// crate on `serde`, `serde_json` and `thiserror` only.
mod base64 {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub(crate) fn encode(bytes: &[u8]) -> String {
        let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
                if i <= chunk.len() {
                    text.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
                } else {
                    text.push('=');
                }
            }
        }
        text
    }

    fn value(c: u8) -> Option<u32> {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        Some(u32::from(v))
    }

    /// The bytes of canonical base64 text, `None` for anything else
    /// (wrong length, other characters, padding in the middle, or unused
    /// bits that are not zero).
    pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
        let text = text.as_bytes();
        if !text.len().is_multiple_of(4) {
            return None;
        }
        let mut bytes = Vec::with_capacity(text.len() / 4 * 3);
        let groups = text.len() / 4;
        for (index, group) in text.chunks(4).enumerate() {
            let padding = group.iter().rev().take_while(|&&c| c == b'=').count();
            if padding > 2 || (padding > 0 && index + 1 != groups) {
                return None;
            }
            let mut n = 0u32;
            for &c in &group[..4 - padding] {
                n = (n << 6) | value(c)?;
            }
            n <<= 6 * padding as u32;
            let out = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            let keep = 3 - padding;
            // Canonical only: the bits the padding drops must be zero.
            if out[keep..].iter().any(|&b| b != 0) {
                return None;
            }
            bytes.extend_from_slice(&out[..keep]);
        }
        Some(bytes)
    }
}

/// Values the shim keeps for the whole run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// The daemon's protocol version.
    pub version: Version,
    /// The app the daemon launched this process for; the answer to
    /// `GetAppID` (`docs/shim/meanings.md`).
    pub app_id: u32,
}

/// Why the daemon gave no session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalReason {
    /// The shim's major version differs from the daemon's.
    IncompatibleVersion,
    /// The daemon did not launch this process (for
    /// [`Request::ShadowHello`]: no open shadow registration for its app).
    UnknownProcess,
}

/// Reading a line failed.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// The line is not one valid message.
    #[error("not a valid message: {0}")]
    Invalid(#[from] serde_json::Error),
    /// The text holds more than one line.
    #[error("a message must be exactly one line")]
    MultipleLines,
}

/// Where `rebornd` listens for shims (ADR 0047): on Windows the name of a
/// named pipe in the local namespace (`reborn-shim-<user>`, user name with
/// everything but ASCII letters and digits replaced by `_`), elsewhere the
/// path of a socket file (`$XDG_RUNTIME_DIR/reborn-shim.sock`, or the temp
/// directory with the user name in the file name). Same scheme as the
/// frontend socket of ADR 0008.
pub fn socket_name() -> String {
    if cfg!(windows) {
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_owned());
        let user: String = user
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        format!("reborn-shim-{user}")
    } else {
        match std::env::var_os("XDG_RUNTIME_DIR") {
            Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir)
                .join("reborn-shim.sock")
                .to_string_lossy()
                .into_owned(),
            _ => {
                let user = std::env::var("USER").unwrap_or_else(|_| "default".to_owned());
                std::env::temp_dir()
                    .join(format!("reborn-shim-{user}.sock"))
                    .to_string_lossy()
                    .into_owned()
            }
        }
    }
}

/// One message as a line, ending in `\n`.
pub fn encode<T: Serialize>(message: &T) -> Result<String, FrameError> {
    let mut line = serde_json::to_string(message)?;
    line.push('\n');
    Ok(line)
}

/// One message from a line (with or without its line ending).
pub fn decode<'a, T: Deserialize<'a>>(line: &'a str) -> Result<T, FrameError> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.contains('\n') {
        return Err(FrameError::MultipleLines);
    }
    Ok(serde_json::from_str(line)?)
}

/// The daemon's decision on a `Hello`, given the app it launched for that
/// process, if any.
pub fn answer_hello(version: Version, launched_app: Option<u32>) -> Response {
    if version.major != PROTOCOL_VERSION.major {
        return Response::Refused {
            reason: RefusalReason::IncompatibleVersion,
        };
    }
    match launched_app {
        Some(app_id) => Response::Session(Session {
            version: PROTOCOL_VERSION,
            app_id,
        }),
        None => Response::Refused {
            reason: RefusalReason::UnknownProcess,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_round_trips_as_one_line() -> Result<(), FrameError> {
        let message = ShimMessage {
            id: 1,
            request: Request::Hello {
                version: PROTOCOL_VERSION,
                pid: 4242,
            },
        };
        let line = encode(&message)?;
        assert_eq!(
            line,
            "{\"id\":1,\"request\":{\"type\":\"hello\",\"version\":{\"major\":0,\"minor\":4},\"pid\":4242}}\n"
        );
        assert_eq!(decode::<ShimMessage>(&line)?, message);
        Ok(())
    }

    #[test]
    fn session_and_refusal_round_trip() -> Result<(), FrameError> {
        for response in [
            answer_hello(PROTOCOL_VERSION, Some(2_143_030)),
            answer_hello(PROTOCOL_VERSION, None),
            answer_hello(Version { major: 1, minor: 0 }, Some(480)),
        ] {
            let message = DaemonMessage::Response { id: 7, response };
            assert_eq!(decode::<DaemonMessage>(&encode(&message)?)?, message);
        }
        let line = encode(&DaemonMessage::Response {
            id: 7,
            response: answer_hello(PROTOCOL_VERSION, Some(2_143_030)),
        })?;
        assert_eq!(
            line,
            "{\"id\":7,\"response\":{\"type\":\"session\",\"version\":{\"major\":0,\"minor\":4},\"app_id\":2143030}}\n"
        );
        Ok(())
    }

    #[test]
    fn ownership_messages_round_trip_as_one_line() -> Result<(), FrameError> {
        let request = ShimMessage {
            id: 2,
            request: Request::AppOwnership { app_id: 2_444_550 },
        };
        let line = encode(&request)?;
        assert_eq!(
            line,
            "{\"id\":2,\"request\":{\"type\":\"app_ownership\",\"app_id\":2444550}}\n"
        );
        assert_eq!(decode::<ShimMessage>(&line)?, request);

        let owned = DaemonMessage::Response {
            id: 2,
            response: Response::Ownership { owned: true },
        };
        let line = encode(&owned)?;
        assert_eq!(
            line,
            "{\"id\":2,\"response\":{\"type\":\"ownership\",\"owned\":true}}\n"
        );
        assert_eq!(decode::<DaemonMessage>(&line)?, owned);

        for reason in [
            UnavailableReason::NotSignedIn,
            UnavailableReason::Offline,
            UnavailableReason::NotAllowed,
            UnavailableReason::TooLarge,
            UnavailableReason::NotReceived,
        ] {
            let message = DaemonMessage::Response {
                id: 9,
                response: Response::Unavailable { reason },
            };
            assert_eq!(decode::<DaemonMessage>(&encode(&message)?)?, message);
        }
        assert_eq!(
            encode(&DaemonMessage::Response {
                id: 9,
                response: Response::Unavailable {
                    reason: UnavailableReason::NotAllowed
                },
            })?,
            "{\"id\":9,\"response\":{\"type\":\"unavailable\",\"reason\":\"not_allowed\"}}\n"
        );
        Ok(())
    }

    #[test]
    fn stats_messages_round_trip_as_one_line() -> Result<(), FrameError> {
        let request = ShimMessage {
            id: 5,
            request: Request::RequestStats,
        };
        let line = encode(&request)?;
        assert_eq!(
            line,
            "{\"id\":5,\"request\":{\"type\":\"request_stats\"}}\n"
        );
        assert_eq!(decode::<ShimMessage>(&line)?, request);

        let accepted = DaemonMessage::Response {
            id: 5,
            response: Response::Accepted,
        };
        let line = encode(&accepted)?;
        assert_eq!(line, "{\"id\":5,\"response\":{\"type\":\"accepted\"}}\n");
        assert_eq!(decode::<DaemonMessage>(&line)?, accepted);

        let received = DaemonMessage::Event {
            event: Event::StatsReceived {
                result: 1,
                achievement_names: vec![AchievementName {
                    name: "PET_1".to_owned(),
                    display_name: "Bongo Beat 1".to_owned(),
                    position: Some(SchemaPosition { stat_id: 2, bit: 0 }),
                }],
            },
        };
        let line = encode(&received)?;
        assert_eq!(
            line,
            "{\"event\":{\"type\":\"stats_received\",\"result\":1,\"achievement_names\":[{\"name\":\"PET_1\",\"display_name\":\"Bongo Beat 1\",\"position\":{\"stat_id\":2,\"bit\":0}}]}}\n"
        );
        assert_eq!(decode::<DaemonMessage>(&line)?, received);

        // A 0.3 daemon sends no position: it reads as `None`, and `None` is
        // not written.
        let older = "{\"event\":{\"type\":\"stats_received\",\"result\":1,\"achievement_names\":[{\"name\":\"PET_1\",\"display_name\":\"Bongo Beat 1\"}]}}\n";
        let without = DaemonMessage::Event {
            event: Event::StatsReceived {
                result: 1,
                achievement_names: vec![AchievementName {
                    name: "PET_1".to_owned(),
                    display_name: "Bongo Beat 1".to_owned(),
                    position: None,
                }],
            },
        };
        assert_eq!(decode::<DaemonMessage>(older)?, without);
        assert_eq!(encode(&without)?, older);

        let refused = DaemonMessage::Event {
            event: Event::StatsReceived {
                result: 15,
                achievement_names: Vec::new(),
            },
        };
        assert_eq!(decode::<DaemonMessage>(&encode(&refused)?)?, refused);
        Ok(())
    }

    #[test]
    fn shadow_messages_round_trip_as_one_line() -> Result<(), FrameError> {
        for (request, text) in [
            (
                Request::RegisterShadow { app_id: 332_950 },
                "{\"id\":6,\"request\":{\"type\":\"register_shadow\",\"app_id\":332950}}\n",
            ),
            (
                Request::CancelShadow,
                "{\"id\":6,\"request\":{\"type\":\"cancel_shadow\"}}\n",
            ),
            (
                Request::ShadowHello {
                    version: PROTOCOL_VERSION,
                    pid: 4242,
                    app_id: 332_950,
                },
                "{\"id\":6,\"request\":{\"type\":\"shadow_hello\",\"version\":{\"major\":0,\"minor\":4},\"pid\":4242,\"app_id\":332950}}\n",
            ),
        ] {
            let message = ShimMessage { id: 6, request };
            let line = encode(&message)?;
            assert_eq!(line, text);
            assert_eq!(decode::<ShimMessage>(&line)?, message);
        }
        Ok(())
    }

    /// SHA-256 of `foo` (not anyone's name).
    const DIGEST: &str = "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae";

    #[test]
    fn persona_name_messages_round_trip_as_one_line() -> Result<(), FrameError> {
        let sha256 = NameDigest::from_hex(DIGEST).unwrap_or_else(|| panic!("no digest"));
        let message = ShimMessage {
            id: 8,
            request: Request::ComparePersonaName { sha256 },
        };
        let line = encode(&message)?;
        assert_eq!(
            line,
            format!(
                "{{\"id\":8,\"request\":{{\"type\":\"compare_persona_name\",\"sha256\":\"{DIGEST}\"}}}}\n"
            )
        );
        assert_eq!(decode::<ShimMessage>(&line)?, message);
        let answer = DaemonMessage::Response {
            id: 8,
            response: Response::PersonaNameCompared { matches: true },
        };
        let line = encode(&answer)?;
        assert_eq!(
            line,
            "{\"id\":8,\"response\":{\"type\":\"persona_name_compared\",\"matches\":true}}\n"
        );
        assert_eq!(decode::<DaemonMessage>(&line)?, answer);
        let missing = DaemonMessage::Response {
            id: 8,
            response: Response::Unavailable {
                reason: UnavailableReason::NotReceived,
            },
        };
        assert_eq!(
            encode(&missing)?,
            "{\"id\":8,\"response\":{\"type\":\"unavailable\",\"reason\":\"not_received\"}}\n"
        );
        Ok(())
    }

    #[test]
    fn a_0_3_reader_ignores_the_position() -> Result<(), FrameError> {
        // The shape of `AchievementName` in protocol 0.3, without the field
        // of 0.4: a 0.4 line still reads (serde skips unknown fields).
        #[derive(Debug, PartialEq, Deserialize)]
        struct OldName {
            name: String,
            display_name: String,
        }
        let line = encode(&AchievementName {
            name: "PET_1".to_owned(),
            display_name: "Bongo Beat 1".to_owned(),
            position: Some(SchemaPosition { stat_id: 2, bit: 7 }),
        })?;
        assert_eq!(
            decode::<OldName>(&line)?,
            OldName {
                name: "PET_1".to_owned(),
                display_name: "Bongo Beat 1".to_owned(),
            }
        );
        Ok(())
    }

    #[test]
    fn only_0_4_daemons_send_schema_positions() {
        assert!(PROTOCOL_VERSION.has_schema_positions());
        assert!(!Version { major: 0, minor: 3 }.has_schema_positions());
        assert!(Version { major: 0, minor: 5 }.has_schema_positions());
        assert!(!Version { major: 1, minor: 4 }.has_schema_positions());
    }

    #[test]
    fn only_0_3_daemons_compare_persona_names() {
        assert!(PROTOCOL_VERSION.has_persona_name_comparison());
        assert!(!Version { major: 0, minor: 2 }.has_persona_name_comparison());
        assert!(Version { major: 0, minor: 4 }.has_persona_name_comparison());
    }

    #[test]
    fn a_digest_is_64_hex_digits_and_never_printed() {
        let upper = DIGEST.to_ascii_uppercase();
        assert_eq!(
            NameDigest::from_hex(&upper).map(|digest| digest.as_hex().to_owned()),
            Some(DIGEST.to_owned())
        );
        assert!(NameDigest::from_hex(&DIGEST[1..]).is_none(), "too short");
        assert!(NameDigest::from_hex(&format!("{}g", &DIGEST[1..])).is_none());
        assert!(NameDigest::from_hex("Alice Example").is_none());
        assert!(
            decode::<ShimMessage>(
                "{\"id\":1,\"request\":{\"type\":\"compare_persona_name\",\"sha256\":\"Alice\"}}"
            )
            .is_err(),
            "only digests pass"
        );
        let request = Request::ComparePersonaName {
            sha256: NameDigest::from_hex(DIGEST).unwrap_or_else(|| panic!("no digest")),
        };
        let shown = format!("{request:?}");
        assert!(!shown.contains(&DIGEST[..8]), "{shown}");
    }

    #[test]
    fn no_message_of_0_2_writes() {
        for request in [
            Request::Hello {
                version: PROTOCOL_VERSION,
                pid: 1,
            },
            Request::AppOwnership { app_id: 1 },
            Request::RequestStats,
            Request::CloudFileSize {
                name: "a".to_owned(),
            },
            Request::CloudRead {
                name: "a".to_owned(),
                max: 1,
            },
            Request::RegisterShadow { app_id: 1 },
            Request::CancelShadow,
            Request::ShadowHello {
                version: PROTOCOL_VERSION,
                pid: 1,
                app_id: 1,
            },
            Request::ComparePersonaName {
                sha256: NameDigest::from_hex(DIGEST).unwrap_or_else(|| panic!("no digest")),
            },
        ] {
            assert!(!request.writes(), "{request:?}");
        }
    }

    #[test]
    fn cloud_read_messages_round_trip_as_one_line() -> Result<(), FrameError> {
        for (request, text) in [
            (
                Request::CloudFileSize {
                    name: "MainPlayerData.json".to_owned(),
                },
                "{\"id\":8,\"request\":{\"type\":\"cloud_file_size\",\"name\":\"MainPlayerData.json\"}}\n",
            ),
            (
                Request::CloudRead {
                    name: "MainPlayerData.json".to_owned(),
                    max: 4096,
                },
                "{\"id\":8,\"request\":{\"type\":\"cloud_read\",\"name\":\"MainPlayerData.json\",\"max\":4096}}\n",
            ),
        ] {
            let message = ShimMessage { id: 8, request };
            let line = encode(&message)?;
            assert_eq!(line, text);
            assert_eq!(decode::<ShimMessage>(&line)?, message);
        }
        for (response, text) in [
            (
                Response::FileSize { size: Some(1234) },
                "{\"id\":8,\"response\":{\"type\":\"file_size\",\"size\":1234}}\n",
            ),
            (
                Response::FileSize { size: None },
                "{\"id\":8,\"response\":{\"type\":\"file_size\",\"size\":null}}\n",
            ),
            (
                Response::FileData {
                    data: Some(Bytes(b"{\"a\":1}\n".to_vec())),
                },
                "{\"id\":8,\"response\":{\"type\":\"file_data\",\"data\":\"eyJhIjoxfQo=\"}}\n",
            ),
            (
                Response::FileData { data: None },
                "{\"id\":8,\"response\":{\"type\":\"file_data\",\"data\":null}}\n",
            ),
        ] {
            let message = DaemonMessage::Response { id: 8, response };
            let line = encode(&message)?;
            assert_eq!(line, text);
            assert_eq!(decode::<DaemonMessage>(&line)?, message);
        }
        Ok(())
    }

    #[test]
    fn base64_follows_rfc_4648() {
        // The test vectors of RFC 4648, section 10.
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64::encode(plain.as_bytes()), coded);
            assert_eq!(base64::decode(coded), Some(plain.as_bytes().to_vec()));
        }
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(base64::decode(&base64::encode(&all)), Some(all));
        for bad in [
            "Zg", "Zg=", "Z===", "====", "Zg==Zm8=", "Zh==", "Zm9=", "Zm 9", "Zm9v\n", "Zm-_",
        ] {
            assert_eq!(base64::decode(bad), None, "{bad}");
        }
        assert!(
            decode::<DaemonMessage>(
                "{\"id\":8,\"response\":{\"type\":\"file_data\",\"data\":\"Zh==\"}}"
            )
            .is_err()
        );
    }

    #[test]
    fn file_data_debug_shows_only_the_length() {
        let response = Response::FileData {
            data: Some(Bytes(b"secret save".to_vec())),
        };
        let text = format!("{response:?}");
        assert!(!text.contains("secret"), "{text}");
        assert!(text.contains("11 bytes"), "{text}");
    }

    #[test]
    fn an_event_is_not_a_response() {
        // An event line has no `id`; a line with neither is invalid.
        assert!(matches!(
            decode::<DaemonMessage>(
                "{\"event\":{\"type\":\"stats_received\",\"result\":1,\"achievement_names\":[]}}"
            ),
            Ok(DaemonMessage::Event { .. })
        ));
        assert!(decode::<DaemonMessage>("{\"response\":{\"type\":\"accepted\"}}").is_err());
        assert!(decode::<DaemonMessage>("{\"event\":{\"type\":\"stats_stored\"}}").is_err());
    }

    #[test]
    fn a_0_1_hello_still_gets_a_session() -> Result<(), FrameError> {
        // Minor versions only add messages, so a 0.1 shim keeps working.
        let message: ShimMessage = decode(
            "{\"id\":1,\"request\":{\"type\":\"hello\",\"version\":{\"major\":0,\"minor\":1},\"pid\":7}}",
        )?;
        let Request::Hello { version, pid } = message.request else {
            panic!("not a hello: {message:?}");
        };
        assert_eq!(pid, 7);
        assert!(matches!(
            answer_hello(version, Some(480)),
            Response::Session(_)
        ));
        Ok(())
    }

    #[test]
    fn hello_is_refused_for_other_versions_and_unknown_processes() {
        assert_eq!(
            answer_hello(Version { major: 1, minor: 0 }, Some(480)),
            Response::Refused {
                reason: RefusalReason::IncompatibleVersion
            }
        );
        assert_eq!(
            answer_hello(Version { major: 0, minor: 9 }, None),
            Response::Refused {
                reason: RefusalReason::UnknownProcess
            }
        );
        assert!(matches!(
            answer_hello(Version { major: 0, minor: 9 }, Some(480)),
            Response::Session(Session { app_id: 480, .. })
        ));
    }

    #[test]
    fn rejects_broken_lines() {
        assert!(matches!(
            decode::<ShimMessage>("{\"id\":1}\n{\"id\":2}"),
            Err(FrameError::MultipleLines)
        ));
        assert!(matches!(
            decode::<ShimMessage>("not json"),
            Err(FrameError::Invalid(_))
        ));
        assert!(
            decode::<ShimMessage>(
                "{\"id\":1,\"request\":{\"type\":\"hello\",\"version\":{\"major\":0,\"minor\":1},\"pid\":1}}\r\n"
            )
            .is_ok()
        );
    }
}
