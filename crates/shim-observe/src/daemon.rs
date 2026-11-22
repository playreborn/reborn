//! The shim channel to `rebornd` for shadow mode (ADR 0046 as amended by
//! ADR 0055): for account data, Reborn's shadow answer comes from
//! `rebornd`, not from a value given to `reborn-observe arm`.
//!
//! - `reborn-observe arm --app <id>` sends `RegisterShadow` for that app,
//!   `disarm` sends `CancelShadow` ([`Client::register_shadow`],
//!   [`Client::cancel_shadow`]).
//! - The proxy inside the game says `ShadowHello` with its process ID and
//!   the app, then asks `AppOwnership` for the game's own app
//!   ([`ShadowLink`]). It only compares; the game always gets the official
//!   client's result.
//!
//! - When the official client delivers callback 0x44d
//!   (`UserStatsReceived_t`, `docs/shim/meanings.md`, experiment 21) to the
//!   game, the proxy sends `RequestStats` on the same shadow session and
//!   waits for the event `StatsReceived` ([`ShadowLink::stats_result`]).
//!
//! Whether an app is owned, and the stats, are account data (ADR 0055):
//! the log gets only `shadow match` or `shadow mismatch`
//! ([`compare_subscribed`], [`compare_stats_received`]), never the values.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use reborn_shim_protocol::{
    DaemonMessage, Event, PROTOCOL_VERSION, RefusalReason, Request, Response, ShimMessage,
    UnavailableReason, decode, encode,
};

/// Longest line read from `rebornd`; no answer of protocol 0.2 comes near
/// it.
const MAX_LINE: u64 = 64 * 1024;

/// Talking to `rebornd` failed.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// The socket could not be reached, read or written.
    #[error("cannot reach rebornd: {0}")]
    Io(#[from] io::Error),
    /// `rebornd` closed the connection.
    #[error("rebornd closed the connection")]
    Closed,
    /// The answer was not the one expected.
    #[error("rebornd sent an unexpected answer")]
    Unexpected,
}

/// Where a [`Client`] reads `rebornd`'s lines from.
pub trait Lines {
    /// The next line, waiting at most `wait` (`None`: as long as it
    /// takes). `Ok(None)` when the wait ran out; [`DaemonError::Closed`]
    /// when `rebornd` closed the connection.
    fn next_line(&mut self, wait: Option<Duration>) -> Result<Option<String>, DaemonError>;
}

/// A plain reader blocks: it cannot stop waiting, so `wait` is ignored.
/// Used by `reborn-observe arm` and in tests.
impl<R: BufRead> Lines for R {
    fn next_line(&mut self, _wait: Option<Duration>) -> Result<Option<String>, DaemonError> {
        let mut line = String::new();
        if self.take(MAX_LINE).read_line(&mut line)? == 0 {
            return Err(DaemonError::Closed);
        }
        Ok(Some(line))
    }
}

/// Lines read by a thread of their own, so a wait can end (the named pipe
/// has no read timeout on Windows). The thread reads whole lines, so a
/// wait that runs out never cuts one in half. It ends when the connection
/// closes or fails, or when this value is gone and the next line arrives.
/// The thread holds its own handle to the pipe: after a failed link drops
/// the client, the pipe stays open until `rebornd` writes a line or the
/// game's process exits. Nothing more is asked on it, and the read-only
/// shadow session ends with the process.
pub struct LineThread {
    lines: mpsc::Receiver<io::Result<String>>,
}

/// Lines read ahead at most; the reading thread then waits.
const LINES_AHEAD: usize = 16;

impl LineThread {
    /// Starts the thread reading `reader`.
    pub fn spawn<R: BufRead + Send + 'static>(mut reader: R) -> io::Result<Self> {
        let (sender, lines) = mpsc::sync_channel(LINES_AHEAD);
        std::thread::Builder::new()
            .name("reborn-shadow-read".to_owned())
            .spawn(move || {
                loop {
                    let mut line = String::new();
                    let read = match (&mut reader).take(MAX_LINE).read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => Ok(line),
                        Err(error) => Err(error),
                    };
                    let failed = read.is_err();
                    if sender.send(read).is_err() || failed {
                        break;
                    }
                }
            })?;
        Ok(Self { lines })
    }
}

impl Lines for LineThread {
    fn next_line(&mut self, wait: Option<Duration>) -> Result<Option<String>, DaemonError> {
        let line = match wait {
            None => self.lines.recv().map_err(|_| DaemonError::Closed)?,
            Some(wait) => match self.lines.recv_timeout(wait) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => return Ok(None),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(DaemonError::Closed),
            },
        };
        Ok(Some(line?))
    }
}

/// One connection to `rebornd`'s shim socket: requests with increasing
/// `id`s, one answer each.
pub struct Client<R, W> {
    reader: R,
    writer: W,
    next_id: u64,
}

impl<R: Lines, W: Write> Client<R, W> {
    /// A client reading answers from `reader` and writing to `writer`.
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            next_id: 1,
        }
    }

    /// Sends `request` and waits for its answer. Events are skipped: they
    /// belong to requests this client never sends.
    pub fn ask(&mut self, request: Request) -> Result<Response, DaemonError> {
        let id = self.next_id;
        self.next_id += 1;
        let line = encode(&ShimMessage { id, request }).map_err(|_| DaemonError::Unexpected)?;
        self.writer.write_all(line.as_bytes())?;
        self.writer.flush()?;
        loop {
            // Without a wait the reader never runs out.
            let Some(reply) = self.reader.next_line(None)? else {
                return Err(DaemonError::Unexpected);
            };
            match decode::<DaemonMessage>(&reply).map_err(|_| DaemonError::Unexpected)? {
                DaemonMessage::Response {
                    id: answered,
                    response,
                } if answered == id => return Ok(response),
                DaemonMessage::Response { .. } => return Err(DaemonError::Unexpected),
                DaemonMessage::Event { .. } => {}
            }
        }
    }

    /// `RegisterShadow { app_id }`: `Ok(Ok(()))` when registered, or why
    /// `rebornd` refused.
    pub fn register_shadow(
        &mut self,
        app_id: u32,
    ) -> Result<Result<(), UnavailableReason>, DaemonError> {
        match self.ask(Request::RegisterShadow { app_id })? {
            Response::Accepted => Ok(Ok(())),
            Response::Unavailable { reason } => Ok(Err(reason)),
            _ => Err(DaemonError::Unexpected),
        }
    }

    /// `CancelShadow`: drops a registration nobody claimed.
    pub fn cancel_shadow(&mut self) -> Result<Result<(), UnavailableReason>, DaemonError> {
        match self.ask(Request::CancelShadow)? {
            Response::Accepted => Ok(Ok(())),
            Response::Unavailable { reason } => Ok(Err(reason)),
            _ => Err(DaemonError::Unexpected),
        }
    }

    /// `ShadowHello`: `Ok(Ok(()))` when the session for `app_id` is this
    /// process's, or why `rebornd` refused.
    pub fn shadow_hello(
        &mut self,
        pid: u32,
        app_id: u32,
    ) -> Result<Result<(), RefusalReason>, DaemonError> {
        match self.ask(Request::ShadowHello {
            version: PROTOCOL_VERSION,
            pid,
            app_id,
        })? {
            Response::Session(session) if session.app_id == app_id => Ok(Ok(())),
            Response::Refused { reason } => Ok(Err(reason)),
            _ => Err(DaemonError::Unexpected),
        }
    }

    /// `AppOwnership { app_id }`: whether the account owns the app, or why
    /// `rebornd` cannot say.
    pub fn owned(&mut self, app_id: u32) -> Result<Result<bool, UnavailableReason>, DaemonError> {
        match self.ask(Request::AppOwnership { app_id })? {
            Response::Ownership { owned } => Ok(Ok(owned)),
            Response::Unavailable { reason } => Ok(Err(reason)),
            _ => Err(DaemonError::Unexpected),
        }
    }

    /// `RequestStats`: `Ok(Ok(()))` when `rebornd` accepted it (its result
    /// follows as an event, [`Self::stats_event`]), or why it cannot.
    pub fn request_stats(&mut self) -> Result<Result<(), UnavailableReason>, DaemonError> {
        match self.ask(Request::RequestStats)? {
            Response::Accepted => Ok(Ok(())),
            Response::Unavailable { reason } => Ok(Err(reason)),
            _ => Err(DaemonError::Unexpected),
        }
    }

    /// Waits at most `wait` for the event `StatsReceived` of an accepted
    /// `RequestStats`: `Ok(Some(result))` with Valve's `EResult`, `Ok(None)`
    /// when the wait ran out (`rebornd` sends no event for a request that
    /// failed, ADR 0055 "Honest failure"). The achievement names are not
    /// kept: callback 0x44d carries none. No request is open, so a
    /// response here is unexpected.
    pub fn stats_event(&mut self, wait: Duration) -> Result<Option<i32>, DaemonError> {
        let Some(line) = self.reader.next_line(Some(wait))? else {
            return Ok(None);
        };
        // `StatsReceived` is the only event of protocol 0.2.
        match decode::<DaemonMessage>(&line).map_err(|_| DaemonError::Unexpected)? {
            DaemonMessage::Event {
                event: Event::StatsReceived { result, .. },
            } => Ok(Some(result)),
            DaemonMessage::Response { .. } => Err(DaemonError::Unexpected),
        }
    }
}

/// One socket connection, shared by the reading and the writing side
/// (`&Stream` reads and writes).
pub struct Socket(std::sync::Arc<interprocess::local_socket::Stream>);

impl Read for Socket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self.0).read(buf)
    }
}

impl Write for Socket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self.0).flush()
    }
}

/// Connects to `rebornd`'s shim socket (ADR 0047).
pub fn connect() -> io::Result<Client<BufReader<Socket>, Socket>> {
    let stream = std::sync::Arc::new(open_stream()?);
    let reader = Socket(std::sync::Arc::clone(&stream));
    Ok(Client::new(BufReader::new(reader), Socket(stream)))
}

/// Connects like [`connect`], with the lines read on a thread of their own
/// so that the proxy's wait for a stats event can end ([`LineThread`]).
pub fn connect_with_waits() -> io::Result<Client<LineThread, Socket>> {
    let stream = std::sync::Arc::new(open_stream()?);
    let reader = Socket(std::sync::Arc::clone(&stream));
    Ok(Client::new(
        LineThread::spawn(BufReader::new(reader))?,
        Socket(stream),
    ))
}

fn open_stream() -> io::Result<interprocess::local_socket::Stream> {
    use interprocess::local_socket::Stream;
    use interprocess::local_socket::prelude::*;
    let name = reborn_shim_protocol::socket_name();
    #[cfg(windows)]
    let name = {
        use interprocess::local_socket::GenericNamespaced;
        name.to_ns_name::<GenericNamespaced>()?
    };
    #[cfg(unix)]
    let name = {
        use interprocess::local_socket::GenericFilePath;
        std::path::PathBuf::from(name).to_fs_name::<GenericFilePath>()?
    };
    Stream::connect(name)
}

/// The proxy's shadow session with `rebornd`: made on the first question,
/// once. If it cannot be made, later questions get the same reason and
/// nothing is tried again (one game run, one attempt).
pub struct ShadowLink<R, W> {
    state: State<R, W>,
}

enum State<R, W> {
    NotYet,
    Ready(Client<R, W>),
    Failed(String),
}

impl<R: Lines, W: Write> Default for ShadowLink<R, W> {
    fn default() -> Self {
        Self {
            state: State::NotYet,
        }
    }
}

impl<R: Lines, W: Write> ShadowLink<R, W> {
    /// Reborn's answer to `BIsSubscribed` for `app_id`, the app given to
    /// `reborn-observe arm --app`: whether `rebornd` says the account owns
    /// it, or why there is no answer. On the first question it connects
    /// with `connect` and claims the shadow session for process `pid`.
    pub fn subscribed(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
    ) -> Result<bool, String> {
        let client = self.client(pid, app_id, connect)?;
        let answer = client.owned(app_id);
        match answer {
            Ok(Ok(owned)) => Ok(owned),
            Ok(Err(reason)) => Err(reason_name(reason)),
            Err(error) => Err(self.fail(error)),
        }
    }

    /// Reborn's answer to the stats request behind callback 0x44d: the
    /// `EResult` of `rebornd`'s `StatsReceived` for the session's own app
    /// (`RequestStats` carries no app ID), or why there is none. Waits at
    /// most `wait` for the event; a wait that runs out keeps the session
    /// (a late event is skipped by the next question, or taken by the
    /// next stats question, which gets the run's same cached answer).
    pub fn stats_result(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
        wait: Duration,
    ) -> Result<i32, String> {
        let client = self.client(pid, app_id, connect)?;
        let answer = match client.request_stats() {
            Ok(Ok(())) => client.stats_event(wait),
            Ok(Err(reason)) => return Err(reason_name(reason)),
            Err(error) => Err(error),
        };
        match answer {
            Ok(Some(result)) => Ok(result),
            Ok(None) => Err(format!("no stats event within {} s", wait.as_secs())),
            Err(error) => Err(self.fail(error)),
        }
    }

    /// The session's client, made on the first question (one attempt per
    /// run).
    fn client(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
    ) -> Result<&mut Client<R, W>, String> {
        if matches!(self.state, State::NotYet) {
            self.state = match open(pid, app_id, connect) {
                Ok(client) => State::Ready(client),
                Err(why) => State::Failed(why),
            };
        }
        match &mut self.state {
            State::Ready(client) => Ok(client),
            State::Failed(why) => Err(why.clone()),
            State::NotYet => Err("no session".to_owned()),
        }
    }

    /// The connection broke: no further question is tried.
    fn fail(&mut self, error: DaemonError) -> String {
        let why = error.to_string();
        self.state = State::Failed(why.clone());
        why
    }
}

fn open<R: Lines, W: Write>(
    pid: u32,
    app_id: u32,
    connect: impl FnOnce() -> io::Result<Client<R, W>>,
) -> Result<Client<R, W>, String> {
    let mut client = connect().map_err(|error| DaemonError::Io(error).to_string())?;
    match client.shadow_hello(pid, app_id) {
        Ok(Ok(())) => Ok(client),
        Ok(Err(reason)) => Err(format!(
            "rebornd refused the shadow session ({}); was `reborn-observe arm --app {app_id}` registered?",
            refusal_name(reason)
        )),
        Err(error) => Err(error.to_string()),
    }
}

/// The protocol's name of a reason, for log lines.
pub fn reason_name(reason: UnavailableReason) -> String {
    match reason {
        UnavailableReason::NotSignedIn => "not_signed_in",
        UnavailableReason::Offline => "offline",
        UnavailableReason::NotAllowed => "not_allowed",
        UnavailableReason::TooLarge => "too_large",
    }
    .to_owned()
}

fn refusal_name(reason: RefusalReason) -> &'static str {
    match reason {
        RefusalReason::IncompatibleVersion => "incompatible_version",
        RefusalReason::UnknownProcess => "unknown_process",
    }
}

/// The shadow note for `BIsSubscribed` (`STEAMAPPS_INTERFACE_VERSION008`
/// position 0, `docs/shim/meanings.md`): the official client's result is
/// the lowest byte of `rax` (experiment 8 recorded 1 there), Reborn's is
/// `rebornd`'s answer. Only match or mismatch is logged (account data,
/// ADR 0055).
pub fn compare_subscribed(rax: u64, reborn: Result<bool, String>) -> String {
    match reborn {
        Ok(owned) => {
            if rax as u8 == u8::from(owned) {
                "shadow match".to_owned()
            } else {
                "shadow mismatch".to_owned()
            }
        }
        Err(why) => format!("shadow: no Reborn answer ({why})"),
    }
}

/// The fields of callback 0x44d the stats shadow compares
/// (`UserStatsReceived_t`, `docs/shim/meanings.md`, experiment 21):
/// the payload size, the app ID in bytes 0..4 and the `EResult` in bytes
/// 8..12. Bytes 4..8 (the rest of the game ID) were never checked, and
/// bytes 12..20 (read as the user's Steam ID) are not compared until the
/// experiment ADR 0055 asks for has compared them with the account's own
/// Steam ID. The comparison does not read bytes 12..20 (nor 20..24, which
/// are unexplained); the step-4b line still logs the 0x44d payload
/// locally (ADR 0050, 0054).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfficialStats {
    size: usize,
    app_id: u32,
    result: i32,
}

/// Bytes of 0x44d read for the comparison: app ID, game ID rest, result.
pub const STATS_READ: usize = 12;

/// The recorded size of 0x44d (experiments 20 and 21), which a callback
/// written by Reborn would have too.
const STATS_SIZE: usize = 24;

impl OfficialStats {
    /// From the payload `size` the callback header gave and the payload's
    /// first bytes (at least [`STATS_READ`]); `None` if fewer are given.
    pub fn new(size: usize, first: &[u8]) -> Option<Self> {
        let word = |at: usize| -> Option<[u8; 4]> { first.get(at..at + 4)?.try_into().ok() };
        Some(Self {
            size,
            app_id: u32::from_le_bytes(word(0)?),
            result: i32::from_le_bytes(word(8)?),
        })
    }
}

/// The shadow note for callback 0x44d (ADR 0055, stage 1: compared by its
/// size and its confirmed fields): a match needs the official callback to
/// have the recorded size, to be for `app_id` (the app given to
/// `reborn-observe arm --app`, the session's own app) and to carry the
/// same `EResult` as `rebornd`'s `StatsReceived`. Only match or mismatch
/// is logged (account data), not which field differs.
pub fn compare_stats_received(
    official: OfficialStats,
    app_id: u32,
    reborn: Result<i32, String>,
) -> String {
    match reborn {
        Ok(result) => {
            if official.size == STATS_SIZE && official.app_id == app_id && official.result == result
            {
                "shadow match".to_owned()
            } else {
                "shadow mismatch".to_owned()
            }
        }
        Err(why) => format!("shadow: no Reborn answer ({why})"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const DARK_DECEPTION: u32 = 332_950;

    /// A client whose answers are `lines`; what it sends goes to a vector.
    fn scripted(lines: &[&str]) -> Client<Cursor<Vec<u8>>, Vec<u8>> {
        let mut text = String::new();
        for line in lines {
            text.push_str(line);
            text.push('\n');
        }
        Client::new(Cursor::new(text.into_bytes()), Vec::new())
    }

    fn sent(client: &Client<Cursor<Vec<u8>>, Vec<u8>>) -> String {
        String::from_utf8_lossy(&client.writer).into_owned()
    }

    #[test]
    fn registering_sends_one_request_and_reads_the_answer() {
        let mut client = scripted(&[
            r#"{"id":1,"response":{"type":"accepted"}}"#,
            r#"{"id":2,"response":{"type":"unavailable","reason":"not_signed_in"}}"#,
        ]);
        assert!(matches!(client.register_shadow(DARK_DECEPTION), Ok(Ok(()))));
        assert!(matches!(
            client.register_shadow(DARK_DECEPTION),
            Ok(Err(UnavailableReason::NotSignedIn))
        ));
        assert_eq!(
            sent(&client),
            "{\"id\":1,\"request\":{\"type\":\"register_shadow\",\"app_id\":332950}}\n\
             {\"id\":2,\"request\":{\"type\":\"register_shadow\",\"app_id\":332950}}\n"
        );
        assert!(matches!(client.cancel_shadow(), Err(DaemonError::Closed)));
    }

    #[test]
    fn answers_for_other_requests_are_unexpected() {
        let mut client = scripted(&[r#"{"id":7,"response":{"type":"accepted"}}"#]);
        assert!(matches!(
            client.cancel_shadow(),
            Err(DaemonError::Unexpected)
        ));
        let mut client = scripted(&[r#"{"id":1,"response":{"type":"ownership","owned":true}}"#]);
        assert!(matches!(
            client.register_shadow(1),
            Err(DaemonError::Unexpected)
        ));
    }

    #[test]
    fn events_are_skipped() {
        let mut client = scripted(&[
            r#"{"event":{"type":"stats_received","result":1,"achievement_names":[]}}"#,
            r#"{"id":1,"response":{"type":"ownership","owned":false}}"#,
        ]);
        assert!(matches!(client.owned(DARK_DECEPTION), Ok(Ok(false))));
    }

    #[test]
    fn the_shadow_link_says_hello_once_then_asks_for_the_own_app() {
        let mut link = ShadowLink::default();
        let mut connects = 0;
        let answer = link.subscribed(4242, DARK_DECEPTION, || {
            connects += 1;
            Ok(scripted(&[
                r#"{"id":1,"response":{"type":"session","version":{"major":0,"minor":2},"app_id":332950}}"#,
                r#"{"id":2,"response":{"type":"ownership","owned":true}}"#,
                r#"{"id":3,"response":{"type":"unavailable","reason":"not_allowed"}}"#,
            ]))
        });
        assert_eq!(answer, Ok(true));
        let again = link.subscribed(4242, DARK_DECEPTION, || {
            connects += 1;
            Err(io::Error::other("not again"))
        });
        assert_eq!(again, Err("not_allowed".to_owned()));
        assert_eq!(connects, 1);
        let State::Ready(client) = &link.state else {
            panic!("no session");
        };
        assert_eq!(
            sent(client),
            "{\"id\":1,\"request\":{\"type\":\"shadow_hello\",\"version\":{\"major\":0,\"minor\":2},\"pid\":4242,\"app_id\":332950}}\n\
             {\"id\":2,\"request\":{\"type\":\"app_ownership\",\"app_id\":332950}}\n\
             {\"id\":3,\"request\":{\"type\":\"app_ownership\",\"app_id\":332950}}\n"
        );
    }

    #[test]
    fn a_refused_or_unreachable_link_is_not_tried_again() {
        let mut link = ShadowLink::default();
        let refused = link.subscribed(1, DARK_DECEPTION, || {
            Ok(scripted(&[
                r#"{"id":1,"response":{"type":"refused","reason":"unknown_process"}}"#,
            ]))
        });
        let Err(why) = refused else {
            panic!("not refused");
        };
        assert!(why.contains("unknown_process"), "{why}");
        let again = link.subscribed(1, DARK_DECEPTION, || -> io::Result<_> {
            panic!("connected again")
        });
        assert_eq!(again, Err(why));

        let mut link: ShadowLink<Cursor<Vec<u8>>, Vec<u8>> = ShadowLink::default();
        let unreachable = link.subscribed(1, DARK_DECEPTION, || Err(io::Error::other("no pipe")));
        assert!(
            unreachable
                .as_ref()
                .is_err_and(|why| why.contains("cannot reach rebornd")),
            "{unreachable:?}"
        );
    }

    #[test]
    fn a_session_for_another_app_is_unexpected() {
        let mut link = ShadowLink::default();
        let answer = link.subscribed(1, DARK_DECEPTION, || {
            Ok(scripted(&[
                r#"{"id":1,"response":{"type":"session","version":{"major":0,"minor":2},"app_id":480}}"#,
            ]))
        });
        assert_eq!(answer, Err("rebornd sent an unexpected answer".to_owned()));
    }

    #[test]
    fn only_match_or_mismatch_is_logged() {
        assert_eq!(compare_subscribed(1, Ok(true)), "shadow match");
        assert_eq!(compare_subscribed(0, Ok(false)), "shadow match");
        // Only the lowest byte is the documented bool.
        assert_eq!(compare_subscribed(0xffff_ff01, Ok(true)), "shadow match");
        assert_eq!(compare_subscribed(0, Ok(true)), "shadow mismatch");
        assert_eq!(compare_subscribed(1, Ok(false)), "shadow mismatch");
        assert_eq!(compare_subscribed(2, Ok(true)), "shadow mismatch");
        assert_eq!(
            compare_subscribed(1, Err("not_signed_in".to_owned())),
            "shadow: no Reborn answer (not_signed_in)"
        );
    }

    const SESSION: &str =
        r#"{"id":1,"response":{"type":"session","version":{"major":0,"minor":2},"app_id":332950}}"#;
    const STATS_OK: &str = r#"{"event":{"type":"stats_received","result":1,"achievement_names":[{"name":"A","display_name":"B"}]}}"#;

    const WAIT: Duration = Duration::from_secs(30);

    #[test]
    fn the_stats_shadow_requests_stats_and_reads_the_event() {
        let mut link = ShadowLink::default();
        let answer = link.stats_result(
            4242,
            DARK_DECEPTION,
            || {
                Ok(scripted(&[
                    SESSION,
                    r#"{"id":2,"response":{"type":"accepted"}}"#,
                    STATS_OK,
                    r#"{"id":3,"response":{"type":"unavailable","reason":"offline"}}"#,
                ]))
            },
            WAIT,
        );
        assert_eq!(answer, Ok(1));
        let again = link.stats_result(
            4242,
            DARK_DECEPTION,
            || Err(io::Error::other("not again")),
            WAIT,
        );
        assert_eq!(again, Err("offline".to_owned()));
        let State::Ready(client) = &link.state else {
            panic!("no session");
        };
        // RequestStats carries no app ID: rebornd answers for the
        // session's own app only.
        assert_eq!(
            sent(client),
            "{\"id\":1,\"request\":{\"type\":\"shadow_hello\",\"version\":{\"major\":0,\"minor\":2},\"pid\":4242,\"app_id\":332950}}\n\
             {\"id\":2,\"request\":{\"type\":\"request_stats\"}}\n\
             {\"id\":3,\"request\":{\"type\":\"request_stats\"}}\n"
        );
    }

    #[test]
    fn ownership_and_stats_share_one_session() {
        let mut link = ShadowLink::default();
        let mut connects = 0;
        let mut connect = || {
            connects += 1;
            Ok(scripted(&[
                SESSION,
                r#"{"id":2,"response":{"type":"ownership","owned":true}}"#,
                r#"{"id":3,"response":{"type":"accepted"}}"#,
                STATS_OK,
            ]))
        };
        assert_eq!(link.subscribed(1, DARK_DECEPTION, &mut connect), Ok(true));
        assert_eq!(
            link.stats_result(1, DARK_DECEPTION, &mut connect, WAIT),
            Ok(1)
        );
        assert_eq!(connects, 1);
    }

    #[test]
    fn a_stats_event_that_never_comes_is_no_answer() {
        // A plain reader cannot wait: the end of the script closes it, and
        // the broken link is not tried again.
        let mut link = ShadowLink::default();
        let answer = link.stats_result(
            1,
            DARK_DECEPTION,
            || {
                Ok(scripted(&[
                    SESSION,
                    r#"{"id":2,"response":{"type":"accepted"}}"#,
                ]))
            },
            WAIT,
        );
        assert_eq!(answer, Err("rebornd closed the connection".to_owned()));
        assert!(matches!(link.state, State::Failed(_)));

        // A response where an event belongs is unexpected.
        let mut client = scripted(&[r#"{"id":9,"response":{"type":"accepted"}}"#]);
        assert!(matches!(
            client.stats_event(WAIT),
            Err(DaemonError::Unexpected)
        ));
    }

    /// A reader that blocks until a chunk arrives, like the named pipe.
    struct Pipe(mpsc::Receiver<Vec<u8>>, Vec<u8>);

    impl Read for Pipe {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.1.is_empty() {
                match self.0.recv() {
                    Ok(chunk) => self.1 = chunk,
                    Err(_) => return Ok(0),
                }
            }
            let n = buf.len().min(self.1.len());
            buf[..n].copy_from_slice(&self.1[..n]);
            self.1.drain(..n);
            Ok(n)
        }
    }

    #[test]
    fn a_line_thread_stops_waiting_without_cutting_a_line() -> Result<(), DaemonError> {
        let (pipe, chunks) = mpsc::channel();
        let mut lines = LineThread::spawn(BufReader::new(Pipe(chunks, Vec::new())))?;
        let short = Some(Duration::from_millis(20));
        assert_eq!(lines.next_line(short)?, None, "nothing sent");
        // Half a line: the wait runs out, the half is kept.
        let _ = pipe.send(br#"{"event":{"type":"stats_rec"#.to_vec());
        assert_eq!(lines.next_line(short)?, None, "half a line");
        let _ = pipe.send(br#"eived","result":2,"achievement_names":[]}}"#.to_vec());
        let _ = pipe.send(b"\n".to_vec());
        let mut client = Client::new(lines, Vec::new());
        assert_eq!(client.stats_event(Duration::from_secs(10))?, Some(2));
        drop(pipe);
        assert!(matches!(
            client.stats_event(Duration::from_secs(10)),
            Err(DaemonError::Closed)
        ));
        Ok(())
    }

    #[test]
    fn a_stats_wait_that_runs_out_keeps_the_session() {
        let (pipe, chunks) = mpsc::channel::<Vec<u8>>();
        let mut link = ShadowLink::default();
        let connect = || {
            let lines = LineThread::spawn(BufReader::new(Pipe(chunks, Vec::new())))?;
            for line in [SESSION, r#"{"id":2,"response":{"type":"accepted"}}"#] {
                let _ = pipe.send(format!("{line}\n").into_bytes());
            }
            Ok(Client::new(lines, Vec::new()))
        };
        let answer = link.stats_result(1, DARK_DECEPTION, connect, Duration::from_millis(20));
        assert_eq!(answer, Err("no stats event within 0 s".to_owned()));
        assert!(matches!(link.state, State::Ready(_)));
    }

    #[test]
    fn a_late_stats_event_is_skipped_or_taken_as_documented() {
        let (pipe, chunks) = mpsc::channel::<Vec<u8>>();
        let send = |line: &str| {
            let _ = pipe.send(format!("{line}\n").into_bytes());
        };
        let mut link = ShadowLink::default();
        let short = Duration::from_millis(20);
        let connect = || {
            let lines = LineThread::spawn(BufReader::new(Pipe(chunks, Vec::new())))?;
            send(SESSION);
            send(r#"{"id":2,"response":{"type":"accepted"}}"#);
            Ok(Client::new(lines, Vec::new()))
        };
        let answer = link.stats_result(1, DARK_DECEPTION, connect, short);
        assert_eq!(answer, Err("no stats event within 0 s".to_owned()));

        // The late event arrives before the next answer: the next question
        // skips it and gets its own answer.
        send(r#"{"event":{"type":"stats_received","result":2,"achievement_names":[]}}"#);
        send(r#"{"id":3,"response":{"type":"ownership","owned":true}}"#);
        assert_eq!(
            link.subscribed(1, DARK_DECEPTION, || Err(io::Error::other("no"))),
            Ok(true)
        );

        // Late again, then a stats question: `ask` skips the event that
        // comes before `Accepted` and the event after it is taken; for one
        // run `rebornd` sends the same cached answer each time.
        send(r#"{"event":{"type":"stats_received","result":2,"achievement_names":[]}}"#);
        send(r#"{"id":4,"response":{"type":"accepted"}}"#);
        send(r#"{"event":{"type":"stats_received","result":2,"achievement_names":[]}}"#);
        assert_eq!(
            link.stats_result(
                1,
                DARK_DECEPTION,
                || Err(io::Error::other("no")),
                Duration::from_secs(10)
            ),
            Ok(2)
        );
        assert!(matches!(link.state, State::Ready(_)));
    }

    fn official(size: usize, app_id: u32, result: u32) -> OfficialStats {
        let mut first = [0_u8; 24];
        first[..4].copy_from_slice(&app_id.to_le_bytes());
        first[8..12].copy_from_slice(&result.to_le_bytes());
        // The bytes read as the Steam ID play no part.
        first[12..20].fill(0x5a);
        OfficialStats::new(size, &first).unwrap_or_else(|| panic!("too short"))
    }

    #[test]
    fn a_stats_callback_matches_on_size_app_and_result_only() {
        let compare = |official, reborn| compare_stats_received(official, DARK_DECEPTION, reborn);
        assert_eq!(
            compare(official(24, DARK_DECEPTION, 1), Ok(1)),
            "shadow match"
        );
        assert_eq!(
            compare(official(24, DARK_DECEPTION, 2), Ok(2)),
            "shadow match"
        );
        assert_eq!(
            compare(official(24, DARK_DECEPTION, 1), Ok(2)),
            "shadow mismatch",
            "another result"
        );
        assert_eq!(
            compare(official(24, 480, 1), Ok(1)),
            "shadow mismatch",
            "another app"
        );
        assert_eq!(
            compare(official(16, DARK_DECEPTION, 1), Ok(1)),
            "shadow mismatch",
            "another size"
        );
        assert_eq!(
            compare(official(24, DARK_DECEPTION, 1), Err("offline".to_owned())),
            "shadow: no Reborn answer (offline)"
        );
        // Bytes 12.. are never read: only the first 12 are needed.
        assert_eq!(
            OfficialStats::new(24, &[0; STATS_READ]),
            Some(official(24, 0, 0))
        );
        assert_eq!(OfficialStats::new(24, &[0; STATS_READ - 1]), None);
    }
}
