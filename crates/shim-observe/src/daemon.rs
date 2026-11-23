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
//! - For `SteamFriends017` position 0 (`GetPersonaName`, experiment 33,
//!   under A1; ADR 0056 decision 2) the proxy sends only the
//!   SHA-256 of the text the official client returned
//!   (`ComparePersonaName`, [`ShadowLink::persona_name`]); `rebornd`
//!   answers whether it is the account's persona name. Only to a `rebornd`
//!   of protocol 0.3 or later.
//! - For the cloud reads `GetFileSize` and `FileRead` the proxy sends
//!   `CloudFileSize` and `CloudRead` (protocol 0.2, so no further version
//!   gate) with the file name the game passed, on the same shadow session
//!   ([`ShadowLink::file_size`], [`ShadowLink::file_read`]); `rebornd`
//!   answers from the session's separate shadow folder, filled from
//!   Valve's cloud when `arm` registered the app (ADR 0055, stage 1).
//! - For `GetNumAchievements` and `GetAchievementName`
//!   (`STEAMUSERSTATS_INTERFACE_VERSION011`/`012` positions 14 and 15, ADR
//!   0059 decision 2) the proxy keeps the official count and the SHA-256 of
//!   each official name ([`AchievementQueue`]) until Reborn's list arrives
//!   with the event `StatsReceived` of a `RequestStats` on the same shadow
//!   session ([`ShadowLink::achievements`]; protocol 0.2, so no further
//!   version gate). Reborn's names are hashed as the event is read and
//!   kept only as digests ([`RebornAchievements`]).
//!
//! Whether an app is owned, the stats, the persona name, file names, file
//! sizes and file contents are account data (ADR 0055, 0056): the log gets
//! only `shadow match` or `shadow mismatch` ([`compare_subscribed`],
//! [`compare_stats_received`], [`compare_persona_name`],
//! [`compare_file_size`], [`compare_file_read`], [`compare_achievement`])
//! or why there is no answer, never the values, no file name, no size and
//! no achievement name.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::mpsc;
use std::time::Duration;

use reborn_shim_protocol::{
    DaemonMessage, Event, NameDigest, PROTOCOL_VERSION, RefusalReason, Request, Response,
    ShimMessage, UnavailableReason, Version, decode, encode,
};

/// Longest line read from `rebornd`: a `CloudRead` answer carries up to
/// 16 MiB (the largest cloud file `rebornd` hands out,
/// `reborn_core::run_cloud::MAX_FILE`) as base64, 4/3 as long (ADR 0055,
/// "Cloud"), plus the JSON around it.
const MAX_LINE: u64 = 16 * 1024 * 1024 / 3 * 4 + 64 * 1024;

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

/// Lines read ahead at most; the reading thread then waits. Worst case in
/// memory: this many lines of up to [`MAX_LINE`] (about 21 MiB, a 16 MiB
/// `CloudRead` answer) plus the one being read, about 360 MiB; in practice
/// the proxy asks one question at a time and waits for its answer, so at
/// most one long line is queued beside a few short stats events.
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
            let mut reply = reply.into_bytes();
            let message = std::str::from_utf8(&reply)
                .map_err(|_| DaemonError::Unexpected)
                .and_then(|text| {
                    decode::<DaemonMessage>(text).map_err(|_| DaemonError::Unexpected)
                });
            // A stale `StatsReceived` skipped here holds achievement names:
            // the line is zeroed (best effort) like in `stats_answer`.
            wipe(&mut reply);
            match message? {
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

    /// `ShadowHello`: the daemon's protocol version when the session for
    /// `app_id` is this process's, or why `rebornd` refused.
    pub fn shadow_hello(
        &mut self,
        pid: u32,
        app_id: u32,
    ) -> Result<Result<Version, RefusalReason>, DaemonError> {
        match self.ask(Request::ShadowHello {
            version: PROTOCOL_VERSION,
            pid,
            app_id,
        })? {
            Response::Session(session) if session.app_id == app_id => Ok(Ok(session.version)),
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

    /// `CloudFileSize { name }`: the size of the file in the session's
    /// cloud folder (`None` if there is none), or why `rebornd` cannot say.
    pub fn cloud_file_size(
        &mut self,
        name: &str,
    ) -> Result<Result<Option<u32>, UnavailableReason>, DaemonError> {
        match self.ask(Request::CloudFileSize {
            name: name.to_owned(),
        })? {
            Response::FileSize { size } => Ok(Ok(size)),
            Response::Unavailable { reason } => Ok(Err(reason)),
            _ => Err(DaemonError::Unexpected),
        }
    }

    /// `CloudRead { name, max }`: at most `max` bytes from the start of the
    /// file in the session's cloud folder (`None` if there is none), or why
    /// `rebornd` cannot say. More than `max` bytes is an unexpected answer.
    pub fn cloud_read(
        &mut self,
        name: &str,
        max: u32,
    ) -> Result<Result<Option<Vec<u8>>, UnavailableReason>, DaemonError> {
        match self.ask(Request::CloudRead {
            name: name.to_owned(),
            max,
        })? {
            Response::FileData { data } => {
                let data = data.map(|data| data.0);
                if data.as_ref().is_some_and(|data| data.len() > max as usize) {
                    return Err(DaemonError::Unexpected);
                }
                Ok(Ok(data))
            }
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

    /// `ComparePersonaName { sha256 }`: whether `rebornd` says the
    /// account's persona name has this digest, or why it cannot say.
    pub fn persona_name_matches(
        &mut self,
        sha256: NameDigest,
    ) -> Result<Result<bool, UnavailableReason>, DaemonError> {
        match self.ask(Request::ComparePersonaName { sha256 })? {
            Response::PersonaNameCompared { matches } => Ok(Ok(matches)),
            Response::Unavailable { reason } => Ok(Err(reason)),
            _ => Err(DaemonError::Unexpected),
        }
    }

    /// Waits at most `wait` for the event `StatsReceived` of an accepted
    /// `RequestStats`: `Ok(Some(result))` with Valve's `EResult`, `Ok(None)`
    /// when the wait ran out (`rebornd` sends no event for a request that
    /// failed, ADR 0055 "Honest failure"). Only the result: callback
    /// 0x44d carries no names ([`Self::stats_answer`] keeps their digests).
    /// No request is open, so a response here is unexpected.
    pub fn stats_event(&mut self, wait: Duration) -> Result<Option<i32>, DaemonError> {
        Ok(self.stats_answer(wait)?.map(|answer| answer.result))
    }

    /// Like [`Self::stats_event`], with Reborn's achievement list as
    /// digests ([`RebornAchievements::from_event`]): the names are hashed
    /// here and their text is zeroed (best effort) before this returns.
    pub fn stats_answer(
        &mut self,
        wait: Duration,
    ) -> Result<Option<RebornAchievements>, DaemonError> {
        let Some(line) = self.reader.next_line(Some(wait))? else {
            return Ok(None);
        };
        let mut line = line.into_bytes();
        let message = std::str::from_utf8(&line)
            .map_err(|_| DaemonError::Unexpected)
            .and_then(|text| decode::<DaemonMessage>(text).map_err(|_| DaemonError::Unexpected));
        wipe(&mut line);
        // `StatsReceived` is the only event of protocols 0.2 and 0.3.
        match message? {
            DaemonMessage::Event {
                event:
                    Event::StatsReceived {
                        result,
                        achievement_names,
                    },
            } => Ok(Some(RebornAchievements::from_event(
                result,
                achievement_names,
            ))),
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
    /// Reborn's achievement list from the first `StatsReceived` read on
    /// this session, as digests. `rebornd` sends the same cached answer for
    /// every `RequestStats` of a run (`apps/rebornd/src/shim_stats.rs`), so
    /// it is asked for once.
    achievements: Option<RebornAchievements>,
}

enum State<R, W> {
    NotYet,
    /// The session, with the daemon's protocol version.
    Ready(Client<R, W>, Version),
    Failed(String),
}

impl<R: Lines, W: Write> Default for ShadowLink<R, W> {
    fn default() -> Self {
        Self {
            state: State::NotYet,
            achievements: None,
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
        let (client, _) = self.client(pid, app_id, connect)?;
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
        let answer = self.request_stats(pid, app_id, connect, wait)?;
        let result = answer.result;
        self.achievements.get_or_insert(answer);
        Ok(result)
    }

    /// Reborn's achievement list for the `GetNumAchievements` and
    /// `GetAchievementName` comparison (ADR 0059, decision 2): from the
    /// first `StatsReceived` this session read (also one read for 0x44d),
    /// otherwise from a `RequestStats` sent now, waiting at most `wait` for
    /// its event. A wait that runs out or a refusal is not kept: the next
    /// question asks again, as [`Self::stats_result`] does. An event with a
    /// result other than OK is kept (it is Valve's real answer for the run,
    /// which `rebornd` repeats): every later comparison then says why there
    /// is no list, without asking again.
    pub fn achievements(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
        wait: Duration,
    ) -> Result<&RebornAchievements, String> {
        if self.achievements.is_none() {
            let answer = self.request_stats(pid, app_id, connect, wait)?;
            self.achievements = Some(answer);
        }
        self.achievements
            .as_ref()
            .ok_or_else(|| "no achievement list".to_owned())
    }

    /// One `RequestStats` and its event, at most `wait` for the event.
    fn request_stats(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
        wait: Duration,
    ) -> Result<RebornAchievements, String> {
        let (client, _) = self.client(pid, app_id, connect)?;
        let answer = match client.request_stats() {
            Ok(Ok(())) => client.stats_answer(wait),
            Ok(Err(reason)) => return Err(reason_name(reason)),
            Err(error) => Err(error),
        };
        match answer {
            Ok(Some(answer)) => Ok(answer),
            Ok(None) => Err(format!("no stats event within {} s", wait.as_secs())),
            Err(error) => Err(self.fail(error)),
        }
    }

    /// Reborn's side of the `SteamFriends017` position 0 comparison (ADR
    /// 0056): whether `rebornd` says the account's persona name has the
    /// digest `sha256`, or why there is no answer. Connects and claims the
    /// session as [`Self::subscribed`] does, if not done yet. A daemon older
    /// than protocol 0.3 is not asked (it would close the connection on the
    /// unknown line, ending the other comparisons too).
    pub fn persona_name(
        &mut self,
        pid: u32,
        app_id: u32,
        sha256: NameDigest,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
    ) -> Result<bool, String> {
        let (client, version) = self.client(pid, app_id, connect)?;
        if !version.has_persona_name_comparison() {
            return Err(format!(
                "rebornd speaks protocol {}.{}, the comparison needs 0.3",
                version.major, version.minor
            ));
        }
        let answer = client.persona_name_matches(sha256);
        match answer {
            Ok(Ok(matches)) => Ok(matches),
            Ok(Err(reason)) => Err(reason_name(reason)),
            Err(error) => Err(self.fail(error)),
        }
    }

    /// Reborn's answer to `GetFileSize(name)` (ADR 0055, stage 1): the
    /// size `rebornd` reads in the shadow folder, `None` for no such file,
    /// or why there is no answer. Same session as [`Self::subscribed`];
    /// `CloudFileSize` is a protocol 0.2 message, which every daemon that
    /// gives a shadow session speaks.
    pub fn file_size(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
        name: &str,
    ) -> Result<Option<u32>, String> {
        let (client, _) = self.client(pid, app_id, connect)?;
        let answer = client.cloud_file_size(name);
        match answer {
            Ok(Ok(size)) => Ok(size),
            Ok(Err(reason)) => Err(reason_name(reason)),
            Err(error) => Err(self.fail(error)),
        }
    }

    /// Reborn's answer to `FileRead(name, buffer, max)` (ADR 0055, stage
    /// 1): at most `max` bytes `rebornd` reads in the shadow folder, `None`
    /// for no such file, or why there is no answer. Same session as
    /// [`Self::subscribed`] (`CloudRead` is protocol 0.2).
    pub fn file_read(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
        name: &str,
        max: u32,
    ) -> Result<Option<Vec<u8>>, String> {
        let (client, _) = self.client(pid, app_id, connect)?;
        let answer = client.cloud_read(name, max);
        match answer {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(reason)) => Err(reason_name(reason)),
            Err(error) => Err(self.fail(error)),
        }
    }

    /// The session's client and the daemon's protocol version, made on the
    /// first question (one attempt per run).
    fn client(
        &mut self,
        pid: u32,
        app_id: u32,
        connect: impl FnOnce() -> io::Result<Client<R, W>>,
    ) -> Result<(&mut Client<R, W>, Version), String> {
        if matches!(self.state, State::NotYet) {
            self.state = match open(pid, app_id, connect) {
                Ok((client, version)) => State::Ready(client, version),
                Err(why) => State::Failed(why),
            };
        }
        match &mut self.state {
            State::Ready(client, version) => Ok((client, *version)),
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
) -> Result<(Client<R, W>, Version), String> {
    let mut client = connect().map_err(|error| DaemonError::Io(error).to_string())?;
    match client.shadow_hello(pid, app_id) {
        Ok(Ok(version)) => Ok((client, version)),
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
        UnavailableReason::NotReceived => "not_received",
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

/// The shadow note for `SteamFriends017` position 0 (ADR 0056): `client`
/// is the digest of the official client's result text, or why there is no
/// text; `reborn` is `rebornd`'s comparison. Only match, mismatch or a
/// reason; the text and the digest are never part of it.
pub fn compare_persona_name(
    client: Result<(), &str>,
    reborn: impl FnOnce() -> Result<bool, String>,
) -> String {
    match client {
        // No text at the result: not `GetPersonaName`'s documented answer.
        Err(why) => format!("shadow mismatch ({why})"),
        Ok(()) => match reborn() {
            Ok(true) => "shadow match".to_owned(),
            Ok(false) => "shadow mismatch".to_owned(),
            Err(why) => format!("shadow: no Reborn answer ({why})"),
        },
    }
}

/// Zeroes `bytes` (best effort: copies made earlier, such as the reading
/// thread's buffer, are not reached).
fn wipe(bytes: &mut [u8]) {
    bytes.fill(0);
    // Keeps the zeroing from being optimised away as a dead store.
    std::hint::black_box(&*bytes);
}

/// The SHA-256 of an achievement's API name (ADR 0059, decision 2). ADR
/// 0055 treats the names as account data in logs, so `Debug` never shows
/// the digest and there is no `Display`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct AchievementDigest([u8; 32]);

impl AchievementDigest {
    /// The digest of the bytes of a name (UTF-8, without a NUL).
    pub fn of(bytes: &[u8]) -> Self {
        use sha2::{Digest, Sha256};
        Self(Sha256::digest(bytes).into())
    }
}

impl std::fmt::Debug for AchievementDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AchievementDigest(<redacted>)")
    }
}

/// Most entries of Reborn's achievement list kept for the comparison: a
/// bound chosen here, not a documented limit. A longer list is not
/// compared.
pub const MAX_REBORN_ACHIEVEMENTS: usize = 10_000;

/// Reborn's side of the achievement comparison, from one `StatsReceived`:
/// Valve's `EResult` and the internal (API) names in Reborn's order
/// (`reborn_cm::achievements::UserStats::achievements`: every `bits`
/// entry of every stat, in schema order; `docs/protocol/achievements.md`)
/// as SHA-256 digests, or why there is no list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RebornAchievements {
    /// The `EResult` Valve sent; 1 is OK.
    pub result: i32,
    names: Result<Vec<AchievementDigest>, &'static str>,
}

impl RebornAchievements {
    /// From the event's fields: the API names are hashed, then their text
    /// and the display names are zeroed (best effort) and dropped. With a
    /// result other than OK there is no list (the event carries no names);
    /// the result is kept all the same, for the 0x44d comparison.
    pub fn from_event(result: i32, names: Vec<reborn_shim_protocol::AchievementName>) -> Self {
        let digests = if result != 1 {
            Err("Valve's stats result was not OK")
        } else if names.len() > MAX_REBORN_ACHIEVEMENTS {
            Err("more achievements than the proxy compares")
        } else {
            Ok(names
                .iter()
                .map(|name| AchievementDigest::of(name.name.as_bytes()))
                .collect())
        };
        for name in names {
            wipe(&mut name.name.into_bytes());
            wipe(&mut name.display_name.into_bytes());
        }
        Self {
            result,
            names: digests,
        }
    }
}

/// What the official client gave the game at one achievement call (ADR
/// 0059, decision 2). Never a name: only its digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OfficialAchievement {
    /// `GetNumAchievements`: its result (a `uint32` on the public
    /// `ISteamUserStats` page; the low 32 bits of `rax`).
    Count(u32),
    /// `GetAchievementName(index)`: the index the game passed (low 32 bits
    /// of `rdx`, the page's `uint32 iAchievement`) and the digest of the
    /// text the result pointed to, or why there was none.
    Name {
        /// The index the game asked for.
        index: u32,
        /// The digest of the official text.
        text: Result<AchievementDigest, &'static str>,
    },
}

/// The shadow note for `GetNumAchievements` and `GetAchievementName`
/// (`STEAMUSERSTATS_INTERFACE_VERSION011`/`012` positions 14 and 15, ADR
/// 0059 decision 2): the official count against the length of Reborn's
/// list, the official text at index `i` against Reborn's `i`-th API name.
/// For an index beyond Reborn's list Reborn's answer is the empty text, as
/// the public `ISteamUserStats` page says `GetAchievementName` returns for
/// an invalid index (`docs/shim/meanings.md`, loop iteration 16a). That
/// index `i` is Reborn's `i`-th achievement is **not sourced**: this
/// comparison tests it, so a mismatch is a finding, not a fault of the
/// tool. Only match, mismatch or the reason is logged, never a name or a
/// digest.
pub fn compare_achievement(
    official: &OfficialAchievement,
    reborn: Result<&RebornAchievements, String>,
) -> String {
    let names = match reborn.map(|reborn| &reborn.names) {
        Ok(Ok(names)) => names,
        Ok(Err(why)) => return format!("shadow: no Reborn answer ({why})"),
        Err(why) => return format!("shadow: no Reborn answer ({why})"),
    };
    let same = match official {
        OfficialAchievement::Count(count) => usize::try_from(*count) == Ok(names.len()),
        OfficialAchievement::Name { text: Err(why), .. } => {
            return format!("shadow mismatch ({why})");
        }
        OfficialAchievement::Name {
            index,
            text: Ok(digest),
        } => {
            let reborn = usize::try_from(*index)
                .ok()
                .and_then(|index| names.get(index).copied())
                .unwrap_or_else(|| AchievementDigest::of(b""));
            *digest == reborn
        }
    };
    if same {
        "shadow match".to_owned()
    } else {
        "shadow mismatch".to_owned()
    }
}

/// Official achievement answers kept at most until Reborn's list arrives;
/// later ones are logged as skipped. With the start of a log line (about
/// 80 bytes) and a digest each, about 30 KiB at most.
pub const MAX_WAITING_ACHIEVEMENTS: usize = 256;

/// Official achievement answers waiting for Reborn's list (ADR 0059,
/// decision 2). The official client answers within about 25 ms of the
/// game's `RequestCurrentStats`, Reborn's list comes with `StatsReceived`
/// about 300 ms later (ADR 0059, "A timing observation"), so the answers
/// are kept here, as digests, and compared when the list is there. The
/// game's thread pushes; the worker takes them all at once. Only one wake
/// for the worker is open at a time, so a game that asks for many names
/// does not fill the worker's queue.
#[derive(Debug, Default)]
pub struct AchievementQueue {
    waiting: Vec<(String, OfficialAchievement)>,
    woken: bool,
}

/// What [`AchievementQueue::push`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Pushed {
    /// Kept; the worker has to be woken for it.
    Wake,
    /// Kept; a wake is already open.
    Kept,
    /// Not kept: [`MAX_WAITING_ACHIEVEMENTS`] wait already. The start of
    /// the log line comes back for the "skipped" note.
    Full(String),
}

impl AchievementQueue {
    /// An empty queue.
    pub const fn new() -> Self {
        Self {
            waiting: Vec::new(),
            woken: false,
        }
    }

    /// Keeps one official answer with the start of its log line.
    pub fn push(&mut self, prefix: String, official: OfficialAchievement) -> Pushed {
        if self.waiting.len() >= MAX_WAITING_ACHIEVEMENTS {
            return Pushed::Full(prefix);
        }
        self.waiting.push((prefix, official));
        if self.woken {
            Pushed::Kept
        } else {
            self.woken = true;
            Pushed::Wake
        }
    }

    /// Everything waiting, in the order the game's calls returned; the next
    /// push wakes the worker again.
    pub fn take(&mut self) -> Vec<(String, OfficialAchievement)> {
        self.woken = false;
        std::mem::take(&mut self.waiting)
    }
}

/// The SHA-1 of `bytes`; ADR 0055 compares `FileRead` "by size and SHA-1
/// only". Computed in memory, never logged.
pub fn sha1_of(bytes: &[u8]) -> [u8; 20] {
    use sha1::{Digest, Sha1};
    Sha1::digest(bytes).into()
}

/// The shadow note for `GetFileSize` (`STEAMREMOTESTORAGE_INTERFACE_VERSION014`
/// position 15, `docs/shim/meanings.md`, experiment 1): the official
/// client's result is the low 32 bits of `rax` (the recorded 44875 was
/// the file's size), Reborn's is the size `rebornd` read in the shadow
/// folder. A file Reborn does not have is not compared: what the official
/// client answers for a missing file is not recorded (ADR 0055). Only
/// match, mismatch or the reason is logged (account data).
pub fn compare_file_size(rax: u64, reborn: Result<Option<u32>, String>) -> String {
    match reborn {
        Ok(Some(size)) => {
            if rax as u32 == size {
                "shadow match".to_owned()
            } else {
                "shadow mismatch".to_owned()
            }
        }
        Ok(None) => "shadow: no Reborn answer (no such file in the shadow folder)".to_owned(),
        Err(why) => format!("shadow: no Reborn answer ({why})"),
    }
}

/// What the official `FileRead` put into the game's buffer: how many bytes
/// (its result) and their SHA-1. The bytes themselves are not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfficialRead {
    /// The official result: bytes read into the buffer.
    pub len: usize,
    /// The SHA-1 of those bytes.
    pub sha1: [u8; 20],
}

/// The shadow note for `FileRead` (`STEAMREMOTESTORAGE_INTERFACE_VERSION014`
/// position 1, `docs/shim/meanings.md`, experiment 1): compared by size
/// and SHA-1 only (ADR 0055, stage 1) with the bytes `rebornd` read from
/// the shadow folder, at most the game's buffer size. Only match,
/// mismatch or the reason is logged (account data).
pub fn compare_file_read(
    official: OfficialRead,
    reborn: Result<Option<Vec<u8>>, String>,
) -> String {
    match reborn {
        Ok(Some(data)) => {
            if data.len() == official.len && sha1_of(&data) == official.sha1 {
                "shadow match".to_owned()
            } else {
                "shadow mismatch".to_owned()
            }
        }
        Ok(None) => "shadow: no Reborn answer (no such file in the shadow folder)".to_owned(),
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
        let State::Ready(client, _) = &link.state else {
            panic!("no session");
        };
        assert_eq!(
            sent(client),
            "{\"id\":1,\"request\":{\"type\":\"shadow_hello\",\"version\":{\"major\":0,\"minor\":3},\"pid\":4242,\"app_id\":332950}}\n\
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
    fn cloud_reads_go_to_the_shadow_session_with_the_games_name() {
        let mut link = ShadowLink::default();
        let size = link.file_size(
            4242,
            2_143_030,
            || {
                Ok(scripted(&[
                    r#"{"id":1,"response":{"type":"session","version":{"major":0,"minor":2},"app_id":2143030}}"#,
                    r#"{"id":2,"response":{"type":"file_size","size":11}}"#,
                    r#"{"id":3,"response":{"type":"file_data","data":"eyJsZXZlbCI6M30="}}"#,
                    r#"{"id":4,"response":{"type":"file_size","size":null}}"#,
                    r#"{"id":5,"response":{"type":"unavailable","reason":"not_allowed"}}"#,
                    r#"{"id":6,"response":{"type":"file_data","data":"eyJsZXZlbCI6M30="}}"#,
                ]))
            },
            "MainPlayerData.json",
        );
        assert_eq!(size, Ok(Some(11)));
        let never = || -> io::Result<_> { panic!("connected again") };
        assert_eq!(
            link.file_read(4242, 2_143_030, never, "MainPlayerData.json", 4096),
            Ok(Some(b"{\"level\":3}".to_vec()))
        );
        assert_eq!(
            link.file_size(4242, 2_143_030, never, "other.json"),
            Ok(None)
        );
        assert_eq!(
            link.file_read(4242, 2_143_030, never, "other.json", 10),
            Err("not_allowed".to_owned())
        );
        // More bytes than the game's buffer: not Reborn's answer.
        assert_eq!(
            link.file_read(4242, 2_143_030, never, "MainPlayerData.json", 4),
            Err("rebornd sent an unexpected answer".to_owned())
        );
        let State::Failed(_) = &link.state else {
            panic!("an unexpected answer fails the link");
        };
    }

    #[test]
    fn cloud_reads_skip_a_late_stats_event() {
        // The stats shadow may leave an event behind on the same session.
        let mut client = scripted(&[
            r#"{"event":{"type":"stats_received","result":1,"achievement_names":[]}}"#,
            r#"{"id":1,"response":{"type":"file_size","size":3}}"#,
        ]);
        assert!(matches!(client.cloud_file_size("a.sav"), Ok(Ok(Some(3)))));
    }

    #[test]
    fn the_cloud_requests_carry_the_name_and_the_buffer_size() {
        let mut client = scripted(&[
            r#"{"id":1,"response":{"type":"file_size","size":3}}"#,
            r#"{"id":2,"response":{"type":"file_data","data":null}}"#,
        ]);
        assert!(matches!(client.cloud_file_size("a.sav"), Ok(Ok(Some(3)))));
        assert!(matches!(client.cloud_read("a.sav", 44_875), Ok(Ok(None))));
        assert_eq!(
            sent(&client),
            "{\"id\":1,\"request\":{\"type\":\"cloud_file_size\",\"name\":\"a.sav\"}}\n\
             {\"id\":2,\"request\":{\"type\":\"cloud_read\",\"name\":\"a.sav\",\"max\":44875}}\n"
        );
    }

    #[test]
    fn a_long_cloud_answer_is_read_whole() {
        // Cave Crawlers' recorded save (44875 bytes) is about 60 000 as
        // base64, close to the old 64 KiB line limit; larger saves up to
        // 16 MiB must fit too.
        let data = vec![7u8; 200_000];
        let line = encode(&DaemonMessage::Response {
            id: 1,
            response: Response::FileData {
                data: Some(reborn_shim_protocol::Bytes(data.clone())),
            },
        })
        .unwrap_or_default();
        let mut client = Client::new(Cursor::new(line.into_bytes()), Vec::new());
        assert!(matches!(
            client.cloud_read("big.sav", 200_000),
            Ok(Ok(Some(read))) if read == data
        ));
        const { assert!(MAX_LINE > (16 * 1024 * 1024_u64).div_ceil(3) * 4 + 64) };
    }

    #[test]
    fn file_sizes_are_compared_by_their_low_32_bits() {
        assert_eq!(compare_file_size(44_875, Ok(Some(44_875))), "shadow match");
        assert_eq!(
            compare_file_size(0xffff_ffff_0000_af4b, Ok(Some(44_875))),
            "shadow match"
        );
        assert_eq!(
            compare_file_size(44_877, Ok(Some(44_875))),
            "shadow mismatch"
        );
        assert_eq!(
            compare_file_size(0, Ok(None)),
            "shadow: no Reborn answer (no such file in the shadow folder)"
        );
        assert_eq!(
            compare_file_size(1, Err("not_allowed".to_owned())),
            "shadow: no Reborn answer (not_allowed)"
        );
    }

    #[test]
    fn file_reads_are_compared_by_size_and_sha1() {
        let bytes = b"{\"level\":3}";
        let official = OfficialRead {
            len: bytes.len(),
            sha1: sha1_of(bytes),
        };
        assert_eq!(
            compare_file_read(official, Ok(Some(bytes.to_vec()))),
            "shadow match"
        );
        assert_eq!(
            compare_file_read(official, Ok(Some(b"{\"level\":4}".to_vec()))),
            "shadow mismatch",
            "same size, other bytes"
        );
        assert_eq!(
            compare_file_read(official, Ok(Some(bytes[..4].to_vec()))),
            "shadow mismatch",
            "fewer bytes"
        );
        assert_eq!(
            compare_file_read(official, Ok(None)),
            "shadow: no Reborn answer (no such file in the shadow folder)"
        );
        // The SHA-1 of nothing is the known empty-input value.
        assert_eq!(
            sha1_of(b"")
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        // No note carries a value, a name or contents.
        for note in [
            compare_file_read(official, Ok(Some(bytes.to_vec()))),
            compare_file_size(11, Ok(Some(11))),
        ] {
            assert!(!note.contains("level") && !note.contains("11"), "{note}");
        }
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
        let State::Ready(client, _) = &link.state else {
            panic!("no session");
        };
        // RequestStats carries no app ID: rebornd answers for the
        // session's own app only.
        assert_eq!(
            sent(client),
            "{\"id\":1,\"request\":{\"type\":\"shadow_hello\",\"version\":{\"major\":0,\"minor\":3},\"pid\":4242,\"app_id\":332950}}\n\
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
        assert!(matches!(link.state, State::Ready(..)));
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
        assert!(matches!(link.state, State::Ready(..)));
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

    /// SHA-256 of `foo` (not anyone's name).
    const DIGEST: &str = "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae";

    fn digest() -> NameDigest {
        NameDigest::from_hex(DIGEST).unwrap_or_else(|| panic!("no digest"))
    }

    #[test]
    fn the_persona_name_is_compared_on_the_shadow_session() {
        let mut link = ShadowLink::default();
        let answer = link.persona_name(4242, DARK_DECEPTION, digest(), || {
            Ok(scripted(&[
                r#"{"id":1,"response":{"type":"session","version":{"major":0,"minor":3},"app_id":332950}}"#,
                r#"{"id":2,"response":{"type":"persona_name_compared","matches":true}}"#,
                r#"{"id":3,"response":{"type":"persona_name_compared","matches":false}}"#,
                r#"{"id":4,"response":{"type":"unavailable","reason":"not_received"}}"#,
                r#"{"id":5,"response":{"type":"ownership","owned":true}}"#,
            ]))
        });
        assert_eq!(answer, Ok(true));
        let no_connect = || -> io::Result<_> { panic!("connected again") };
        assert_eq!(
            link.persona_name(4242, DARK_DECEPTION, digest(), no_connect),
            Ok(false)
        );
        assert_eq!(
            link.persona_name(4242, DARK_DECEPTION, digest(), no_connect),
            Err("not_received".to_owned())
        );
        // Another kind of answer breaks the link.
        assert_eq!(
            link.persona_name(4242, DARK_DECEPTION, digest(), no_connect),
            Err("rebornd sent an unexpected answer".to_owned())
        );
        let State::Failed(_) = &link.state else {
            panic!("not failed");
        };
    }

    #[test]
    fn only_the_digest_crosses_the_pipe() {
        let mut client =
            scripted(&[r#"{"id":1,"response":{"type":"persona_name_compared","matches":true}}"#]);
        assert!(matches!(
            client.persona_name_matches(digest()),
            Ok(Ok(true))
        ));
        assert_eq!(
            sent(&client),
            format!(
                "{{\"id\":1,\"request\":{{\"type\":\"compare_persona_name\",\"sha256\":\"{DIGEST}\"}}}}\n"
            )
        );
    }

    #[test]
    fn the_persona_note_says_only_match_mismatch_or_why() {
        assert_eq!(compare_persona_name(Ok(()), || Ok(true)), "shadow match");
        assert_eq!(
            compare_persona_name(Ok(()), || Ok(false)),
            "shadow mismatch"
        );
        assert_eq!(
            compare_persona_name(Ok(()), || Err("not_signed_in".to_owned())),
            "shadow: no Reborn answer (not_signed_in)"
        );
        // Without a text, rebornd is not asked.
        assert_eq!(
            compare_persona_name(Err("no text at the result"), || panic!("asked")),
            "shadow mismatch (no text at the result)"
        );
    }

    #[test]
    fn an_older_rebornd_is_not_asked_and_ownership_keeps_working() {
        let mut link = ShadowLink::default();
        let answer = link.persona_name(4242, DARK_DECEPTION, digest(), || {
            Ok(scripted(&[
                r#"{"id":1,"response":{"type":"session","version":{"major":0,"minor":2},"app_id":332950}}"#,
                r#"{"id":2,"response":{"type":"ownership","owned":true}}"#,
            ]))
        });
        assert_eq!(
            answer,
            Err("rebornd speaks protocol 0.2, the comparison needs 0.3".to_owned())
        );
        let no_connect = || -> io::Result<_> { panic!("connected again") };
        assert_eq!(link.subscribed(4242, DARK_DECEPTION, no_connect), Ok(true));
        let State::Ready(client, _) = &link.state else {
            panic!("no session");
        };
        assert!(
            !sent(client).contains("compare_persona_name"),
            "nothing a 0.2 daemon cannot read"
        );
    }

    /// Made-up achievement API names, in Reborn's order.
    const STATS_THREE: &str = r#"{"event":{"type":"stats_received","result":1,"achievement_names":[{"name":"ACH_ONE","display_name":"One"},{"name":"ACH_TWO","display_name":"Two"},{"name":"ACH_THREE","display_name":"Three"}]}}"#;

    fn name(index: u32, text: &str) -> OfficialAchievement {
        OfficialAchievement::Name {
            index,
            text: Ok(AchievementDigest::of(text.as_bytes())),
        }
    }

    fn three() -> RebornAchievements {
        let mut client = scripted(&[STATS_THREE]);
        match client.stats_answer(WAIT) {
            Ok(Some(answer)) => answer,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_stats_event_keeps_only_digests_of_the_names() {
        let reborn = three();
        assert_eq!(reborn.result, 1);
        let shown = format!("{reborn:?}");
        for text in ["ACH_", "One", "Two", "Three"] {
            assert!(!shown.contains(text), "{shown}");
        }
        // Each name is compared at its own index only.
        for (index, text) in [(0, "ACH_ONE"), (1, "ACH_TWO"), (2, "ACH_THREE")] {
            assert_eq!(
                compare_achievement(&name(index, text), Ok(&reborn)),
                "shadow match"
            );
        }
        assert_eq!(
            compare_achievement(&name(0, "ACH_TWO"), Ok(&reborn)),
            "shadow mismatch",
            "another index"
        );
        assert_eq!(
            compare_achievement(&name(1, "One"), Ok(&reborn)),
            "shadow mismatch",
            "the display name is not the API name"
        );
        assert_eq!(
            compare_achievement(&name(0, "ach_one"), Ok(&reborn)),
            "shadow mismatch",
            "letter case counts"
        );
    }

    #[test]
    fn the_count_is_compared_with_the_length_of_the_list() {
        let reborn = three();
        let count = |n| compare_achievement(&OfficialAchievement::Count(n), Ok(&reborn));
        assert_eq!(count(3), "shadow match");
        assert_eq!(count(2), "shadow mismatch");
        assert_eq!(count(29), "shadow mismatch");
        let empty = RebornAchievements::from_event(1, Vec::new());
        assert_eq!(
            compare_achievement(&OfficialAchievement::Count(0), Ok(&empty)),
            "shadow match"
        );
    }

    #[test]
    fn an_index_beyond_the_list_expects_the_empty_text() {
        // The public page: an empty string for an invalid index.
        let reborn = three();
        assert_eq!(
            compare_achievement(&name(3, ""), Ok(&reborn)),
            "shadow match"
        );
        assert_eq!(
            compare_achievement(&name(u32::MAX, ""), Ok(&reborn)),
            "shadow match"
        );
        assert_eq!(
            compare_achievement(&name(3, "ACH_FOUR"), Ok(&reborn)),
            "shadow mismatch"
        );
        assert_eq!(
            compare_achievement(&name(0, ""), Ok(&reborn)),
            "shadow mismatch",
            "an empty text where Reborn has a name"
        );
        let no_text = OfficialAchievement::Name {
            index: 0,
            text: Err("no text at the result"),
        };
        assert_eq!(
            compare_achievement(&no_text, Ok(&reborn)),
            "shadow mismatch (no text at the result)"
        );
    }

    #[test]
    fn without_a_list_nothing_is_compared() {
        assert_eq!(
            compare_achievement(&name(0, "ACH_ONE"), Err("offline".to_owned())),
            "shadow: no Reborn answer (offline)"
        );
        let failed = RebornAchievements::from_event(2, Vec::new());
        assert_eq!(
            compare_achievement(&OfficialAchievement::Count(0), Ok(&failed)),
            "shadow: no Reborn answer (Valve's stats result was not OK)"
        );
        let names = (0..=MAX_REBORN_ACHIEVEMENTS)
            .map(|index| reborn_shim_protocol::AchievementName {
                name: format!("ACH_{index}"),
                display_name: String::new(),
            })
            .collect();
        let long = RebornAchievements::from_event(1, names);
        assert_eq!(
            compare_achievement(&name(0, "ACH_0"), Ok(&long)),
            "shadow: no Reborn answer (more achievements than the proxy compares)"
        );
    }

    #[test]
    fn the_list_is_asked_for_once_per_session() {
        let mut link = ShadowLink::default();
        let reborn = link
            .achievements(
                4242,
                DARK_DECEPTION,
                || {
                    Ok(scripted(&[
                        SESSION,
                        r#"{"id":2,"response":{"type":"accepted"}}"#,
                        STATS_THREE,
                    ]))
                },
                WAIT,
            )
            .cloned();
        assert_eq!(reborn, Ok(three()));
        let never = || -> io::Result<_> { panic!("connected again") };
        assert_eq!(
            link.achievements(4242, DARK_DECEPTION, never, WAIT)
                .cloned(),
            Ok(three())
        );
        let State::Ready(client, _) = &link.state else {
            panic!("no session");
        };
        assert_eq!(
            sent(client),
            "{\"id\":1,\"request\":{\"type\":\"shadow_hello\",\"version\":{\"major\":0,\"minor\":3},\"pid\":4242,\"app_id\":332950}}\n\
             {\"id\":2,\"request\":{\"type\":\"request_stats\"}}\n",
            "one RequestStats, no name sent"
        );
    }

    #[test]
    fn the_list_from_a_stats_callback_comparison_is_used_again() {
        let mut link = ShadowLink::default();
        let connect = || {
            Ok(scripted(&[
                SESSION,
                r#"{"id":2,"response":{"type":"accepted"}}"#,
                STATS_THREE,
            ]))
        };
        assert_eq!(link.stats_result(1, DARK_DECEPTION, connect, WAIT), Ok(1));
        let never = || -> io::Result<_> { panic!("connected again") };
        assert_eq!(
            link.achievements(1, DARK_DECEPTION, never, WAIT).cloned(),
            Ok(three())
        );
    }

    #[test]
    fn a_refused_list_is_asked_for_again() {
        let mut link = ShadowLink::default();
        let connect = || {
            Ok(scripted(&[
                SESSION,
                r#"{"id":2,"response":{"type":"unavailable","reason":"not_signed_in"}}"#,
                r#"{"id":3,"response":{"type":"accepted"}}"#,
                STATS_THREE,
            ]))
        };
        assert_eq!(
            link.achievements(1, DARK_DECEPTION, connect, WAIT).cloned(),
            Err("not_signed_in".to_owned())
        );
        let never = || -> io::Result<_> { panic!("connected again") };
        assert_eq!(
            link.achievements(1, DARK_DECEPTION, never, WAIT).cloned(),
            Ok(three())
        );
    }

    #[test]
    fn the_achievement_queue_wakes_once_and_is_bounded() {
        let mut queue = AchievementQueue::new();
        assert_eq!(
            queue.push("a".to_owned(), OfficialAchievement::Count(3)),
            Pushed::Wake
        );
        assert_eq!(queue.push("b".to_owned(), name(0, "ACH_ONE")), Pushed::Kept);
        let taken = queue.take();
        assert_eq!(
            taken,
            vec![
                ("a".to_owned(), OfficialAchievement::Count(3)),
                ("b".to_owned(), name(0, "ACH_ONE")),
            ]
        );
        assert!(queue.take().is_empty());
        // After a take, the next push wakes the worker again.
        assert_eq!(
            queue.push("c".to_owned(), OfficialAchievement::Count(3)),
            Pushed::Wake
        );
        for index in 1..MAX_WAITING_ACHIEVEMENTS {
            assert_eq!(
                queue.push(format!("{index}"), OfficialAchievement::Count(3)),
                Pushed::Kept
            );
        }
        assert_eq!(
            queue.push("full".to_owned(), OfficialAchievement::Count(3)),
            Pushed::Full("full".to_owned())
        );
        assert_eq!(queue.take().len(), MAX_WAITING_ACHIEVEMENTS);
    }

    #[test]
    fn achievement_digests_are_never_shown() {
        let digest = AchievementDigest::of(b"ACH_ONE");
        assert_eq!(format!("{digest:?}"), "AchievementDigest(<redacted>)");
        assert_ne!(digest, AchievementDigest::of(b"ACH_TWO"));
    }
}
