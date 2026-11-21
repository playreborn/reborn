//! Accepting shim connections and answering them (ADR 0047).
//!
//! One JSON object per line (`reborn-shim-protocol`). So far a shim can
//! only say `Hello` and gets its session: the app the daemon launched for
//! its process ID, or a refusal.

use std::sync::Arc;

use reborn_ipc::socket::LocalSocketStream;
use reborn_shim_protocol::{DaemonMessage, Request, ShimMessage, answer_hello, decode, encode};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tracing::{debug, info, warn};

use crate::daemon::Daemon;

/// Longest line read from a shim; anything longer closes the connection.
const MAX_LINE: usize = 64 * 1024;

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
        let Some(reply) = answer(&line, |pid| daemon.launched_app(pid)) else {
            debug!("closing shim connection after a bad line");
            break;
        };
        if write.write_all(reply.as_bytes()).await.is_err() {
            break;
        }
    }
}

/// The reply line to one line from a shim, `None` if the line is not a
/// valid message. `launched_app` gives the app the daemon launched for a
/// process ID.
fn answer(line: &str, launched_app: impl Fn(u32) -> Option<u32>) -> Option<String> {
    let message: ShimMessage = decode(line).ok()?;
    let response = match message.request {
        Request::Hello { version, pid } => {
            let response = answer_hello(version, launched_app(pid));
            info!(pid, ?response, "shim said hello");
            response
        }
    };
    encode(&DaemonMessage::Response {
        id: message.id,
        response,
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reborn_shim_protocol::{
        PROTOCOL_VERSION, RefusalReason, Response, Session, Version, decode,
    };

    fn hello(pid: u32, version: Version) -> String {
        encode(&ShimMessage {
            id: 3,
            request: Request::Hello { version, pid },
        })
        .unwrap_or_default()
    }

    fn launched(pid: u32) -> Option<u32> {
        (pid == 100).then_some(2_143_030)
    }

    #[test]
    fn a_launched_process_gets_its_session() {
        let reply = answer(&hello(100, PROTOCOL_VERSION), launched).unwrap_or_default();
        assert_eq!(
            decode::<DaemonMessage>(&reply).ok(),
            Some(DaemonMessage::Response {
                id: 3,
                response: Response::Session(Session {
                    version: PROTOCOL_VERSION,
                    app_id: 2_143_030,
                }),
            })
        );
    }

    #[test]
    fn other_processes_and_versions_are_refused() {
        let reply = answer(&hello(101, PROTOCOL_VERSION), launched).unwrap_or_default();
        assert!(reply.contains("unknown_process"), "{reply}");
        let reply =
            answer(&hello(100, Version { major: 9, minor: 0 }), launched).unwrap_or_default();
        assert_eq!(
            decode::<DaemonMessage>(&reply).ok(),
            Some(DaemonMessage::Response {
                id: 3,
                response: Response::Refused {
                    reason: RefusalReason::IncompatibleVersion
                },
            })
        );
    }

    #[test]
    fn bad_lines_get_no_reply() {
        assert_eq!(answer("not json\n", launched), None);
        assert_eq!(answer("{\"id\":1}\n", launched), None);
    }
}
