//! Accepting shim connections and answering them (ADR 0047, ADR 0055).
//!
//! One JSON object per line (`reborn-shim-protocol`). A shim first says
//! `Hello` and gets its session: the app the daemon launched for its
//! process ID, or a refusal. The connection is then bound to that process
//! for good: a later `Hello` for another process ID is refused
//! (`unknown_process`), and a refused `Hello` never clears the binding.
//! After that it may ask `AppOwnership`, answered from what `rebornd`
//! fetched from Valve for this run at launch (ADR 0055, stage 0):
//!
//! - every request is checked against the connection's session first (one
//!   process, one app); without one it gets `NotAllowed`;
//! - only the session's own app and the DLC its app info names
//!   (`depots/<id>/dlcappid`) are answered; every other app ID gets
//!   `NotAllowed`, even one the account owns (strict, as decided in
//!   ADR 0055);
//! - a run started without shim mode has no ownership data and gets
//!   `NotAllowed`;
//! - no request here reaches Valve: the answer comes from the run's set,
//!   so it keeps working while the connection to Valve is lost.
//!
//! Whether an app is owned is account data: it is never logged.

use std::collections::BTreeSet;
use std::sync::Arc;

use reborn_ipc::socket::LocalSocketStream;
use reborn_shim_protocol::{
    DaemonMessage, RefusalReason, Request, Response, ShimMessage, UnavailableReason, answer_hello,
    decode, encode,
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info, warn};

use crate::daemon::Daemon;

/// Longest line read from a shim; anything longer closes the connection.
/// No message of protocol 0.2 comes near it; ADR 0055 raises it per
/// session from the app's cloud quota once cloud writes exist.
const MAX_LINE: usize = 64 * 1024;

/// A game process `rebornd` launched, as the shim channel sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShimRun {
    /// The app the process was launched for.
    pub app_id: u32,
    /// Ownership data fetched from Valve at launch; `None` for a run that
    /// was not started in shim mode.
    pub ownership: Option<Arc<RunOwnership>>,
}

/// What the run may learn about ownership (ADR 0055, "What is cached"):
/// kept in memory for the run only, never written or logged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOwnership {
    /// The app IDs the account's active licences grant, base package
    /// included, from the Valve session the game was launched in
    /// (`reborn_cm::Session::licensed_app_ids`).
    pub owned: BTreeSet<u32>,
    /// The DLC app IDs the own app's info names
    /// (`reborn_core::launch::LaunchPlan::dlc_app_ids`).
    pub dlc: BTreeSet<u32>,
}

/// State of one shim connection.
#[derive(Debug, Default)]
struct Connection {
    /// The process this connection got a session for.
    pid: Option<u32>,
}

/// Accepts shim connections until the listener fails.
pub async fn accept_loop(
    listener: interprocess::local_socket::tokio::Listener,
    daemon: Arc<Daemon>,
) {
    use interprocess::local_socket::traits::tokio::Listener as _;
    loop {
        match listener.accept().await {
            Ok(stream) => {
                tokio::spawn(serve(Arc::clone(&daemon), stream));
            }
            Err(error) => warn!(%error, "accepting a shim connection failed"),
        }
    }
}

/// Serves one shim until it disconnects or sends something it should not.
async fn serve(daemon: Arc<Daemon>, stream: LocalSocketStream) {
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read);
    let mut line = String::new();
    let mut connection = Connection::default();
    loop {
        line.clear();
        match AsyncReadExt::take(&mut lines, MAX_LINE as u64)
            .read_line(&mut line)
            .await
        {
            Ok(0) => break,
            Ok(_) if !line.ends_with('\n') => {
                debug!("closing shim connection after an overlong line");
                break;
            }
            Ok(_) => {}
            Err(error) => {
                debug!(%error, "closing shim connection");
                break;
            }
        }
        let Some(reply) = answer(&line, &mut connection, |pid| daemon.shim_run(pid)) else {
            debug!("closing shim connection after a bad line");
            break;
        };
        if write.write_all(reply.as_bytes()).await.is_err() {
            break;
        }
    }
}

/// The reply line to one line from a shim, `None` if the line is not a
/// valid message. `run` gives the run the daemon launched for a process
/// ID, while it runs.
fn answer(
    line: &str,
    connection: &mut Connection,
    run: impl Fn(u32) -> Option<ShimRun>,
) -> Option<String> {
    let message: ShimMessage = decode(line).ok()?;
    let response = match message.request {
        Request::Hello { version, pid } => {
            let response = match connection.pid {
                // Bound to another process: refused, the binding stays.
                Some(bound) if bound != pid => Response::Refused {
                    reason: RefusalReason::UnknownProcess,
                },
                _ => answer_hello(version, run(pid).map(|run| run.app_id)),
            };
            info!(pid, ?response, "shim said hello");
            // Only the first successful hello binds; a refused one never
            // clears a binding.
            if connection.pid.is_none() && matches!(response, Response::Session(_)) {
                connection.pid = Some(pid);
            }
            response
        }
        Request::AppOwnership { app_id } => {
            let response = answer_ownership(connection.pid.and_then(&run).as_ref(), app_id);
            match &response {
                Response::Unavailable { reason } => {
                    info!(pid = ?connection.pid, app_id, ?reason, "shim ownership request refused");
                }
                _ => debug!(pid = ?connection.pid, app_id, "shim ownership request answered"),
            }
            response
        }
    };
    encode(&DaemonMessage::Response {
        id: message.id,
        response,
    })
    .ok()
}

/// The answer to `AppOwnership { app_id }` for the connection's run
/// (ADR 0055): the session check first, then the app check, then the
/// run's data.
fn answer_ownership(run: Option<&ShimRun>, app_id: u32) -> Response {
    let not_allowed = Response::Unavailable {
        reason: UnavailableReason::NotAllowed,
    };
    let Some(run) = run else {
        return not_allowed;
    };
    let Some(ownership) = run.ownership.as_deref() else {
        return not_allowed;
    };
    if app_id != run.app_id && !ownership.dlc.contains(&app_id) {
        return not_allowed;
    }
    Response::Ownership {
        owned: ownership.owned.contains(&app_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reborn_shim_protocol::{
        PROTOCOL_VERSION, RefusalReason, Response, Session, Version, decode,
    };

    /// Cave Crawlers and the two DLC IDs its recorded app info lists
    /// (`crates/formats/tests/data/appinfo_2143030.vdf`, `listofdlc`),
    /// used here only as numbers: the fake run names them as DLC.
    const GAME: u32 = 2_143_030;
    const DLC: u32 = 2_444_550;
    const OTHER_DLC: u32 = 2_444_553;
    /// An app the fake account owns that is not the game's business.
    const OTHER_OWNED: u32 = 3_419_430;

    fn line(id: u64, request: Request) -> String {
        encode(&ShimMessage { id, request }).unwrap_or_default()
    }

    fn hello(pid: u32, version: Version) -> String {
        line(3, Request::Hello { version, pid })
    }

    fn ownership(app_id: u32) -> String {
        line(4, Request::AppOwnership { app_id })
    }

    /// Process 100 runs the game in shim mode, 101 without shim mode.
    fn launched(pid: u32) -> Option<ShimRun> {
        match pid {
            100 => Some(ShimRun {
                app_id: GAME,
                ownership: Some(Arc::new(RunOwnership {
                    owned: BTreeSet::from([GAME, OTHER_DLC, OTHER_OWNED]),
                    dlc: BTreeSet::from([DLC, OTHER_DLC]),
                })),
            }),
            101 => Some(ShimRun {
                app_id: GAME,
                ownership: None,
            }),
            _ => None,
        }
    }

    fn reply(line: &str, connection: &mut Connection) -> Option<DaemonMessage> {
        decode::<DaemonMessage>(&answer(line, connection, launched)?).ok()
    }

    fn response(id: u64, response: Response) -> Option<DaemonMessage> {
        Some(DaemonMessage::Response { id, response })
    }

    fn not_allowed() -> Option<DaemonMessage> {
        response(
            4,
            Response::Unavailable {
                reason: UnavailableReason::NotAllowed,
            },
        )
    }

    #[test]
    fn a_launched_process_gets_its_session() {
        let mut connection = Connection::default();
        assert_eq!(
            reply(&hello(100, PROTOCOL_VERSION), &mut connection),
            response(
                3,
                Response::Session(Session {
                    version: PROTOCOL_VERSION,
                    app_id: GAME,
                })
            )
        );
        assert_eq!(connection.pid, Some(100));
    }

    #[test]
    fn other_processes_and_versions_are_refused() {
        let mut connection = Connection::default();
        let text =
            answer(&hello(102, PROTOCOL_VERSION), &mut connection, launched).unwrap_or_default();
        assert!(text.contains("unknown_process"), "{text}");
        assert_eq!(
            reply(&hello(100, Version { major: 9, minor: 0 }), &mut connection),
            response(
                3,
                Response::Refused {
                    reason: RefusalReason::IncompatibleVersion
                }
            )
        );
        assert_eq!(connection.pid, None);
    }

    #[test]
    fn the_game_and_its_dlc_get_ownership_answers() {
        let mut connection = Connection::default();
        reply(&hello(100, PROTOCOL_VERSION), &mut connection);
        assert_eq!(
            reply(&ownership(GAME), &mut connection),
            response(4, Response::Ownership { owned: true })
        );
        assert_eq!(
            reply(&ownership(OTHER_DLC), &mut connection),
            response(4, Response::Ownership { owned: true })
        );
        assert_eq!(
            reply(&ownership(DLC), &mut connection),
            response(4, Response::Ownership { owned: false })
        );
    }

    #[test]
    fn other_apps_are_refused_even_when_owned() {
        let mut connection = Connection::default();
        reply(&hello(100, PROTOCOL_VERSION), &mut connection);
        assert_eq!(
            reply(&ownership(OTHER_OWNED), &mut connection),
            not_allowed()
        );
        assert_eq!(reply(&ownership(730), &mut connection), not_allowed());
    }

    #[test]
    fn ownership_needs_a_session_on_this_connection() {
        let mut connection = Connection::default();
        assert_eq!(reply(&ownership(GAME), &mut connection), not_allowed());
        // A refused hello binds nothing.
        reply(&hello(102, PROTOCOL_VERSION), &mut connection);
        assert_eq!(reply(&ownership(GAME), &mut connection), not_allowed());
    }

    #[test]
    fn a_run_without_shim_mode_or_that_ended_gets_no_ownership() {
        let mut connection = Connection::default();
        reply(&hello(101, PROTOCOL_VERSION), &mut connection);
        assert_eq!(connection.pid, Some(101));
        assert_eq!(reply(&ownership(GAME), &mut connection), not_allowed());

        // The game exited: the daemon no longer knows the process.
        let mut connection = Connection { pid: Some(100) };
        let ended = |_pid: u32| -> Option<ShimRun> { None };
        let text = answer(&ownership(GAME), &mut connection, ended).unwrap_or_default();
        assert_eq!(decode::<DaemonMessage>(&text).ok(), not_allowed());
    }

    #[test]
    fn a_connection_stays_bound_to_its_first_process() {
        let mut connection = Connection::default();
        reply(&hello(100, PROTOCOL_VERSION), &mut connection);
        assert_eq!(connection.pid, Some(100));

        // Another launched process on the same connection: refused.
        assert_eq!(
            reply(&hello(101, PROTOCOL_VERSION), &mut connection),
            response(
                3,
                Response::Refused {
                    reason: RefusalReason::UnknownProcess
                }
            )
        );
        assert_eq!(connection.pid, Some(100));

        // A refused hello for the bound process keeps the binding.
        reply(&hello(100, Version { major: 9, minor: 0 }), &mut connection);
        assert_eq!(connection.pid, Some(100));
        assert_eq!(
            reply(&ownership(GAME), &mut connection),
            response(4, Response::Ownership { owned: true })
        );

        // The same process may say hello again and gets its session.
        assert!(matches!(
            reply(&hello(100, PROTOCOL_VERSION), &mut connection),
            Some(DaemonMessage::Response {
                response: Response::Session(_),
                ..
            })
        ));
        assert_eq!(connection.pid, Some(100));
    }

    #[test]
    fn bad_lines_get_no_reply() {
        let mut connection = Connection::default();
        assert_eq!(answer("not json\n", &mut connection, launched), None);
        assert_eq!(answer("{\"id\":1}\n", &mut connection, launched), None);
    }
}
