//! Accepting frontend connections and speaking the IPC protocol.

use std::sync::Arc;

use reborn_ipc::frame::{read_frame, write_frame};
use reborn_ipc::protocol::{
    ClientFrame, LAUNCH_QUESTIONS_MINOR, PROTOCOL_MAJOR, Request, Response, SHIM_MODE_NOTICE,
    ServerFrame,
};
use reborn_ipc::socket::LocalSocketStream;
use tokio::io::BufReader;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

use crate::daemon::{self, Daemon};

/// Accepts connections until the listener fails.
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
            Err(error) => warn!(%error, "accepting a connection failed"),
        }
    }
}

/// Serves one frontend until it disconnects.
async fn serve(daemon: Arc<Daemon>, stream: LocalSocketStream) {
    let stream = Arc::new(stream);
    let (frames, outgoing) = mpsc::unbounded_channel::<ServerFrame>();
    let writer = tokio::spawn(write_loop(
        Arc::clone(&stream),
        outgoing,
        daemon.subscribe(),
    ));

    let mut reader = BufReader::new(&*stream);
    let mut greeted = false;
    let mut client_minor = 0;
    loop {
        let frame: ClientFrame = match read_frame(&mut reader).await {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(error) => {
                debug!(%error, "closing connection after a bad frame");
                break;
            }
        };
        if greeted {
            // Requests run concurrently, so a frontend can answer the Steam
            // Guard prompt while its sign-in request is still pending.
            let daemon = Arc::clone(&daemon);
            let frames = frames.clone();
            tokio::spawn(async move {
                let response = for_client(daemon.handle(frame.request).await, client_minor);
                // The frontend may be gone by now; nothing to do then.
                let _ = frames.send(ServerFrame::Response {
                    id: frame.id,
                    response,
                });
            });
            continue;
        }
        let response = {
            match frame.request {
                Request::Hello {
                    major,
                    minor,
                    client,
                } => {
                    if major == PROTOCOL_MAJOR {
                        greeted = true;
                        client_minor = minor;
                        info!(%client, "frontend connected");
                    } else {
                        info!(%client, major, "frontend with incompatible protocol");
                    }
                    // Incompatible clients see the daemon's major version and give up.
                    daemon::hello()
                }
                _ => Response::Error {
                    message: "the first request must be `hello`".to_owned(),
                },
            }
        };
        if frames
            .send(ServerFrame::Response {
                id: frame.id,
                response,
            })
            .is_err()
        {
            break;
        }
    }
    drop(frames);
    let _ = writer.await;
}

/// Turns answers a frontend's protocol version does not know into ones it
/// does. A frontend older than 1.13 can neither confirm the shim-mode
/// notice (ADR 0057) nor answer a cloud conflict (ADR 0055), so it gets
/// them as errors and the game stays stopped.
fn for_client(response: Response, client_minor: u16) -> Response {
    if client_minor >= LAUNCH_QUESTIONS_MINOR {
        return response;
    }
    match response {
        Response::ShimModeNotice { app_id } => Response::Error {
            message: format!(
                "app {app_id}: {SHIM_MODE_NOTICE} This frontend cannot ask you to confirm \
                 this; update it to start games in shim mode."
            ),
        },
        Response::CloudConflict { app_id, files } => Response::Error {
            message: format!(
                "app {app_id}: {} saves changed both in Reborn's folder and in Steam Cloud, \
                 so the game was not started and nothing was changed. This frontend cannot \
                 ask you which version to play with; update it to choose.",
                files.len()
            ),
        },
        other => other,
    }
}

/// Writes responses and events to the frontend.
async fn write_loop(
    stream: Arc<LocalSocketStream>,
    mut frames: mpsc::UnboundedReceiver<ServerFrame>,
    mut events: broadcast::Receiver<reborn_ipc::Event>,
) {
    loop {
        let frame = tokio::select! {
            frame = frames.recv() => match frame {
                Some(frame) => frame,
                None => break,
            },
            event = events.recv() => match event {
                Ok(event) => ServerFrame::Event { event },
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    debug!(skipped, "frontend too slow, skipped events");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
        };
        if let Err(error) = write_frame(&mut &*stream, &frame).await {
            debug!(%error, "frontend went away");
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_frontends_get_the_shim_notice_as_an_error() {
        let notice = || Response::ShimModeNotice { app_id: 480 };
        assert_eq!(
            for_client(notice(), LAUNCH_QUESTIONS_MINOR),
            Response::ShimModeNotice { app_id: 480 }
        );
        let Response::Error { message } = for_client(notice(), LAUNCH_QUESTIONS_MINOR - 1) else {
            panic!("expected an error");
        };
        assert!(
            message.contains(SHIM_MODE_NOTICE) && message.contains("app 480"),
            "{message}"
        );
        // Everything else passes unchanged.
        assert_eq!(
            for_client(Response::Launched { app_id: 480 }, 0),
            Response::Launched { app_id: 480 }
        );
    }

    #[test]
    fn old_frontends_get_a_cloud_conflict_as_an_error() {
        use reborn_ipc::protocol::CloudConflictFile;
        let conflict = || Response::CloudConflict {
            app_id: 480,
            files: vec![CloudConflictFile {
                name: "save.dat".to_owned(),
                local_sha1: "aa".repeat(20),
                local_size: 1,
                local_modified: None,
                cloud_sha1: "cc".repeat(20),
                cloud_size: 2,
                cloud_modified: 3,
                had_base: true,
            }],
        };
        assert_eq!(for_client(conflict(), LAUNCH_QUESTIONS_MINOR), conflict());
        let Response::Error { message } = for_client(conflict(), LAUNCH_QUESTIONS_MINOR - 1) else {
            panic!("expected an error");
        };
        assert!(
            message.contains("app 480")
                && message.contains("1 saves")
                && message.contains("not started"),
            "{message}"
        );
    }
}
