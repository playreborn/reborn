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
//! (in a [`DaemonMessage::Event`], which has no `id`). The other messages
//! of ADR 0055 (stores, cloud) follow in later stage-0 changes under the
//! same version, before any release.
//!
//! This crate holds only message types and framing (MIT OR Apache-2.0), so
//! the shim never depends on GPL crates (AGENTS.md section 3).
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// The protocol version in [`Request::Hello`]. A daemon refuses a shim
/// with another major version.
pub const PROTOCOL_VERSION: Version = Version { major: 0, minor: 2 };

/// A protocol version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// Changes when old peers can no longer understand new messages.
    pub major: u32,
    /// Changes when messages are only added.
    pub minor: u32,
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
    /// The request is taken; its result follows as an event (for
    /// [`Request::RequestStats`]: [`Event::StatsReceived`]).
    Accepted,
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
    /// The daemon did not launch this process.
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
            "{\"id\":1,\"request\":{\"type\":\"hello\",\"version\":{\"major\":0,\"minor\":2},\"pid\":4242}}\n"
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
            "{\"id\":7,\"response\":{\"type\":\"session\",\"version\":{\"major\":0,\"minor\":2},\"app_id\":2143030}}\n"
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
                }],
            },
        };
        let line = encode(&received)?;
        assert_eq!(
            line,
            "{\"event\":{\"type\":\"stats_received\",\"result\":1,\"achievement_names\":[{\"name\":\"PET_1\",\"display_name\":\"Bongo Beat 1\"}]}}\n"
        );
        assert_eq!(decode::<DaemonMessage>(&line)?, received);

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
