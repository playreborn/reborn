//! The session with `rebornd` (ADR 0047): connect to the shim socket once,
//! say `Hello` with this process ID, keep what the daemon answers, and keep
//! the connection open for later requests (ADR 0055).
//!
//! `rebornd` binds a connection to the process that said `Hello` on it and
//! answers account-data requests only on that connection, so the shim keeps
//! the one it said `Hello` on. Game threads may ask at the same time, so
//! every exchange (one request, its response) runs under one lock, and
//! each request gets its own `id`; a response with another `id`, or an
//! event while no request that has events is open, is an error, never
//! guessed around.
//!
//! When an exchange fails (the daemon went away), the shim tries to
//! reconnect once for the whole run, with a new `Hello` that must give the
//! same app (ADR 0055, "`rebornd` stops during the run"). A request after
//! that fails, and its caller stops the game.

use std::io::{BufRead, BufReader, Write};
use std::sync::{Mutex, OnceLock};

use interprocess::local_socket::Stream;
use interprocess::local_socket::prelude::*;
use reborn_shim_protocol::{
    DaemonMessage, PROTOCOL_VERSION, Request, Response, Session, ShimMessage, decode, encode,
};

use crate::log;

/// The session, asked for once; `None` if there is none.
static SESSION: OnceLock<Option<Session>> = OnceLock::new();

/// The connection the session was given on, for later requests.
static LINK: Mutex<Link> = Mutex::new(Link::empty());

/// The session with `rebornd`, connecting on first use.
pub fn get() -> Option<&'static Session> {
    SESSION.get_or_init(connect).as_ref()
}

/// Sets the session without a daemon, for tests.
#[cfg(test)]
pub fn set_for_test(session: Session) {
    let _ = SESSION.set(Some(session));
}

/// Sets the connection without a daemon, for tests. No reconnect is tried
/// on it.
#[cfg(test)]
pub fn set_transport_for_test(transport: Box<dyn Transport>) {
    let mut link = LINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *link = Link::new(transport);
    link.reconnect_left = false;
}

/// One line-based connection to `rebornd`; a trait so tests can use a
/// fake one.
pub trait Transport: Send {
    /// Sends one line (it already ends in `\n`).
    fn send(&mut self, line: &str) -> std::io::Result<()>;
    /// Receives one line; an error at the end of the stream.
    fn receive(&mut self) -> std::io::Result<String>;
}

/// The shim socket.
struct SocketTransport {
    reader: BufReader<Stream>,
}

impl Transport for SocketTransport {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let stream = self.reader.get_mut();
        stream.write_all(line.as_bytes())?;
        stream.flush()
    }

    fn receive(&mut self) -> std::io::Result<String> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        Ok(line)
    }
}

/// Why a request got no answer.
#[derive(Debug)]
pub enum RequestError {
    /// No session for this process (no `Hello` answered).
    NoSession,
    /// The connection failed and could not be made again.
    Connection(std::io::Error),
    /// The daemon's line was not the answer to this request.
    Unexpected,
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::NoSession => write!(f, "no session with rebornd"),
            RequestError::Connection(error) => write!(f, "connection to rebornd failed: {error}"),
            RequestError::Unexpected => write!(f, "rebornd's answer did not match the request"),
        }
    }
}

/// Sends `request` on the session's connection and returns `rebornd`'s
/// response. Safe to call from any thread; exchanges run one at a time.
pub fn request(request: Request) -> Result<Response, RequestError> {
    // Before taking the lock: `get` may connect, which takes it too.
    let app_id = get().ok_or(RequestError::NoSession)?.app_id;
    let mut link = LINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    link.request(&request, || reconnect(app_id))
}

/// The connection and the state of its exchanges.
pub struct Link {
    transport: Option<Box<dyn Transport>>,
    /// The `id` of the next request; 1 is the `Hello`.
    next_id: u64,
    /// Whether the one reconnect of the run is still left.
    reconnect_left: bool,
}

impl Link {
    const fn empty() -> Self {
        Link {
            transport: None,
            next_id: 2,
            reconnect_left: true,
        }
    }

    /// A link on a connection that already said `Hello` (id 1).
    pub fn new(transport: Box<dyn Transport>) -> Self {
        Link {
            transport: Some(transport),
            ..Link::empty()
        }
    }

    /// One exchange; on a failed connection one reconnect for the whole
    /// run (`reconnect` gives a new connection that already said `Hello`
    /// for the same app), then the exchange once more.
    pub fn request(
        &mut self,
        request: &Request,
        reconnect: impl FnOnce() -> std::io::Result<Box<dyn Transport>>,
    ) -> Result<Response, RequestError> {
        let error = match self.exchange(request) {
            Ok(response) => return Ok(response),
            Err(RequestError::Connection(error)) => error,
            Err(other) => return Err(other),
        };
        self.transport = None;
        if !self.reconnect_left {
            return Err(RequestError::Connection(error));
        }
        self.reconnect_left = false;
        log(&format!(
            "connection to rebornd lost ({error}); reconnecting once"
        ));
        let transport = reconnect().map_err(RequestError::Connection)?;
        self.transport = Some(transport);
        self.next_id = 2;
        self.exchange(request)
    }

    fn exchange(&mut self, request: &Request) -> Result<Response, RequestError> {
        let Some(transport) = self.transport.as_mut() else {
            return Err(RequestError::Connection(std::io::Error::from(
                std::io::ErrorKind::NotConnected,
            )));
        };
        let id = self.next_id;
        self.next_id += 1;
        let line = encode(&ShimMessage {
            id,
            request: request.clone(),
        })
        .map_err(|_| RequestError::Unexpected)?;
        transport.send(&line).map_err(RequestError::Connection)?;
        let reply = transport.receive().map_err(RequestError::Connection)?;
        match decode::<DaemonMessage>(&reply) {
            Ok(DaemonMessage::Response {
                id: answered,
                response,
            }) if answered == id => Ok(response),
            // Another id, an event (the shim sends no request that has
            // events yet) or a broken line: the connection is no longer in
            // step, so nothing on it is trusted again.
            _ => {
                self.transport = None;
                self.reconnect_left = false;
                Err(RequestError::Unexpected)
            }
        }
    }
}

fn connect() -> Option<Session> {
    let result = open().and_then(|mut transport| {
        let session = hello(transport.as_mut())?;
        Ok(session.map(|session| (session, transport)))
    });
    match result {
        Ok(Some((session, transport))) => {
            log(&format!("session: app {}", session.app_id));
            let mut link = LINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            *link = Link::new(transport);
            Some(session)
        }
        Ok(None) => None,
        Err(error) => {
            log(&format!("no session: {error}"));
            None
        }
    }
}

/// A new connection with a new `Hello`, which must give the same app.
fn reconnect(app_id: u32) -> std::io::Result<Box<dyn Transport>> {
    let mut transport = open()?;
    match hello(transport.as_mut())? {
        Some(session) if session.app_id == app_id => {
            log("reconnected to rebornd");
            Ok(transport)
        }
        Some(_) => Err(std::io::Error::other("the new session is for another app")),
        None => Err(std::io::Error::other("no session on the new connection")),
    }
}

fn open() -> std::io::Result<Box<dyn Transport>> {
    let stream = Stream::connect(socket_name()?)?;
    Ok(Box::new(SocketTransport {
        reader: BufReader::new(stream),
    }))
}

/// Says `Hello` (id 1) and reads the session.
fn hello(transport: &mut dyn Transport) -> std::io::Result<Option<Session>> {
    let line = encode(&ShimMessage {
        id: 1,
        request: Request::Hello {
            version: PROTOCOL_VERSION,
            pid: std::process::id(),
        },
    })
    .map_err(std::io::Error::other)?;
    transport.send(&line)?;
    let reply = transport.receive()?;
    match decode::<DaemonMessage>(&reply).map_err(std::io::Error::other)? {
        DaemonMessage::Response {
            id: 1,
            response: Response::Session(session),
        } => Ok(Some(session)),
        DaemonMessage::Response {
            response: Response::Refused { reason },
            ..
        } => {
            log(&format!("rebornd refused the session: {reason:?}"));
            Ok(None)
        }
        // No event can come before the session exists.
        DaemonMessage::Response { .. } | DaemonMessage::Event { .. } => {
            Err(std::io::Error::other("unexpected answer"))
        }
    }
}

fn socket_name() -> std::io::Result<interprocess::local_socket::Name<'static>> {
    let name = reborn_shim_protocol::socket_name();
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        name.to_ns_name::<GenericNamespaced>()
    }
    #[cfg(unix)]
    {
        use interprocess::local_socket::GenericFilePath;
        std::path::PathBuf::from(name).to_fs_name::<GenericFilePath>()
    }
}

/// A fake `rebornd` for tests: answers each request line with what
/// `answer` gives for it, and keeps the lines it got.
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// What the fake does with one request: a response, a raw line, or a
    /// broken connection.
    pub enum Reply {
        Response(Response),
        Line(String),
        Broken,
    }

    pub struct Fake {
        pub sent: Arc<Mutex<Vec<String>>>,
        pending: Option<String>,
        answer: Box<dyn FnMut(&ShimMessage) -> Reply + Send>,
    }

    impl Fake {
        pub fn new(answer: impl FnMut(&ShimMessage) -> Reply + Send + 'static) -> Self {
            Fake {
                sent: Arc::default(),
                pending: None,
                answer: Box::new(answer),
            }
        }
    }

    impl Transport for Fake {
        fn send(&mut self, line: &str) -> std::io::Result<()> {
            self.sent
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(line.to_owned());
            let message: ShimMessage = decode(line).map_err(std::io::Error::other)?;
            self.pending = match (self.answer)(&message) {
                Reply::Response(response) => Some(
                    encode(&DaemonMessage::Response {
                        id: message.id,
                        response,
                    })
                    .map_err(std::io::Error::other)?,
                ),
                Reply::Line(line) => Some(line),
                Reply::Broken => None,
            };
            Ok(())
        }

        fn receive(&mut self) -> std::io::Result<String> {
            self.pending
                .take()
                .ok_or_else(|| std::io::ErrorKind::UnexpectedEof.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{Fake, Reply};
    use super::*;

    const OWNERSHIP: Request = Request::AppOwnership { app_id: 2_368_470 };

    #[test]
    fn requests_get_their_own_ids_and_answers() {
        let fake = Fake::new(|_| Reply::Response(Response::Ownership { owned: true }));
        let sent = fake.sent.clone();
        let mut link = Link::new(Box::new(fake));
        for _ in 0..2 {
            let response = link.request(&OWNERSHIP, || panic!("no reconnect"));
            assert!(matches!(response, Ok(Response::Ownership { owned: true })));
        }
        let sent = sent.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        assert_eq!(
            *sent,
            [
                "{\"id\":2,\"request\":{\"type\":\"app_ownership\",\"app_id\":2368470}}\n",
                "{\"id\":3,\"request\":{\"type\":\"app_ownership\",\"app_id\":2368470}}\n",
            ]
        );
    }

    #[test]
    fn a_response_for_another_id_or_an_event_is_not_an_answer() {
        for line in [
            "{\"id\":9,\"response\":{\"type\":\"ownership\",\"owned\":true}}\n",
            "{\"event\":{\"type\":\"stats_received\",\"result\":1,\"achievement_names\":[]}}\n",
            "not json\n",
        ] {
            let fake = Fake::new(move |_| Reply::Line(line.to_owned()));
            let mut link = Link::new(Box::new(fake));
            assert!(matches!(
                link.request(&OWNERSHIP, || panic!("no reconnect after a wrong answer")),
                Err(RequestError::Unexpected)
            ));
            // The connection is not trusted again, and not reconnected.
            assert!(matches!(
                link.request(&OWNERSHIP, || panic!("no reconnect")),
                Err(RequestError::Connection(_))
            ));
        }
    }

    #[test]
    fn a_lost_connection_is_reconnected_once_per_run() {
        let mut link = Link::new(Box::new(Fake::new(|_| Reply::Broken)));
        let response = link.request(&OWNERSHIP, || {
            Ok(Box::new(Fake::new(|message| {
                assert_eq!(message.id, 2, "ids start again on the new connection");
                Reply::Response(Response::Ownership { owned: false })
            })) as Box<dyn Transport>)
        });
        assert!(matches!(response, Ok(Response::Ownership { owned: false })));

        let mut link = Link::new(Box::new(Fake::new(|_| Reply::Broken)));
        let response = link.request(&OWNERSHIP, || {
            Ok(Box::new(Fake::new(|_| Reply::Broken)) as Box<dyn Transport>)
        });
        assert!(matches!(response, Err(RequestError::Connection(_))));
        assert!(matches!(
            link.request(&OWNERSHIP, || panic!("only one reconnect per run")),
            Err(RequestError::Connection(_))
        ));
    }

    #[test]
    fn a_failed_reconnect_gives_no_answer() {
        let mut link = Link::new(Box::new(Fake::new(|_| Reply::Broken)));
        let response = link.request(&OWNERSHIP, || Err(std::io::Error::other("rebornd is gone")));
        assert!(matches!(response, Err(RequestError::Connection(_))));
    }
}
