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
//! Whether an app is owned is account data (ADR 0055): the log gets only
//! `shadow match` or `shadow mismatch` ([`compare_subscribed`]), never the
//! values.

use std::io::{self, BufRead, BufReader, Read, Write};

use reborn_shim_protocol::{
    DaemonMessage, PROTOCOL_VERSION, RefusalReason, Request, Response, ShimMessage,
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

/// One connection to `rebornd`'s shim socket: requests with increasing
/// `id`s, one answer each.
pub struct Client<R, W> {
    reader: R,
    writer: W,
    next_id: u64,
}

impl<R: BufRead, W: Write> Client<R, W> {
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
            let mut reply = String::new();
            let read = (&mut self.reader).take(MAX_LINE).read_line(&mut reply)?;
            if read == 0 {
                return Err(DaemonError::Closed);
            }
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
    let stream = std::sync::Arc::new(Stream::connect(name)?);
    let reader = Socket(std::sync::Arc::clone(&stream));
    Ok(Client::new(BufReader::new(reader), Socket(stream)))
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

impl<R: BufRead, W: Write> Default for ShadowLink<R, W> {
    fn default() -> Self {
        Self {
            state: State::NotYet,
        }
    }
}

impl<R: BufRead, W: Write> ShadowLink<R, W> {
    /// Reborn's answer to `BIsSubscribed` for `app_id`, the app given to
    /// `reborn-observe arm --app`: whether `rebornd` says the account owns
    /// it, or why there is no answer. On the first call it connects with
    /// `connect` and claims the shadow session for process `pid`.
    pub fn subscribed(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
    ) -> Result<bool, String> {
        if matches!(self.state, State::NotYet) {
            self.state = match open(pid, app_id, connect) {
                Ok(client) => State::Ready(client),
                Err(why) => State::Failed(why),
            };
        }
        let client = match &mut self.state {
            State::Ready(client) => client,
            State::Failed(why) => return Err(why.clone()),
            State::NotYet => return Err("no session".to_owned()),
        };
        match client.owned(app_id) {
            Ok(Ok(owned)) => Ok(owned),
            Ok(Err(reason)) => Err(reason_name(reason)),
            Err(error) => {
                let why = error.to_string();
                self.state = State::Failed(why.clone());
                Err(why)
            }
        }
    }
}

fn open<R: BufRead, W: Write>(
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
}
