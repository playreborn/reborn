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
//! - no ownership request reaches Valve: the answer comes from the run's
//!   set, so it keeps working while the connection to Valve is lost.
//!
//! `RequestStats` is checked the same way (a session on this connection, a
//! run in shim mode) and then answered by `shim_stats` for the run's own
//! app only: `Accepted` and later the event `StatsReceived`, written on
//! the same connection after the response, or `Unavailable`.
//!
//! Whether an app is owned, and the stats, are account data: they are
//! never logged.

use std::collections::BTreeSet;
use std::sync::Arc;

use reborn_ipc::socket::LocalSocketStream;
use reborn_shim_protocol::{
    DaemonMessage, RefusalReason, Request, Response, ShimMessage, UnavailableReason, answer_hello,
    decode, encode,
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::daemon::Daemon;
use crate::shim_stats::{RunStats, StatsBackend, StatsState, event_when_received, request_stats};

/// Longest line read from a shim; anything longer closes the connection.
/// No message of protocol 0.2 comes near it; ADR 0055 raises it per
/// session from the app's cloud quota once cloud writes exist.
const MAX_LINE: usize = 64 * 1024;

/// Most accepted stats requests one connection may wait on at once; a shim
/// with more is closed. The recorded games ask once per run.
const MAX_WAITING: usize = 16;

/// A game process `rebornd` launched, as the shim channel sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShimRun {
    /// The app the process was launched for.
    pub app_id: u32,
    /// Ownership data fetched from Valve at launch; `None` for a run that
    /// was not started in shim mode.
    pub ownership: Option<Arc<RunOwnership>>,
    /// The run's stats, fetched on the game's first `RequestStats`; `None`
    /// for a run that was not started in shim mode.
    pub stats: Option<Arc<RunStats>>,
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
/// Responses and events go through one writer, so an event is always
/// written after the response that accepted its request.
async fn serve(daemon: Arc<Daemon>, stream: LocalSocketStream) {
    let runs = Arc::clone(&daemon);
    serve_stream(stream, move |pid| runs.shim_run(pid), daemon).await;
}

/// [`serve`] on any byte stream, with the runs and the stats source given
/// (the daemon, or fakes in tests).
async fn serve_stream<S, B>(stream: S, run: impl Fn(u32) -> Option<ShimRun>, backend: Arc<B>)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
    B: StatsBackend,
{
    let (read, mut write) = tokio::io::split(stream);
    let (outgoing, mut to_write) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        while let Some(line) = to_write.recv().await {
            if write.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let mut waiting = Vec::new();
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
        let Some(reply) = answer(&line, &mut connection, &run, &backend).await else {
            debug!("closing shim connection after a bad line");
            break;
        };
        if reply.stats.is_some() {
            waiting.retain(|task: &tokio::task::JoinHandle<()>| !task.is_finished());
            if waiting.len() >= MAX_WAITING {
                debug!("closing shim connection with too many open stats requests");
                break;
            }
        }
        if outgoing.send(reply.line).is_err() {
            break;
        }
        if let Some(stats) = reply.stats {
            let outgoing = outgoing.clone();
            waiting.push(tokio::spawn(async move {
                if let Some(line) = event_line(stats).await {
                    let _ = outgoing.send(line);
                }
            }));
        }
    }
    // The shim is gone: nobody waits for its events any more. A fetch that
    // runs keeps going only while the game's run exists (`shim_stats`).
    for task in waiting {
        task.abort();
    }
    drop(outgoing);
    let _ = writer.await;
}

/// The event line for an accepted `RequestStats`, once the run's stats are
/// received; `None` when the request failed (no event, `shim_stats`).
async fn event_line(stats: watch::Receiver<StatsState>) -> Option<String> {
    let event = event_when_received(stats).await?;
    encode(&DaemonMessage::Event { event }).ok()
}

/// What the daemon writes back for one line.
struct Reply {
    /// The response line.
    line: String,
    /// For an accepted `RequestStats`: where its event shows up.
    stats: Option<watch::Receiver<StatsState>>,
}

/// The reply to one line from a shim, `None` if the line is not a valid
/// message. `run` gives the run the daemon launched for a process ID,
/// while it runs; `backend` is where stats come from.
async fn answer<B: StatsBackend>(
    line: &str,
    connection: &mut Connection,
    run: impl Fn(u32) -> Option<ShimRun>,
    backend: &Arc<B>,
) -> Option<Reply> {
    let message: ShimMessage = decode(line).ok()?;
    let mut stats = None;
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
        Request::RequestStats => {
            let (response, receiver) =
                answer_stats(connection.pid.and_then(&run).as_ref(), backend).await;
            match &response {
                Response::Unavailable { reason } => {
                    info!(pid = ?connection.pid, ?reason, "shim stats request refused");
                }
                _ => debug!(pid = ?connection.pid, "shim stats request accepted"),
            }
            stats = receiver;
            response
        }
    };
    let line = encode(&DaemonMessage::Response {
        id: message.id,
        response,
    })
    .ok()?;
    Some(Reply { line, stats })
}

/// The answer to `RequestStats` for the connection's run (ADR 0055): the
/// session check first, then shim mode, then the run's own app only.
async fn answer_stats<B: StatsBackend>(
    run: Option<&ShimRun>,
    backend: &Arc<B>,
) -> (Response, Option<watch::Receiver<StatsState>>) {
    let not_allowed = Response::Unavailable {
        reason: UnavailableReason::NotAllowed,
    };
    let Some(run) = run else {
        return (not_allowed, None);
    };
    let Some(stats) = run.stats.as_ref() else {
        return (not_allowed, None);
    };
    request_stats(backend, run.app_id, stats).await
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
    use crate::shim_stats::tests::{BONGO_CAT, Fake, recorded};
    use crate::shim_stats::{FetchError, Link};
    use reborn_shim_protocol::{
        Event, PROTOCOL_VERSION, RefusalReason, Response, Session, Version, decode,
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

    fn request_stats() -> String {
        line(5, Request::RequestStats)
    }

    /// The runs of the tests: process 100 runs Cave Crawlers in shim mode,
    /// 101 without shim mode, 200 Bongo Cat (the recorded stats) in shim
    /// mode. The stats of each run are kept, like the daemon keeps them.
    struct Runs {
        game: ShimRun,
        bongo: ShimRun,
    }

    impl Runs {
        fn new() -> Self {
            Self {
                game: ShimRun {
                    app_id: GAME,
                    ownership: Some(Arc::new(RunOwnership {
                        owned: BTreeSet::from([GAME, OTHER_DLC, OTHER_OWNED]),
                        dlc: BTreeSet::from([DLC, OTHER_DLC]),
                    })),
                    stats: Some(Arc::new(RunStats::default())),
                },
                bongo: ShimRun {
                    app_id: BONGO_CAT,
                    ownership: Some(Arc::new(RunOwnership::default())),
                    stats: Some(Arc::new(RunStats::default())),
                },
            }
        }

        fn launched(&self, pid: u32) -> Option<ShimRun> {
            match pid {
                100 => Some(self.game.clone()),
                101 => Some(ShimRun {
                    app_id: GAME,
                    ownership: None,
                    stats: None,
                }),
                200 => Some(self.bongo.clone()),
                _ => None,
            }
        }
    }

    /// No Valve session in the ownership tests: they never need one.
    fn offline() -> Arc<Fake> {
        Fake::new(Link::SignedOut, Vec::new())
    }

    async fn reply_with(
        line: &str,
        connection: &mut Connection,
        runs: &Runs,
        backend: &Arc<Fake>,
    ) -> Option<(DaemonMessage, Option<watch::Receiver<StatsState>>)> {
        let reply = answer(line, connection, |pid| runs.launched(pid), backend).await?;
        Some((decode::<DaemonMessage>(&reply.line).ok()?, reply.stats))
    }

    async fn reply(line: &str, connection: &mut Connection) -> Option<DaemonMessage> {
        let runs = Runs::new();
        reply_with(line, connection, &runs, &offline())
            .await
            .map(|(message, _)| message)
    }

    fn response(id: u64, response: Response) -> Option<DaemonMessage> {
        Some(DaemonMessage::Response { id, response })
    }

    fn unavailable(id: u64, reason: UnavailableReason) -> Option<DaemonMessage> {
        response(id, Response::Unavailable { reason })
    }

    fn not_allowed() -> Option<DaemonMessage> {
        unavailable(4, UnavailableReason::NotAllowed)
    }

    #[tokio::test]
    async fn a_launched_process_gets_its_session() {
        let mut connection = Connection::default();
        assert_eq!(
            reply(&hello(100, PROTOCOL_VERSION), &mut connection).await,
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

    #[tokio::test]
    async fn other_processes_and_versions_are_refused() {
        let mut connection = Connection::default();
        let runs = Runs::new();
        let text = answer(
            &hello(102, PROTOCOL_VERSION),
            &mut connection,
            |pid| runs.launched(pid),
            &offline(),
        )
        .await
        .map(|reply| reply.line)
        .unwrap_or_default();
        assert!(text.contains("unknown_process"), "{text}");
        assert_eq!(
            reply(&hello(100, Version { major: 9, minor: 0 }), &mut connection).await,
            response(
                3,
                Response::Refused {
                    reason: RefusalReason::IncompatibleVersion
                }
            )
        );
        assert_eq!(connection.pid, None);
    }

    #[tokio::test]
    async fn the_game_and_its_dlc_get_ownership_answers() {
        let mut connection = Connection::default();
        reply(&hello(100, PROTOCOL_VERSION), &mut connection).await;
        assert_eq!(
            reply(&ownership(GAME), &mut connection).await,
            response(4, Response::Ownership { owned: true })
        );
        assert_eq!(
            reply(&ownership(OTHER_DLC), &mut connection).await,
            response(4, Response::Ownership { owned: true })
        );
        assert_eq!(
            reply(&ownership(DLC), &mut connection).await,
            response(4, Response::Ownership { owned: false })
        );
    }

    #[tokio::test]
    async fn other_apps_are_refused_even_when_owned() {
        let mut connection = Connection::default();
        reply(&hello(100, PROTOCOL_VERSION), &mut connection).await;
        assert_eq!(
            reply(&ownership(OTHER_OWNED), &mut connection).await,
            not_allowed()
        );
        assert_eq!(reply(&ownership(730), &mut connection).await, not_allowed());
    }

    #[tokio::test]
    async fn ownership_needs_a_session_on_this_connection() {
        let mut connection = Connection::default();
        assert_eq!(
            reply(&ownership(GAME), &mut connection).await,
            not_allowed()
        );
        // A refused hello binds nothing.
        reply(&hello(102, PROTOCOL_VERSION), &mut connection).await;
        assert_eq!(
            reply(&ownership(GAME), &mut connection).await,
            not_allowed()
        );
    }

    #[tokio::test]
    async fn a_run_without_shim_mode_or_that_ended_gets_no_ownership() {
        let mut connection = Connection::default();
        reply(&hello(101, PROTOCOL_VERSION), &mut connection).await;
        assert_eq!(connection.pid, Some(101));
        assert_eq!(
            reply(&ownership(GAME), &mut connection).await,
            not_allowed()
        );

        // The game exited: the daemon no longer knows the process.
        let mut connection = Connection { pid: Some(100) };
        let ended = |_pid: u32| -> Option<ShimRun> { None };
        let text = answer(&ownership(GAME), &mut connection, ended, &offline())
            .await
            .map(|reply| reply.line)
            .unwrap_or_default();
        assert_eq!(decode::<DaemonMessage>(&text).ok(), not_allowed());
    }

    #[tokio::test]
    async fn a_connection_stays_bound_to_its_first_process() {
        let mut connection = Connection::default();
        reply(&hello(100, PROTOCOL_VERSION), &mut connection).await;
        assert_eq!(connection.pid, Some(100));

        // Another launched process on the same connection: refused.
        assert_eq!(
            reply(&hello(101, PROTOCOL_VERSION), &mut connection).await,
            response(
                3,
                Response::Refused {
                    reason: RefusalReason::UnknownProcess
                }
            )
        );
        assert_eq!(connection.pid, Some(100));

        // A refused hello for the bound process keeps the binding.
        reply(&hello(100, Version { major: 9, minor: 0 }), &mut connection).await;
        assert_eq!(connection.pid, Some(100));
        assert_eq!(
            reply(&ownership(GAME), &mut connection).await,
            response(4, Response::Ownership { owned: true })
        );

        // The same process may say hello again and gets its session.
        assert!(matches!(
            reply(&hello(100, PROTOCOL_VERSION), &mut connection).await,
            Some(DaemonMessage::Response {
                response: Response::Session(_),
                ..
            })
        ));
        assert_eq!(connection.pid, Some(100));
    }

    #[tokio::test]
    async fn bad_lines_get_no_reply() {
        let mut connection = Connection::default();
        let runs = Runs::new();
        let launched = |pid| runs.launched(pid);
        assert!(
            answer("not json\n", &mut connection, launched, &offline())
                .await
                .is_none()
        );
        assert!(
            answer("{\"id\":1}\n", &mut connection, launched, &offline())
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn stats_are_fetched_for_the_own_app_and_sent_as_an_event() {
        let runs = Runs::new();
        let valve = Fake::new(Link::Connected, vec![Ok(recorded())]);
        let mut connection = Connection::default();
        reply_with(
            &hello(200, PROTOCOL_VERSION),
            &mut connection,
            &runs,
            &valve,
        )
        .await;
        let Some((message, Some(stats))) =
            reply_with(&request_stats(), &mut connection, &runs, &valve).await
        else {
            panic!("not accepted");
        };
        assert_eq!(
            message,
            response(5, Response::Accepted).unwrap_or_else(|| unreachable!())
        );
        let line = event_line(stats).await.unwrap_or_default();
        let Ok(DaemonMessage::Event {
            event:
                Event::StatsReceived {
                    result,
                    achievement_names,
                },
        }) = decode::<DaemonMessage>(&line)
        else {
            panic!("not a stats event: {line}");
        };
        assert_eq!(result, 1);
        assert_eq!(achievement_names.len(), 28);
        assert_eq!(achievement_names[0].name, "PET_1");
        assert_eq!(achievement_names[0].display_name, "Bongo Beat 1");
        // Only the run's own app was asked about, once.
        assert_eq!(
            *valve.asked.lock().unwrap_or_else(|p| p.into_inner()),
            [BONGO_CAT]
        );

        // A second request in the same run: from the run's cache.
        let Some((message, Some(stats))) =
            reply_with(&request_stats(), &mut connection, &runs, &valve).await
        else {
            panic!("not accepted");
        };
        assert_eq!(
            message,
            response(5, Response::Accepted).unwrap_or_else(|| unreachable!())
        );
        assert_eq!(event_line(stats).await, Some(line));
        assert_eq!(valve.fetches(), 1);
    }

    #[tokio::test]
    async fn stats_need_a_session_and_shim_mode() {
        let runs = Runs::new();
        let valve = Fake::new(Link::Connected, vec![Ok(recorded())]);
        // No hello on this connection.
        let mut connection = Connection::default();
        let (message, stats) = reply_with(&request_stats(), &mut connection, &runs, &valve)
            .await
            .unwrap_or_else(|| panic!("no reply"));
        assert_eq!(Some(message), unavailable(5, UnavailableReason::NotAllowed));
        assert!(stats.is_none());
        // A refused hello binds nothing.
        reply_with(
            &hello(102, PROTOCOL_VERSION),
            &mut connection,
            &runs,
            &valve,
        )
        .await;
        let (message, _) = reply_with(&request_stats(), &mut connection, &runs, &valve)
            .await
            .unwrap_or_else(|| panic!("no reply"));
        assert_eq!(Some(message), unavailable(5, UnavailableReason::NotAllowed));
        // A run without shim mode.
        let mut connection = Connection::default();
        reply_with(
            &hello(101, PROTOCOL_VERSION),
            &mut connection,
            &runs,
            &valve,
        )
        .await;
        let (message, _) = reply_with(&request_stats(), &mut connection, &runs, &valve)
            .await
            .unwrap_or_else(|| panic!("no reply"));
        assert_eq!(Some(message), unavailable(5, UnavailableReason::NotAllowed));
        // The game exited.
        let mut connection = Connection { pid: Some(200) };
        let ended = |_pid: u32| -> Option<ShimRun> { None };
        let reply = answer(&request_stats(), &mut connection, ended, &valve)
            .await
            .unwrap_or_else(|| panic!("no reply"));
        assert_eq!(
            decode::<DaemonMessage>(&reply.line).ok(),
            unavailable(5, UnavailableReason::NotAllowed)
        );
        assert_eq!(valve.fetches(), 0);
    }

    #[tokio::test]
    async fn stats_need_a_signed_in_daemon_but_wait_through_offline() {
        // Signed out: unavailable, nothing fetched.
        let runs = Runs::new();
        let valve = Fake::new(Link::SignedOut, vec![Ok(recorded())]);
        let mut connection = Connection::default();
        reply_with(
            &hello(200, PROTOCOL_VERSION),
            &mut connection,
            &runs,
            &valve,
        )
        .await;
        let (message, stats) = reply_with(&request_stats(), &mut connection, &runs, &valve)
            .await
            .unwrap_or_else(|| panic!("no reply"));
        assert_eq!(
            Some(message),
            unavailable(5, UnavailableReason::NotSignedIn)
        );
        assert!(stats.is_none());
        assert_eq!(valve.fetches(), 0);

        // Offline (reconnecting): accepted, fetched once a session is back
        // (ADR 0055: the request waits while rebornd reconnects).
        let runs = Runs::new();
        let valve = Fake::new(
            Link::Offline,
            vec![Err(FetchError::NoSession), Ok(recorded())],
        );
        let mut connection = Connection::default();
        reply_with(
            &hello(200, PROTOCOL_VERSION),
            &mut connection,
            &runs,
            &valve,
        )
        .await;
        let Some((message, Some(stats))) =
            reply_with(&request_stats(), &mut connection, &runs, &valve).await
        else {
            panic!("not accepted");
        };
        assert_eq!(
            message,
            DaemonMessage::Response {
                id: 5,
                response: Response::Accepted
            }
        );
        assert!(event_line(stats).await.is_some());
        assert_eq!(valve.fetches(), 1);
    }

    #[tokio::test]
    async fn a_failed_fetch_sends_no_event() {
        let runs = Runs::new();
        let valve = Fake::new(Link::Connected, vec![Err(FetchError::Unreadable)]);
        let mut connection = Connection::default();
        reply_with(
            &hello(200, PROTOCOL_VERSION),
            &mut connection,
            &runs,
            &valve,
        )
        .await;
        let Some((_, Some(stats))) =
            reply_with(&request_stats(), &mut connection, &runs, &valve).await
        else {
            panic!("not accepted");
        };
        assert_eq!(event_line(stats).await, None);
    }

    /// One end of an in-memory connection served by [`serve_stream`] for
    /// the test runs, and the task serving the other end.
    fn connect(
        valve: &Arc<Fake>,
    ) -> (
        BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        tokio::io::WriteHalf<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<()>,
    ) {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let runs = Runs::new();
        let served = tokio::spawn(serve_stream(
            server,
            move |pid| runs.launched(pid),
            Arc::clone(valve),
        ));
        let (read, write) = tokio::io::split(client);
        (BufReader::new(read), write, served)
    }

    async fn next(lines: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>) -> String {
        let mut line = String::new();
        let read = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            lines.read_line(&mut line),
        );
        assert!(matches!(read.await, Ok(Ok(_))), "no line");
        line
    }

    #[tokio::test]
    async fn served_connection_sends_the_event_after_accepted() {
        let valve = Fake::new(Link::Connected, vec![Ok(recorded())]);
        valve.close();
        let (mut lines, mut write, served) = connect(&valve);
        write
            .write_all(hello(200, PROTOCOL_VERSION).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(next(&mut lines).await.contains("\"session\""));
        write
            .write_all(request_stats().as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            decode::<DaemonMessage>(&next(&mut lines).await).ok(),
            response(5, Response::Accepted)
        );
        // Ownership on the same connection is answered while the fetch
        // waits for Valve.
        write
            .write_all(ownership(BONGO_CAT).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            decode::<DaemonMessage>(&next(&mut lines).await).ok(),
            response(4, Response::Ownership { owned: false })
        );
        valve.open(1);
        let event = next(&mut lines).await;
        assert!(
            matches!(
                decode::<DaemonMessage>(&event),
                Ok(DaemonMessage::Event {
                    event: Event::StatsReceived { result: 1, .. }
                })
            ),
            "{event}"
        );
        drop((lines, write));
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), served)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn a_disconnect_ends_the_waiting_for_events() {
        // The fetch never finishes; the connection still closes cleanly,
        // which needs every waiting task gone (they hold the writer).
        let valve = Fake::new(Link::Connected, vec![Ok(recorded())]);
        valve.close();
        let (mut lines, mut write, served) = connect(&valve);
        write
            .write_all(hello(200, PROTOCOL_VERSION).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        next(&mut lines).await;
        write
            .write_all(request_stats().as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        next(&mut lines).await;
        drop((lines, write));
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), served)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn too_many_open_stats_requests_close_the_connection() {
        let valve = Fake::new(Link::Connected, vec![Ok(recorded())]);
        valve.close();
        let (mut lines, mut write, served) = connect(&valve);
        write
            .write_all(hello(200, PROTOCOL_VERSION).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        next(&mut lines).await;
        for _ in 0..=MAX_WAITING {
            write
                .write_all(request_stats().as_bytes())
                .await
                .unwrap_or_else(|error| panic!("{error}"));
        }
        for _ in 0..MAX_WAITING {
            assert!(next(&mut lines).await.contains("accepted"));
        }
        let mut rest = String::new();
        let end = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            lines.read_line(&mut rest),
        );
        assert!(matches!(end.await, Ok(Ok(0))), "{rest}");
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), served)
                .await
                .is_ok()
        );
        assert_eq!(valve.fetches(), 0);
    }

    #[tokio::test]
    async fn runs_do_not_share_stats() {
        // Each run asks Valve once, for its own app (the fake answers
        // both with the same recording; only the asked app IDs matter).
        let runs = Runs::new();
        let valve = Fake::new(Link::Connected, vec![Ok(recorded())]);
        let mut bongo = Connection::default();
        reply_with(&hello(200, PROTOCOL_VERSION), &mut bongo, &runs, &valve).await;
        let Some((_, Some(stats))) = reply_with(&request_stats(), &mut bongo, &runs, &valve).await
        else {
            panic!("not accepted");
        };
        assert!(event_line(stats).await.is_some());
        let mut game = Connection::default();
        reply_with(&hello(100, PROTOCOL_VERSION), &mut game, &runs, &valve).await;
        let Some((_, Some(stats))) = reply_with(&request_stats(), &mut game, &runs, &valve).await
        else {
            panic!("not accepted");
        };
        event_line(stats).await;
        assert_eq!(
            *valve.asked.lock().unwrap_or_else(|p| p.into_inner()),
            [BONGO_CAT, GAME]
        );
    }
}
