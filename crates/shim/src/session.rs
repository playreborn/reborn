//! The session with `rebornd` (ADR 0047): connect to the shim socket once,
//! say `Hello` with this process ID, keep what the daemon answers, and keep
//! the connection open for later requests (ADR 0055).
//!
//! `rebornd` binds a connection to the process that said `Hello` on it and
//! answers account-data requests only on that connection, so the shim keeps
//! the one it said `Hello` on.
//!
//! **Reader thread (ADR 0047).** Each connection has its own thread that
//! reads every line `rebornd` sends. A response goes to the request waiting
//! for its `id`; an event ([`DaemonMessage::Event`], for example
//! `StatsReceived`) goes to the event queue ([`take_events`]), which the
//! shim turns into callbacks later (nothing consumes it yet outside the
//! tests). A response for an `id` nobody waits for, an event before the
//! session exists, or a line that is not a message means the connection is
//! **out of step**: nothing on it is trusted again, the waiting request and
//! every later one fail, and no answer is guessed.
//!
//! **Bounded waits.** Every exchange, `Hello` included, waits at most
//! [`ANSWER_TIMEOUT`] for its response, so a stalled `rebornd` never
//! freezes the game silently. A request that is not answered in time fails
//! with "rebornd did not answer in time", and its caller stops the game
//! with that log line (ADR 0055, "Honest failure"). The connection then
//! counts as out of step (a late answer would arrive for an `id` nobody
//! waits for) and is dropped. The reader thread of a dropped connection
//! may stay blocked in its read until `rebornd` closes the socket or the
//! game exits (local sockets give no portable way to cancel a blocking
//! read on a Windows named pipe); it touches nothing once its connection
//! is dropped. It shares the socket, so the socket stays open as long;
//! that is harmless, because a timed-out connection is never replaced.
//! Exchanges run one at a time under one lock, so another game thread
//! that needs the daemon meanwhile waits at most as long, and then fails
//! at once on the dropped connection.
//!
//! **One reconnect.** When the connection is lost (the daemon went away),
//! the shim tries to reconnect once for the whole run, with a new `Hello`
//! that must give the same app (ADR 0055, "`rebornd` stops during the
//! run"). A timeout or an out-of-step connection does not use it: the
//! daemon is still there but no longer in step with the shim, the request
//! may still be in its hands, and waiting a second time would only freeze
//! the game longer. After the reconnect is used, a request that needs the
//! daemon fails, and its caller stops the game. A lost write uses up the
//! reconnect too, although it is not sent again: the shim does not know
//! whether the write was done, so its caller stops the game, and nothing
//! is left to reconnect for. The reconnect runs under the same lock as the
//! request that found the loss, so it can hold that lock for up to three
//! bounded waits (connect, `Hello`, the request sent again; about 30 s at
//! most, usually far less), and other game threads that need the daemon
//! wait that long.
//!
//! **Bounded connect.** Opening the socket also waits at most
//! [`ANSWER_TIMEOUT`] (interprocess's `ConnectWaitMode::Timeout`; its
//! default waits without bound, for example for a busy named pipe), for
//! the first connect and for the reconnect.
//!
//! **Only reads are sent again.** A request whose line was already sent
//! when the connection was lost may have reached `rebornd`. Only
//! idempotent reads ([`resendable`]: `AppOwnership`, `RequestStats`, and
//! the cloud reads once they exist) are sent again on the new connection.
//! Writes (`StoreStats`, `CloudWrite`, once they exist) are never sent
//! again: the daemon may already have done the write, and a second one
//! would be a write the game did not ask for (red line 4). A request that
//! never left the shim (the loss was seen before it was sent) is sent on
//! the new connection whatever it is.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use interprocess::local_socket::Stream;
use interprocess::local_socket::prelude::*;
use reborn_shim_protocol::{
    DaemonMessage, Event, PROTOCOL_VERSION, Request, Response, Session, ShimMessage, decode, encode,
};

use crate::log;

/// How long the shim waits for `rebornd` to answer one exchange (`Hello` or
/// a request) before it gives up. Generous on purpose: requests are
/// answered from the daemon's memory or after one round trip to Valve,
/// which takes well under a second normally; 10 s leaves room for a slow
/// machine or a busy daemon while still ending a stall in a time a player
/// can see. Fixed, not configurable, so that runs are comparable.
pub const ANSWER_TIMEOUT: Duration = Duration::from_secs(10);

/// The session, asked for once; `None` if there is none.
static SESSION: OnceLock<Option<Session>> = OnceLock::new();

/// The events of the run, outside [`LINK`] so that [`take_events`] never
/// waits behind an exchange in flight.
static EVENTS: LazyLock<EventQueue> = LazyLock::new(EventQueue::default);

/// The connection the session was given on, for later requests.
static LINK: LazyLock<Mutex<Link>> = LazyLock::new(|| {
    let mut link = Link::new(Box::new(open_socket), ANSWER_TIMEOUT);
    link.events = EVENTS.clone();
    Mutex::new(link)
});

/// The session with `rebornd`, connecting on first use.
pub fn get() -> Option<&'static Session> {
    SESSION.get_or_init(connect).as_ref()
}

/// Sets the session without a daemon, for tests.
#[cfg(test)]
pub fn set_for_test(session: Session) {
    let _ = SESSION.set(Some(session));
}

/// Replaces the link, for tests (for example one on a fake transport).
#[cfg(test)]
pub fn set_link_for_test(link: Link) {
    *lock(&LINK) = link;
}

/// Sends `request` on the session's connection and returns `rebornd`'s
/// response. Safe to call from any thread; exchanges run one at a time.
pub fn request(request: Request) -> Result<Response, RequestError> {
    // Before taking the lock: `get` may connect, which takes it too.
    let app_id = get().ok_or(RequestError::NoSession)?.app_id;
    lock(&LINK).request(&request, app_id)
}

/// The events `rebornd` sent since the last call, oldest first. The
/// callbacks for them (for example 0x44d for `StatsReceived`) are built by
/// a later change; until then nothing reads it outside the tests of
/// [`Link`].
#[allow(dead_code)] // the 0x44d consumer comes with the stats callbacks
pub fn take_events() -> Vec<Event> {
    lock(&EVENTS).drain(..).collect()
}

/// Whether `request` may be sent again on a new connection after the old
/// one was lost while the request was possibly already at `rebornd`: only
/// idempotent reads. Writes (`StoreStats`, `CloudWrite`, when they are
/// added) must never be: the daemon may already have done the write, and
/// a second one would be a write the game did not ask for (red line 4).
/// The match has no catch-all, so every new message is decided here.
pub fn resendable(request: &Request) -> bool {
    match request {
        // Reads: asking again gives the same answer and changes nothing.
        Request::AppOwnership { .. }
        | Request::RequestStats
        | Request::CloudFileSize { .. }
        | Request::CloudRead { .. } => true,
        // Never sent through `request`: the first message of a connection,
        // or messages of `reborn-observe` (`ComparePersonaName` is a
        // shadow-session read of the observation proxy, ADR 0056).
        Request::Hello { .. }
        | Request::ShadowHello { .. }
        | Request::RegisterShadow { .. }
        | Request::CancelShadow
        | Request::ComparePersonaName { .. } => false,
    }
}

/// Locks `mutex`, also after a panic on another thread (the data is still
/// consistent: every change under these locks is one assignment).
fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The sending side of a line-based connection to `rebornd`; a trait so
/// tests can use a fake one.
pub trait LineSender: Send {
    /// Sends one line (it already ends in `\n`).
    fn send(&mut self, line: &str) -> std::io::Result<()>;
}

/// The receiving side; used only by the connection's reader thread.
pub trait LineReceiver: Send + 'static {
    /// Receives one line, blocking until one comes; an error at the end of
    /// the stream.
    fn receive(&mut self) -> std::io::Result<String>;
}

/// Both sides of a new connection, as an opener gives them.
pub struct Transport {
    /// Where requests go.
    pub sender: Box<dyn LineSender>,
    /// Where responses and events come from.
    pub receiver: Box<dyn LineReceiver>,
}

/// Makes a new connection to `rebornd`; injectable so tests can use a fake
/// daemon.
pub type Opener = Box<dyn Fn() -> std::io::Result<Transport> + Send>;

/// The shim socket, shared by the game thread that sends and the reader
/// thread that receives (`&Stream` reads and writes; interprocess advises
/// this over its deprecated split halves).
struct SharedStream(Arc<Stream>);

impl Read for SharedStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        (&*self.0).read(buf)
    }
}

/// The sending side of the shim socket.
struct SocketSender(Arc<Stream>);

impl LineSender for SocketSender {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut stream = &*self.0;
        stream.write_all(line.as_bytes())?;
        stream.flush()
    }
}

/// The receiving side of the shim socket.
struct SocketReceiver(BufReader<SharedStream>);

impl LineReceiver for SocketReceiver {
    fn receive(&mut self) -> std::io::Result<String> {
        let mut line = String::new();
        if self.0.read_line(&mut line)? == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        Ok(line)
    }
}

/// How the shim socket is opened: waiting at most [`ANSWER_TIMEOUT`] for
/// `rebornd` to take the connection.
fn connect_options(
    name: interprocess::local_socket::Name<'_>,
) -> interprocess::local_socket::ConnectOptions<'_> {
    interprocess::local_socket::ConnectOptions::new()
        .name(name)
        .wait_mode(interprocess::ConnectWaitMode::Timeout(ANSWER_TIMEOUT))
}

/// Opens the shim socket.
fn open_socket() -> std::io::Result<Transport> {
    let stream = Arc::new(connect_options(socket_name()?).connect_sync()?);
    Ok(Transport {
        sender: Box::new(SocketSender(stream.clone())),
        receiver: Box::new(SocketReceiver(BufReader::new(SharedStream(stream)))),
    })
}

/// Why a request got no answer.
#[derive(Debug)]
pub enum RequestError {
    /// No session for this process (no `Hello` answered).
    NoSession,
    /// The connection failed and could not be made again.
    Connection(std::io::Error),
    /// The connection is out of step with `rebornd` (why, for the log):
    /// nothing on it is trusted again.
    OutOfStep(&'static str),
    /// `rebornd` did not answer within [`ANSWER_TIMEOUT`].
    TimedOut,
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestError::NoSession => write!(f, "no session with rebornd"),
            RequestError::Connection(error) => write!(f, "connection to rebornd failed: {error}"),
            RequestError::OutOfStep(why) => {
                write!(f, "connection to rebornd is out of step ({why})")
            }
            RequestError::TimedOut => write!(
                f,
                "rebornd did not answer in time ({} s)",
                ANSWER_TIMEOUT.as_secs()
            ),
        }
    }
}

/// The state of one connection, shared with its reader thread.
#[derive(Clone)]
enum Status {
    Open,
    /// The stream ended or failed.
    Lost(String),
    OutOfStep(&'static str),
    TimedOut,
}

impl Status {
    fn error(&self) -> RequestError {
        match self {
            // Not reached: a waiting request only wakes without an answer
            // after `Routes::end`, which leaves `Open`, and an exchange
            // starts only on an open connection. Still an error, never an
            // answer, if it ever is.
            Status::Open => RequestError::OutOfStep("no answer on an open connection"),
            Status::Lost(why) => RequestError::Connection(std::io::Error::other(why.clone())),
            Status::OutOfStep(why) => RequestError::OutOfStep(why),
            Status::TimedOut => RequestError::TimedOut,
        }
    }
}

/// What the reader thread routes by.
struct Routes {
    status: Status,
    /// Whether the `Hello` (id 1) was answered; no event comes before.
    hello_answered: bool,
    /// The requests waiting for an answer, by `id`.
    waiting: HashMap<u64, mpsc::Sender<Response>>,
}

impl Routes {
    /// Ends the connection with `status` (the first end wins). Dropping the
    /// waiting senders wakes every waiting request.
    fn end(&mut self, status: Status) {
        if matches!(self.status, Status::Open) {
            self.status = status;
        }
        self.waiting.clear();
    }
}

type EventQueue = Arc<Mutex<VecDeque<Event>>>;

/// Why an exchange failed, and whether its line had left the shim.
enum ExchangeError {
    /// The connection was already lost; the line was not sent.
    NotSent(RequestError),
    /// The line was sent (or perhaps partly): `rebornd` may have it.
    Sent(RequestError),
}

/// One connection: its sending side, and the routes its reader thread
/// fills.
struct Connection {
    sender: Box<dyn LineSender>,
    routes: Arc<Mutex<Routes>>,
    /// The `id` of the next request; 1 is the `Hello`.
    next_id: u64,
}

impl Connection {
    /// Starts the reader thread for `transport`.
    fn start(transport: Transport, events: EventQueue) -> std::io::Result<Self> {
        let routes = Arc::new(Mutex::new(Routes {
            status: Status::Open,
            hello_answered: false,
            waiting: HashMap::new(),
        }));
        let reader_routes = routes.clone();
        let mut receiver = transport.receiver;
        std::thread::Builder::new()
            .name("reborn-shim-reader".to_owned())
            .spawn(move || read_lines(receiver.as_mut(), &reader_routes, &events))?;
        Ok(Connection {
            sender: transport.sender,
            routes,
            next_id: 1,
        })
    }

    /// Sends `request` with the next `id` and waits at most `timeout` for
    /// its response.
    fn exchange(
        &mut self,
        request: &Request,
        timeout: Duration,
    ) -> Result<Response, ExchangeError> {
        let id = self.next_id;
        self.next_id += 1;
        let (answer, wait) = mpsc::channel();
        {
            let mut routes = lock(&self.routes);
            if !matches!(routes.status, Status::Open) {
                return Err(ExchangeError::NotSent(routes.status.error()));
            }
            // Before sending, so the answer always finds its request.
            routes.waiting.insert(id, answer);
        }
        let line = encode(&ShimMessage {
            id,
            request: request.clone(),
        })
        .map_err(|_| ExchangeError::NotSent(RequestError::OutOfStep("a request not encodable")))?;
        if let Err(error) = self.sender.send(&line) {
            let mut routes = lock(&self.routes);
            routes.end(Status::Lost(error.to_string()));
            return Err(ExchangeError::Sent(routes.status.error()));
        }
        match wait.recv_timeout(timeout) {
            Ok(response) => Ok(response),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                log("rebornd did not answer in time; dropping the connection");
                let mut routes = lock(&self.routes);
                routes.end(Status::TimedOut);
                Err(ExchangeError::Sent(routes.status.error()))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(ExchangeError::Sent(lock(&self.routes).status.error()))
            }
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // The reader thread stops at its next line.
        lock(&self.routes).end(Status::OutOfStep("the connection was dropped"));
    }
}

/// The reader thread: routes every line until the connection ends.
fn read_lines(receiver: &mut dyn LineReceiver, routes: &Mutex<Routes>, events: &EventQueue) {
    loop {
        let line = receiver.receive();
        let mut routes = lock(routes);
        if !matches!(routes.status, Status::Open) {
            return;
        }
        let line = match line {
            Ok(line) => line,
            Err(error) => {
                routes.end(Status::Lost(error.to_string()));
                return;
            }
        };
        match decode::<DaemonMessage>(&line) {
            Ok(DaemonMessage::Response { id, response }) => match routes.waiting.remove(&id) {
                Some(answer) => {
                    if id == 1 {
                        routes.hello_answered = true;
                    }
                    // The request may have given up just now; then nobody
                    // needs the answer.
                    let _ = answer.send(response);
                }
                None => {
                    log("rebornd answered a request that is not open");
                    routes.end(Status::OutOfStep("an answer for no open request"));
                    return;
                }
            },
            Ok(DaemonMessage::Event { event }) if routes.hello_answered => {
                lock(events).push_back(event);
            }
            Ok(DaemonMessage::Event { .. }) => {
                log("rebornd sent an event before the session");
                routes.end(Status::OutOfStep("an event before the session"));
                return;
            }
            Err(_) => {
                log("rebornd sent a line that is not a message");
                routes.end(Status::OutOfStep("a line that is not a message"));
                return;
            }
        }
    }
}

/// The connection and the state of its exchanges for the run.
pub struct Link {
    open: Opener,
    timeout: Duration,
    connection: Option<Connection>,
    /// Whether the one reconnect of the run is still left.
    reconnect_left: bool,
    /// Events of every connection of the run, oldest first.
    events: EventQueue,
}

impl Link {
    /// A link that opens connections with `open` and waits at most
    /// `timeout` for each answer. Nothing is opened yet.
    pub fn new(open: Opener, timeout: Duration) -> Self {
        Link {
            open,
            timeout,
            connection: None,
            reconnect_left: true,
            events: EventQueue::default(),
        }
    }

    /// Opens a connection and says `Hello`; keeps the connection when
    /// `rebornd` gives a session. `Ok(None)` when it refused.
    pub fn start(&mut self) -> std::io::Result<Option<Session>> {
        let (connection, session) = self.hello()?;
        if session.is_some() {
            self.connection = Some(connection);
        }
        Ok(session)
    }

    /// A new connection with its `Hello` (id 1) answered.
    fn hello(&self) -> std::io::Result<(Connection, Option<Session>)> {
        let mut connection = Connection::start((self.open)()?, self.events.clone())?;
        let hello = Request::Hello {
            version: PROTOCOL_VERSION,
            pid: std::process::id(),
        };
        let response = connection.exchange(&hello, self.timeout).map_err(
            |(ExchangeError::NotSent(error) | ExchangeError::Sent(error))| {
                std::io::Error::other(error.to_string())
            },
        )?;
        match response {
            Response::Session(session) => Ok((connection, Some(session))),
            Response::Refused { reason } => {
                log(&format!("rebornd refused the session: {reason:?}"));
                Ok((connection, None))
            }
            _ => Err(std::io::Error::other("unexpected answer to Hello")),
        }
    }

    /// A new connection whose `Hello` gives the same app.
    fn reconnect(&self, app_id: u32) -> std::io::Result<Connection> {
        match self.hello()? {
            (connection, Some(session)) if session.app_id == app_id => {
                log("reconnected to rebornd");
                Ok(connection)
            }
            (_, Some(_)) => Err(std::io::Error::other("the new session is for another app")),
            (_, None) => Err(std::io::Error::other("no session on the new connection")),
        }
    }

    /// One exchange for the session's app `app_id`. On a lost connection
    /// one reconnect for the whole run, then the request once more if it
    /// never left the shim or is [`resendable`]. A timeout or an
    /// out-of-step connection ends the link for the run.
    pub fn request(&mut self, request: &Request, app_id: u32) -> Result<Response, RequestError> {
        let (error, sent) = match self.exchange(request) {
            Ok(response) => return Ok(response),
            Err(ExchangeError::NotSent(RequestError::Connection(error))) => (error, false),
            Err(ExchangeError::Sent(RequestError::Connection(error))) => (error, true),
            Err(ExchangeError::NotSent(other) | ExchangeError::Sent(other)) => {
                self.connection = None;
                self.reconnect_left = false;
                return Err(other);
            }
        };
        self.connection = None;
        if !self.reconnect_left {
            return Err(RequestError::Connection(error));
        }
        self.reconnect_left = false;
        if sent && !resendable(request) {
            log(&format!(
                "connection to rebornd lost ({error}) after a request that is not sent twice"
            ));
            return Err(RequestError::Connection(error));
        }
        log(&format!(
            "connection to rebornd lost ({error}); reconnecting once"
        ));
        self.connection = Some(self.reconnect(app_id).map_err(RequestError::Connection)?);
        match self.exchange(request) {
            Ok(response) => Ok(response),
            Err(ExchangeError::NotSent(error) | ExchangeError::Sent(error)) => {
                self.connection = None;
                Err(error)
            }
        }
    }

    fn exchange(&mut self, request: &Request) -> Result<Response, ExchangeError> {
        let timeout = self.timeout;
        match self.connection.as_mut() {
            Some(connection) => connection.exchange(request, timeout),
            None => Err(ExchangeError::NotSent(RequestError::Connection(
                std::io::Error::from(std::io::ErrorKind::NotConnected),
            ))),
        }
    }

    /// The events received since the last call, oldest first (for tests;
    /// the shim reads the run's queue with [`take_events`]).
    #[cfg(test)]
    pub fn take_events(&self) -> Vec<Event> {
        lock(&self.events).drain(..).collect()
    }
}

fn connect() -> Option<Session> {
    let mut link = lock(&LINK);
    match link.start() {
        Ok(Some(session)) => {
            log(&format!("session: app {}", session.app_id));
            Some(session)
        }
        Ok(None) => None,
        Err(error) => {
            log(&format!("no session: {error}"));
            None
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
/// `answer` gives for it, and keeps the lines it got. `Hello` is answered
/// with a session for [`fake::APP`] unless `answer` handles it.
#[cfg(test)]
pub mod fake {
    use super::*;

    /// The app of the fake's sessions.
    pub const APP: u32 = 2_368_470;

    /// What the fake does with one request.
    pub enum Reply {
        /// The response, with the request's `id`.
        Response(Response),
        /// Raw lines, in this order.
        Lines(Vec<String>),
        /// Nothing at all: a stalled daemon.
        Silent,
        /// The stream ends.
        Broken,
        /// The response, then the stream ends (the daemon stopped after
        /// answering).
        ResponseThenEnd(Response),
    }

    /// The lines one fake connection got.
    pub type Sent = Arc<Mutex<Vec<String>>>;

    type Answer = Arc<Mutex<dyn FnMut(&ShimMessage) -> Option<Reply> + Send>>;

    struct FakeSender {
        sent: Sent,
        answer: Answer,
        lines: mpsc::Sender<std::io::Result<String>>,
    }

    impl LineSender for FakeSender {
        fn send(&mut self, line: &str) -> std::io::Result<()> {
            lock(&self.sent).push(line.to_owned());
            let message: ShimMessage = decode(line).map_err(std::io::Error::other)?;
            let fallback = match message.request {
                Request::Hello { version, .. } => Reply::Response(Response::Session(Session {
                    version,
                    app_id: APP,
                })),
                _ => Reply::Broken,
            };
            let reply = (lock(&self.answer))(&message).unwrap_or(fallback);
            let out: Vec<std::io::Result<String>> = match reply {
                Reply::Response(response) => vec![
                    encode(&DaemonMessage::Response {
                        id: message.id,
                        response,
                    })
                    .map_err(std::io::Error::other),
                ],
                Reply::Lines(lines) => lines.into_iter().map(Ok).collect(),
                Reply::Silent => Vec::new(),
                Reply::Broken => vec![Err(std::io::ErrorKind::UnexpectedEof.into())],
                Reply::ResponseThenEnd(response) => vec![
                    encode(&DaemonMessage::Response {
                        id: message.id,
                        response,
                    })
                    .map_err(std::io::Error::other),
                    Err(std::io::ErrorKind::UnexpectedEof.into()),
                ],
            };
            for line in out {
                let _ = self.lines.send(line);
            }
            Ok(())
        }
    }

    struct FakeReceiver(mpsc::Receiver<std::io::Result<String>>);

    impl LineReceiver for FakeReceiver {
        fn receive(&mut self) -> std::io::Result<String> {
            // When the sender is dropped (the connection is dropped), the
            // stream ends and the reader thread stops.
            self.0
                .recv()
                .unwrap_or_else(|_| Err(std::io::ErrorKind::UnexpectedEof.into()))
        }
    }

    /// A fake daemon: every connection opened through [`Fake::opener`]
    /// answers with the same `answer`; `None` from it means the default
    /// (a session for `Hello`, a broken stream otherwise).
    pub struct Fake {
        answer: Answer,
        /// The lines of each connection opened so far.
        pub connections: Arc<Mutex<Vec<Sent>>>,
    }

    impl Fake {
        pub fn new(answer: impl FnMut(&ShimMessage) -> Option<Reply> + Send + 'static) -> Self {
            Fake {
                answer: Arc::new(Mutex::new(answer)),
                connections: Arc::default(),
            }
        }

        /// An opener for a [`Link`].
        pub fn opener(&self) -> Opener {
            let answer = self.answer.clone();
            let connections = self.connections.clone();
            Box::new(move || {
                let sent = Sent::default();
                lock(&connections).push(sent.clone());
                let (lines, received) = mpsc::channel();
                Ok(Transport {
                    sender: Box::new(FakeSender {
                        sent,
                        answer: answer.clone(),
                        lines,
                    }),
                    receiver: Box::new(FakeReceiver(received)),
                })
            })
        }

        /// The lines each connection got, without the `Hello` lines.
        pub fn requests(&self) -> Vec<Vec<String>> {
            lock(&self.connections)
                .iter()
                .map(|sent| {
                    lock(sent)
                        .iter()
                        .filter(|line| !line.contains("\"type\":\"hello\""))
                        .cloned()
                        .collect()
                })
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{APP, Fake, Reply};
    use super::*;
    use reborn_shim_protocol::AchievementName;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const OWNERSHIP: Request = Request::AppOwnership { app_id: APP };

    /// Long enough that an answering fake is never too slow, also on a
    /// busy CI machine.
    const PATIENT: Duration = Duration::from_secs(30);

    /// Short, for the fakes that never answer.
    const IMPATIENT: Duration = Duration::from_millis(50);

    fn started(fake: &Fake, timeout: Duration) -> Link {
        let mut link = Link::new(fake.opener(), timeout);
        let session = link.start().expect("hello");
        assert_eq!(session.map(|session| session.app_id), Some(APP));
        link
    }

    fn ownership_line(id: u64) -> String {
        format!("{{\"id\":{id},\"request\":{{\"type\":\"app_ownership\",\"app_id\":{APP}}}}}\n")
    }

    fn owned(owned: bool) -> Option<Reply> {
        Some(Reply::Response(Response::Ownership { owned }))
    }

    fn stats_event() -> Event {
        Event::StatsReceived {
            result: 1,
            achievement_names: vec![AchievementName {
                name: "ACH_WIN".to_owned(),
                display_name: "Win".to_owned(),
            }],
        }
    }

    fn event_line(event: &Event) -> String {
        encode(&DaemonMessage::Event {
            event: event.clone(),
        })
        .expect("encode")
    }

    #[test]
    fn hello_is_id_1_and_requests_get_their_own_ids_and_answers() {
        let fake = Fake::new(|message| match message.request {
            Request::AppOwnership { .. } => owned(true),
            _ => None,
        });
        let mut link = started(&fake, PATIENT);
        for _ in 0..2 {
            let response = link.request(&OWNERSHIP, APP);
            assert!(matches!(response, Ok(Response::Ownership { owned: true })));
        }
        let connections = fake.connections.lock().expect("lock");
        let sent = connections[0].lock().expect("lock");
        assert!(sent[0].starts_with("{\"id\":1,\"request\":{\"type\":\"hello\""));
        assert_eq!(sent[1..], [ownership_line(2), ownership_line(3)]);
    }

    #[test]
    fn a_refused_hello_gives_no_session_and_no_connection() {
        use reborn_shim_protocol::RefusalReason;
        let fake = Fake::new(|message| match message.request {
            Request::Hello { .. } => Some(Reply::Response(Response::Refused {
                reason: RefusalReason::UnknownProcess,
            })),
            _ => owned(true),
        });
        let mut link = Link::new(fake.opener(), PATIENT);
        assert!(matches!(link.start(), Ok(None)));
        assert!(matches!(
            link.request(&OWNERSHIP, APP),
            Err(RequestError::Connection(_))
        ));
    }

    #[test]
    fn a_stalled_hello_times_out() {
        let fake = Fake::new(|_| Some(Reply::Silent));
        let mut link = Link::new(fake.opener(), IMPATIENT);
        let error = link.start().expect_err("no answer");
        assert!(
            error.to_string().contains("did not answer in time"),
            "{error}"
        );
    }

    #[test]
    fn a_stalled_request_times_out_and_ends_the_link_without_reconnect() {
        let fake = Fake::new(|message| match message.request {
            Request::AppOwnership { .. } => Some(Reply::Silent),
            _ => None,
        });
        let mut link = started(&fake, PATIENT);
        link.timeout = IMPATIENT;
        let error = link.request(&OWNERSHIP, APP).expect_err("timeout");
        assert!(matches!(error, RequestError::TimedOut));
        assert!(error.to_string().contains("rebornd did not answer in time"));
        // No reconnect after a timeout, and later requests fail at once.
        assert!(matches!(
            link.request(&OWNERSHIP, APP),
            Err(RequestError::Connection(_))
        ));
        assert_eq!(fake.connections.lock().expect("lock").len(), 1);
    }

    #[test]
    fn a_response_for_another_id_or_a_broken_line_puts_the_connection_out_of_step() {
        for line in [
            "{\"id\":9,\"response\":{\"type\":\"ownership\",\"owned\":true}}\n",
            "not json\n",
        ] {
            let fake = Fake::new(move |message| match message.request {
                Request::AppOwnership { .. } => Some(Reply::Lines(vec![line.to_owned()])),
                _ => None,
            });
            let mut link = started(&fake, PATIENT);
            assert!(matches!(
                link.request(&OWNERSHIP, APP),
                Err(RequestError::OutOfStep(_))
            ));
            // The connection is not trusted again, and not reconnected.
            assert!(matches!(
                link.request(&OWNERSHIP, APP),
                Err(RequestError::Connection(_))
            ));
            assert_eq!(fake.connections.lock().expect("lock").len(), 1, "{line}");
        }
    }

    #[test]
    fn an_event_before_the_session_is_out_of_step() {
        let fake = Fake::new(|message| match message.request {
            Request::Hello { .. } => Some(Reply::Lines(vec![event_line(&stats_event())])),
            _ => None,
        });
        let mut link = Link::new(fake.opener(), PATIENT);
        let error = link.start().expect_err("out of step");
        assert!(error.to_string().contains("out of step"), "{error}");
        assert!(link.take_events().is_empty());
    }

    #[test]
    fn events_go_to_the_queue_and_responses_to_their_request() {
        let fake = Fake::new(|message| match message.request {
            // The event comes before the response, as it may when the
            // daemon answers an earlier `RequestStats` meanwhile.
            Request::RequestStats => Some(Reply::Lines(vec![
                encode(&DaemonMessage::Response {
                    id: message.id,
                    response: Response::Accepted,
                })
                .expect("encode"),
                event_line(&stats_event()),
            ])),
            Request::AppOwnership { .. } => Some(Reply::Lines(vec![
                event_line(&stats_event()),
                encode(&DaemonMessage::Response {
                    id: message.id,
                    response: Response::Ownership { owned: false },
                })
                .expect("encode"),
            ])),
            _ => None,
        });
        let mut link = started(&fake, PATIENT);
        assert!(matches!(
            link.request(&Request::RequestStats, APP),
            Ok(Response::Accepted)
        ));
        assert!(matches!(
            link.request(&OWNERSHIP, APP),
            Ok(Response::Ownership { owned: false })
        ));
        // The event after `Accepted` may still be on its way.
        let mut events = Vec::new();
        let deadline = std::time::Instant::now() + PATIENT;
        while events.len() < 2 && std::time::Instant::now() < deadline {
            events.extend(link.take_events());
            std::thread::yield_now();
        }
        assert_eq!(events, [stats_event(), stats_event()]);
        assert!(link.take_events().is_empty(), "taken once");
    }

    #[test]
    fn a_lost_connection_is_reconnected_once_per_run() {
        let hellos = Arc::new(AtomicUsize::new(0));
        let count = hellos.clone();
        let fake = Fake::new(move |message| match message.request {
            Request::Hello { .. } => {
                count.fetch_add(1, Ordering::SeqCst);
                None
            }
            // The first connection breaks, the second answers.
            _ if count.load(Ordering::SeqCst) == 1 => Some(Reply::Broken),
            _ => owned(false),
        });
        let mut link = started(&fake, PATIENT);
        assert!(matches!(
            link.request(&OWNERSHIP, APP),
            Ok(Response::Ownership { owned: false })
        ));
        assert_eq!(
            fake.requests(),
            [vec![ownership_line(2)], vec![ownership_line(2)]],
            "the read is sent again, ids start again on the new connection"
        );
        assert_eq!(hellos.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn only_one_reconnect_per_run() {
        let fake = Fake::new(|_| None);
        let mut link = started(&fake, PATIENT);
        assert!(matches!(
            link.request(&OWNERSHIP, APP),
            Err(RequestError::Connection(_))
        ));
        assert!(matches!(
            link.request(&OWNERSHIP, APP),
            Err(RequestError::Connection(_))
        ));
        assert_eq!(fake.connections.lock().expect("lock").len(), 2);
    }

    #[test]
    fn a_reconnect_for_another_app_gives_no_answer() {
        let hellos = Arc::new(AtomicUsize::new(0));
        let count = hellos.clone();
        let fake = Fake::new(move |message| match message.request {
            Request::Hello { version, .. } => {
                let app_id = if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    APP
                } else {
                    APP + 1
                };
                Some(Reply::Response(Response::Session(Session {
                    version,
                    app_id,
                })))
            }
            _ if count.load(Ordering::SeqCst) == 1 => Some(Reply::Broken),
            _ => owned(true),
        });
        let mut link = started(&fake, PATIENT);
        let error = link.request(&OWNERSHIP, APP).expect_err("another app");
        assert!(error.to_string().contains("another app"), "{error}");
        assert_eq!(fake.requests()[1], Vec::<String>::new(), "nothing asked");
    }

    #[test]
    fn a_failed_reconnect_gives_no_answer() {
        let opened = Arc::new(AtomicUsize::new(0));
        let fake = Fake::new(|_| None);
        let inner = fake.opener();
        let count = opened.clone();
        let mut link = Link::new(
            Box::new(move || {
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    inner()
                } else {
                    Err(std::io::Error::other("rebornd is gone"))
                }
            }),
            PATIENT,
        );
        assert!(matches!(link.start(), Ok(Some(_))));
        assert!(matches!(
            link.request(&OWNERSHIP, APP),
            Err(RequestError::Connection(_))
        ));
        assert_eq!(opened.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_request_not_resendable_is_not_sent_again() {
        // No write exists in protocol 0.2; a request the shim never resends
        // stands in for one, to test the rule a write will follow.
        let fake = Fake::new(|message| match message.request {
            Request::Hello { .. } => None,
            _ => Some(Reply::Broken),
        });
        let mut link = started(&fake, PATIENT);
        let not_resendable = Request::CancelShadow;
        assert!(!resendable(&not_resendable));
        assert!(matches!(
            link.request(&not_resendable, APP),
            Err(RequestError::Connection(_))
        ));
        assert_eq!(
            fake.connections.lock().expect("lock").len(),
            1,
            "no new connection to send it on"
        );
    }

    #[test]
    fn a_request_that_never_left_goes_on_the_new_connection() {
        // The daemon stops right after the `Hello`; the shim sees the end
        // before its next request, so that request was never sent and may
        // go on the new connection even if it is not resendable.
        let hellos = Arc::new(AtomicUsize::new(0));
        let count = hellos.clone();
        let fake = Fake::new(move |message| match message.request {
            Request::Hello { version, .. } => {
                let session = Response::Session(Session {
                    version,
                    app_id: APP,
                });
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    Some(Reply::ResponseThenEnd(session))
                } else {
                    Some(Reply::Response(session))
                }
            }
            _ => Some(Reply::Response(Response::Accepted)),
        });
        let mut link = started(&fake, PATIENT);
        let deadline = std::time::Instant::now() + PATIENT;
        while link
            .connection
            .as_ref()
            .is_some_and(|connection| matches!(lock(&connection.routes).status, Status::Open))
            && std::time::Instant::now() < deadline
        {
            std::thread::yield_now();
        }
        let not_resendable = Request::CancelShadow;
        assert!(matches!(
            link.request(&not_resendable, APP),
            Ok(Response::Accepted)
        ));
        assert_eq!(
            fake.requests(),
            [
                Vec::<String>::new(),
                vec![
                    "{\"id\":2,\"request\":{\"type\":\"cancel_shadow\"}}
"
                    .to_owned()
                ]
            ]
        );
    }

    #[test]
    fn the_socket_connect_waits_at_most_the_answer_timeout() {
        let name = socket_name().expect("name");
        let options = format!("{:?}", connect_options(name));
        assert!(
            options.contains(&format!("Timeout({ANSWER_TIMEOUT:?})")),
            "{options}"
        );
    }

    #[test]
    fn a_connect_that_times_out_gives_no_session_and_no_reconnect_answer() {
        // What `open_socket` returns when `rebornd` does not take the
        // connection in time.
        fn timed_out() -> std::io::Result<Transport> {
            Err(std::io::ErrorKind::TimedOut.into())
        }
        let mut link = Link::new(Box::new(timed_out), PATIENT);
        assert!(link.start().is_err(), "no session");

        // The reconnect: the first connect works, the second times out.
        let opened = Arc::new(AtomicUsize::new(0));
        let fake = Fake::new(|_| None);
        let inner = fake.opener();
        let count = opened.clone();
        let mut link = Link::new(
            Box::new(move || {
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    inner()
                } else {
                    timed_out()
                }
            }),
            PATIENT,
        );
        assert!(matches!(link.start(), Ok(Some(_))));
        let error = link.request(&OWNERSHIP, APP).expect_err("no answer");
        assert!(
            matches!(&error, RequestError::Connection(inner) if inner.kind() == std::io::ErrorKind::TimedOut),
            "{error}"
        );
        assert_eq!(opened.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn events_can_be_taken_while_an_exchange_waits() {
        // `take_events` reads `EVENTS`, not the link, so it does not wait
        // for the lock an exchange holds.
        let guard = lock(&LINK);
        lock(&EVENTS).push_back(stats_event());
        let events = take_events();
        drop(guard);
        assert!(events.contains(&stats_event()));
    }

    #[test]
    fn only_reads_are_resendable() {
        let version = PROTOCOL_VERSION;
        for request in [
            Request::Hello { version, pid: 1 },
            Request::AppOwnership { app_id: APP },
            Request::RequestStats,
            Request::CloudFileSize {
                name: "save.dat".to_owned(),
            },
            Request::CloudRead {
                name: "save.dat".to_owned(),
                max: 16,
            },
            Request::RegisterShadow { app_id: APP },
            Request::CancelShadow,
            Request::ShadowHello {
                version,
                pid: 1,
                app_id: APP,
            },
        ] {
            if resendable(&request) {
                assert!(!request.writes(), "{request:?}");
            }
        }
        assert!(resendable(&Request::AppOwnership { app_id: APP }));
        assert!(resendable(&Request::RequestStats));
        assert!(resendable(&Request::CloudRead {
            name: "save.dat".to_owned(),
            max: 16,
        }));
    }
}
