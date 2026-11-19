//! A connection to `rebornd`, for frontends.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::BufReader;
use tokio::sync::{mpsc, oneshot};
use tracing::debug;

use crate::frame::{FrameError, read_frame, write_frame};
use crate::protocol::{
    ClientFrame, Event, PROTOCOL_MAJOR, PROTOCOL_MINOR, Request, Response, ServerFrame,
};
use crate::socket::{self, LocalSocketStream};

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>;

/// Talking to the daemon failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ClientError {
    /// No daemon is listening (or it cannot be reached).
    #[error("cannot reach rebornd (is it running?): {0}")]
    Connect(std::io::Error),
    /// Sending failed.
    #[error(transparent)]
    Frame(#[from] FrameError),
    /// The daemon speaks an incompatible protocol version.
    #[error(
        "rebornd speaks protocol {daemon_major}.x, this frontend {}.x",
        PROTOCOL_MAJOR
    )]
    Incompatible {
        /// The daemon's major version.
        daemon_major: u16,
    },
    /// The daemon refused the connection.
    #[error("rebornd refused the connection: {0}")]
    Refused(String),
    /// The connection closed before an answer arrived.
    #[error("rebornd closed the connection")]
    Closed,
}

/// A connection to `rebornd`. Requests can be sent from several tasks at
/// once; events are read with [`Client::next_event`].
pub struct Client {
    stream: Arc<LocalSocketStream>,
    /// Serializes writers so frames never interleave.
    write_lock: tokio::sync::Mutex<()>,
    pending: Pending,
    next_id: AtomicU64,
    events: tokio::sync::Mutex<mpsc::UnboundedReceiver<Event>>,
    daemon_version: String,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("daemon_version", &self.daemon_version)
            .finish_non_exhaustive()
    }
}

impl Client {
    /// Connects to the current user's daemon and performs the hello.
    /// `client_name` identifies the frontend in the daemon's logs.
    pub async fn connect(client_name: &str) -> Result<Self, ClientError> {
        let stream = socket::connect().await.map_err(ClientError::Connect)?;
        let stream = Arc::new(stream);
        let pending: Pending = Arc::default();
        let (event_sender, events) = mpsc::unbounded_channel();
        tokio::spawn(read_loop(
            Arc::clone(&stream),
            Arc::clone(&pending),
            event_sender,
        ));

        let mut client = Self {
            stream,
            write_lock: tokio::sync::Mutex::new(()),
            pending,
            next_id: AtomicU64::new(1),
            events: tokio::sync::Mutex::new(events),
            daemon_version: String::new(),
        };
        let hello = Request::Hello {
            major: PROTOCOL_MAJOR,
            minor: PROTOCOL_MINOR,
            client: client_name.to_owned(),
        };
        match client.request(hello).await? {
            Response::Hello {
                major,
                daemon_version,
                ..
            } if major == PROTOCOL_MAJOR => {
                client.daemon_version = daemon_version;
                Ok(client)
            }
            Response::Hello { major, .. } => Err(ClientError::Incompatible {
                daemon_major: major,
            }),
            Response::Error { message } => Err(ClientError::Refused(message)),
            other => Err(ClientError::Refused(format!("unexpected answer {other:?}"))),
        }
    }

    /// The daemon's version from the hello.
    pub fn daemon_version(&self) -> &str {
        &self.daemon_version
    }

    /// Sends a request and waits for its response. A [`Response::Error`]
    /// is returned as a normal response.
    pub async fn request(&self, request: Request) -> Result<Response, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (answer, answered) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, answer);
        {
            let _writing = self.write_lock.lock().await;
            write_frame(&mut &*self.stream, &ClientFrame { id, request }).await?;
        }
        answered.await.map_err(|_| ClientError::Closed)
    }

    /// The next event, or `None` once the connection is closed.
    pub async fn next_event(&self) -> Option<Event> {
        self.events.lock().await.recv().await
    }
}

async fn read_loop(
    stream: Arc<LocalSocketStream>,
    pending: Pending,
    events: mpsc::UnboundedSender<Event>,
) {
    let mut reader = BufReader::new(&*stream);
    loop {
        match read_frame::<ServerFrame>(&mut reader).await {
            Ok(Some(ServerFrame::Response { id, response })) => {
                let waiting = pending
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&id);
                if let Some(waiting) = waiting {
                    let _ = waiting.send(response);
                }
            }
            Ok(Some(ServerFrame::Event { event })) => {
                let _ = events.send(event);
            }
            Ok(None) => break,
            Err(error) => {
                debug!(%error, "connection to rebornd ended");
                break;
            }
        }
    }
    // Dropping the senders wakes every waiting request with `Closed`.
    pending
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clear();
}
