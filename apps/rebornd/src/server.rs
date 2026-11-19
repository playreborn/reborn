//! Accepting frontend connections and speaking the IPC protocol.

use std::sync::Arc;

use reborn_ipc::frame::{read_frame, write_frame};
use reborn_ipc::protocol::{ClientFrame, PROTOCOL_MAJOR, Request, Response, ServerFrame};
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
                let response = daemon.handle(frame.request).await;
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
                Request::Hello { major, client, .. } => {
                    if major == PROTOCOL_MAJOR {
                        greeted = true;
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
