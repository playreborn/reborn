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
//! `CloudFileSize { name }` and `CloudRead { name, max }` (ADR 0055) are
//! checked the same way and answered only from the run's own cloud folder
//! (`reborn_core::run_cloud`, `<Reborn data>/cloud/<account ID>/<app
//! ID>/`), never from Valve: `FileSize` or `FileData` (`None` for a
//! missing file; at most `max` bytes), `TooLarge` for a file above the
//! app's quota (at most 16 MiB), and `NotAllowed` for an unsafe name (a
//! path that could leave the folder, an absolute path, a root token), a
//! symlink, a folder, a read error, a run without shim mode, an app
//! without a quota, and shadow sessions (their separate shadow folder
//! comes with stage 1). Nothing is written. File names and contents are
//! never logged.
//!
//! Shadow sessions (ADR 0055, amending ADR 0047; `shim_shadow`): a
//! connection without a session may send `RegisterShadow { app_id }`
//! (`reborn-observe arm --app`) or `CancelShadow` (`disarm`); a
//! connection bound to a process gets `NotAllowed` for both. The
//! observation proxy then says `ShadowHello { pid, app_id }` and its
//! connection is bound to that process as a shadow session, for good:
//! it never becomes a launched run's session, and a launched process never
//! gets a shadow session. A shadow session is answered like a shim-mode run
//! for reads (`AppOwnership`, `RequestStats`), and every write is refused
//! with `NotAllowed` before anything else. It ends when the connection
//! closes, or for all at once when the Valve session ends.
//!
//! Whether an app is owned, and the stats, are account data: they are
//! never logged.

use std::collections::BTreeSet;
use std::sync::Arc;

use reborn_core::run_cloud::{RunCloud, RunCloudError};
use reborn_ipc::socket::LocalSocketStream;
use reborn_shim_protocol::{
    Bytes, DaemonMessage, PROTOCOL_VERSION, RefusalReason, Request, Response, ShimMessage,
    UnavailableReason, answer_hello, decode, encode,
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

use crate::daemon::Daemon;
use crate::shim_shadow::ShadowSessions;
use crate::shim_stats::{RunStats, StatsBackend, StatsState, event_when_received, request_stats};

/// Longest line read from a shim; anything longer closes the connection.
/// No request of protocol 0.2 comes near it (cloud reads are short
/// requests; only their answers are long); ADR 0055 raises it per session
/// from the app's cloud quota once cloud writes exist.
const MAX_LINE: usize = 64 * 1024;

/// Most accepted stats requests one connection may wait on at once; a shim
/// with more is closed. The recorded games ask once per run.
const MAX_WAITING: usize = 16;

/// A game process `rebornd` launched, or a shadow session's process
/// (`shadow`), as the shim channel sees it.
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
    /// The run's cloud folder (ADR 0055); `None` for a run not started in
    /// shim mode, an app without a cloud quota, and shadow sessions.
    pub cloud: Option<Arc<RunCloud>>,
    /// Whether this is a read-only shadow session for a process
    /// `reborn-observe arm` registered (ADR 0055), not a process `rebornd`
    /// launched.
    pub shadow: bool,
}

/// Where the shim channel finds runs and shadow sessions: the daemon, or
/// fakes in tests.
pub trait ShimHost: Send + Sync + 'static {
    /// The run the daemon launched the process `pid` for, while it runs.
    fn launched(&self, pid: u32) -> Option<ShimRun>;
    /// The shadow registration and sessions.
    fn shadows(&self) -> &ShadowSessions;
    /// Registers `app_id` for a shadow session (`RegisterShadow`): the
    /// response, after the registration is in [`Self::shadows`] or was
    /// refused.
    fn register_shadow(&self, app_id: u32) -> impl Future<Output = Response> + Send;
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
    /// For a shadow session: the token of its claim, so only this
    /// session's data is reached and only this session is ended.
    shadow: Option<u64>,
}

impl Connection {
    /// The connection's run, while it lasts: a shadow session only from
    /// the shadow sessions, a launched process only from the launches.
    fn run<H: ShimHost>(&self, host: &H) -> Option<ShimRun> {
        let pid = self.pid?;
        match self.shadow {
            Some(token) => host.shadows().get(pid, token),
            None => host.launched(pid),
        }
    }
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
    serve_stream(stream, Arc::clone(&daemon), daemon).await;
}

/// [`serve`] on any byte stream, with the runs and the stats source given
/// (the daemon, or fakes in tests).
async fn serve_stream<S, H, B>(stream: S, host: Arc<H>, backend: Arc<B>)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
    H: ShimHost,
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
        let Some(reply) = answer(&line, &mut connection, &*host, &backend).await else {
            debug!("closing shim connection after a bad line");
            break;
        };
        let Reply { line: reply, stats } = reply;
        match stats.map(|stats| (decided_event_line(&stats), stats)) {
            None => {
                if outgoing.send(reply).is_err() {
                    break;
                }
            }
            // Already decided (the run's cache, or failed): the event, if
            // any, follows the response at once; nothing waits.
            Some((Some(event), _)) => {
                if outgoing.send(reply).is_err() {
                    break;
                }
                if let Some(event) = event
                    && outgoing.send(event).is_err()
                {
                    break;
                }
            }
            // Still open: a task waits for the event. Only these count.
            Some((None, stats)) => {
                waiting.retain(|task: &tokio::task::JoinHandle<()>| !task.is_finished());
                if waiting.len() >= MAX_WAITING {
                    debug!("closing shim connection with too many open stats requests");
                    break;
                }
                if outgoing.send(reply).is_err() {
                    break;
                }
                let outgoing = outgoing.clone();
                waiting.push(tokio::spawn(async move {
                    if let Some(line) = event_line(stats).await {
                        let _ = outgoing.send(line);
                    }
                }));
            }
        }
    }
    // The shim is gone: nobody waits for its events any more. A fetch that
    // runs keeps going only while the game's run exists (`shim_stats`).
    for task in waiting {
        task.abort();
    }
    // A shadow session lasts as long as the proxy's connection.
    if let (Some(token), Some(pid)) = (connection.shadow, connection.pid) {
        host.shadows().end(pid, token);
        info!(pid, "shim shadow session ended");
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

/// For a run whose stats are already decided: `Some` with the event line
/// (`None` inside for a failed run, which sends no event). `None` while
/// the answer is still open.
fn decided_event_line(stats: &watch::Receiver<StatsState>) -> Option<Option<String>> {
    match &*stats.borrow() {
        StatsState::Idle | StatsState::Fetching => None,
        StatsState::Received(event) => Some(
            encode(&DaemonMessage::Event {
                event: event.clone(),
            })
            .ok(),
        ),
        StatsState::Failed(_) => Some(None),
    }
}

/// What the daemon writes back for one line.
struct Reply {
    /// The response line.
    line: String,
    /// For an accepted `RequestStats`: where its event shows up.
    stats: Option<watch::Receiver<StatsState>>,
}

/// The reply to one line from a shim, `None` if the line is not a valid
/// message. `host` gives the runs the daemon launched and the shadow
/// sessions; `backend` is where stats come from.
async fn answer<H: ShimHost, B: StatsBackend>(
    line: &str,
    connection: &mut Connection,
    host: &H,
    backend: &Arc<B>,
) -> Option<Reply> {
    let message: ShimMessage = decode(line).ok()?;
    let mut stats = None;
    let not_allowed = Response::Unavailable {
        reason: UnavailableReason::NotAllowed,
    };
    // A shadow session never writes (ADR 0055), checked before anything
    // else.
    if connection.shadow.is_some() && message.request.writes() {
        info!(pid = ?connection.pid, "shim write on a shadow session refused");
        return reply_line(message.id, not_allowed, None);
    }
    let response = match message.request {
        Request::Hello { version, pid } => {
            let response = match connection.pid {
                // Bound to another process, or as a shadow session:
                // refused, the binding stays.
                Some(bound) if bound != pid || connection.shadow.is_some() => Response::Refused {
                    reason: RefusalReason::UnknownProcess,
                },
                _ => answer_hello(version, host.launched(pid).map(|run| run.app_id)),
            };
            info!(pid, ?response, "shim said hello");
            // Only the first successful hello binds; a refused one never
            // clears a binding.
            if connection.pid.is_none() && matches!(response, Response::Session(_)) {
                connection.pid = Some(pid);
            }
            response
        }
        Request::ShadowHello {
            version,
            pid,
            app_id,
        } => {
            let response = answer_shadow_hello(connection, host, version, pid, app_id);
            info!(pid, app_id, ?response, "shim shadow hello");
            response
        }
        Request::RegisterShadow { app_id } => {
            let response = if connection.pid.is_some() {
                not_allowed
            } else {
                host.register_shadow(app_id).await
            };
            info!(app_id, ?response, "shadow session registration");
            response
        }
        Request::CancelShadow => {
            if connection.pid.is_some() {
                not_allowed
            } else {
                host.shadows().cancel();
                info!("shadow session registration cancelled");
                Response::Accepted
            }
        }
        Request::AppOwnership { app_id } => {
            let response = answer_ownership(connection.run(host).as_ref(), app_id);
            match &response {
                Response::Unavailable { reason } => {
                    info!(pid = ?connection.pid, app_id, ?reason, "shim ownership request refused");
                }
                _ => debug!(pid = ?connection.pid, app_id, "shim ownership request answered"),
            }
            response
        }
        Request::RequestStats => {
            let (response, receiver) = answer_stats(connection.run(host).as_ref(), backend).await;
            match &response {
                Response::Unavailable { reason } => {
                    info!(pid = ?connection.pid, ?reason, "shim stats request refused");
                }
                _ => debug!(pid = ?connection.pid, "shim stats request accepted"),
            }
            stats = receiver;
            response
        }
        Request::CloudFileSize { name } => {
            let response = answer_cloud(connection.run(host).as_ref(), name, CloudAsk::Size).await;
            log_cloud(connection.pid, "size", &response);
            response
        }
        Request::CloudRead { name, max } => {
            let response =
                answer_cloud(connection.run(host).as_ref(), name, CloudAsk::Read { max }).await;
            log_cloud(connection.pid, "read", &response);
            response
        }
    };
    reply_line(message.id, response, stats)
}

fn reply_line(
    id: u64,
    response: Response,
    stats: Option<watch::Receiver<StatsState>>,
) -> Option<Reply> {
    let line = encode(&DaemonMessage::Response { id, response }).ok()?;
    Some(Reply { line, stats })
}

/// The answer to `ShadowHello` (ADR 0055): the version first, then the
/// binding, then the registration. Only a connection without a session
/// claims one; the same shadow process may say it again on its own
/// connection and keeps its session.
fn answer_shadow_hello<H: ShimHost>(
    connection: &mut Connection,
    host: &H,
    version: reborn_shim_protocol::Version,
    pid: u32,
    app_id: u32,
) -> Response {
    let unknown = Response::Refused {
        reason: RefusalReason::UnknownProcess,
    };
    if version.major != PROTOCOL_VERSION.major {
        return answer_hello(version, None);
    }
    match connection.pid {
        Some(bound) if bound == pid && connection.shadow.is_some() => {
            let run = connection.run(host).filter(|run| run.app_id == app_id);
            answer_hello(version, run.map(|run| run.app_id))
        }
        Some(_) => unknown,
        // A process `rebornd` launched is never a shadow.
        None if host.launched(pid).is_some() => unknown,
        None => match host.shadows().claim(pid, app_id) {
            Some((token, run)) => {
                connection.pid = Some(pid);
                connection.shadow = Some(token);
                answer_hello(version, Some(run.app_id))
            }
            None => unknown,
        },
    }
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

/// A cloud read of the shim.
#[derive(Clone, Copy, Debug)]
enum CloudAsk {
    /// `CloudFileSize`.
    Size,
    /// `CloudRead` with the game's buffer size.
    Read { max: u32 },
}

/// The answer to `CloudFileSize` or `CloudRead` for the connection's run
/// (ADR 0055): the session check first, then the run's cloud folder (shim
/// mode, an app with a quota, not a shadow session), then the file, read
/// off the async threads. Never asks Valve.
async fn answer_cloud(run: Option<&ShimRun>, name: String, ask: CloudAsk) -> Response {
    let not_allowed = Response::Unavailable {
        reason: UnavailableReason::NotAllowed,
    };
    let Some(cloud) = run.and_then(|run| run.cloud.clone()) else {
        return not_allowed;
    };
    match tokio::task::spawn_blocking(move || cloud_answer(&cloud, &name, ask)).await {
        Ok(response) => response,
        Err(error) => {
            warn!(%error, "shim cloud read failed");
            not_allowed
        }
    }
}

/// The answer from the run's cloud folder. Every refusal but `TooLarge`
/// is `NotAllowed`: an unsafe name, a symlink or a folder is not the
/// game's business, and for a read error ADR 0055 names no reason; the
/// shim stops the game on any of them.
fn cloud_answer(cloud: &RunCloud, name: &str, ask: CloudAsk) -> Response {
    let result = match ask {
        CloudAsk::Size => cloud.file_size(name).and_then(|size| {
            let size = size
                .map(|size| u32::try_from(size).map_err(|_| RunCloudError::TooLarge))
                .transpose()?;
            Ok(Response::FileSize { size })
        }),
        CloudAsk::Read { max } => cloud
            .read(name, u64::from(max))
            .map(|data| Response::FileData {
                data: data.map(Bytes),
            }),
    };
    match result {
        Ok(response) => response,
        Err(RunCloudError::TooLarge) => Response::Unavailable {
            reason: UnavailableReason::TooLarge,
        },
        Err(error) => {
            debug!(%error, "shim cloud read refused");
            Response::Unavailable {
                reason: UnavailableReason::NotAllowed,
            }
        }
    }
}

/// Logs a cloud answer with sizes only: names and contents are the user's
/// saves.
fn log_cloud(pid: Option<u32>, what: &str, response: &Response) {
    match response {
        Response::Unavailable { reason } => {
            info!(?pid, what, ?reason, "shim cloud request refused");
        }
        Response::FileSize { size } => debug!(?pid, ?size, "shim cloud size answered"),
        Response::FileData { data } => debug!(
            ?pid,
            bytes = data.as_ref().map(|data| data.0.len()),
            "shim cloud read answered"
        ),
        _ => {}
    }
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
    /// Dark Deception, a recorded `BIsSubscribed` game
    /// (`docs/shim/meanings.md`, experiment 8), as the shadow game; used
    /// here only as a number the fake account owns.
    const SHADOW_GAME: u32 = 332_950;
    /// The shadow game's process (not launched by the daemon).
    const SHADOW_PID: u32 = 300;

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
    /// `ended` makes every game exited. A shadow registration succeeds
    /// only with `signed_in`, with the fake account's owned apps.
    struct Runs {
        game: ShimRun,
        bongo: ShimRun,
        ended: bool,
        signed_in: bool,
        shadows: ShadowSessions,
    }

    /// The fake account's owned apps, for shadow registrations.
    fn account_owns() -> BTreeSet<u32> {
        BTreeSet::from([GAME, OTHER_DLC, OTHER_OWNED, SHADOW_GAME])
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
                    cloud: None,
                    shadow: false,
                },
                bongo: ShimRun {
                    app_id: BONGO_CAT,
                    ownership: Some(Arc::new(RunOwnership::default())),
                    stats: Some(Arc::new(RunStats::default())),
                    cloud: None,
                    shadow: false,
                },
                ended: false,
                signed_in: true,
                shadows: ShadowSessions::default(),
            }
        }

        fn ended() -> Self {
            Self {
                ended: true,
                ..Self::new()
            }
        }

        fn signed_out() -> Self {
            Self {
                signed_in: false,
                ..Self::new()
            }
        }
    }

    impl ShimHost for Runs {
        fn launched(&self, pid: u32) -> Option<ShimRun> {
            if self.ended {
                return None;
            }
            match pid {
                100 => Some(self.game.clone()),
                101 => Some(ShimRun {
                    app_id: GAME,
                    ownership: None,
                    stats: None,
                    cloud: None,
                    shadow: false,
                }),
                200 => Some(self.bongo.clone()),
                _ => None,
            }
        }

        fn shadows(&self) -> &ShadowSessions {
            &self.shadows
        }

        async fn register_shadow(&self, app_id: u32) -> Response {
            if !self.signed_in {
                return Response::Unavailable {
                    reason: UnavailableReason::NotSignedIn,
                };
            }
            self.shadows.register(
                app_id,
                RunOwnership {
                    owned: account_owns(),
                    dlc: BTreeSet::new(),
                },
            );
            Response::Accepted
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
        let reply = answer(line, connection, runs, backend).await?;
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
            &runs,
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
        let mut connection = Connection {
            pid: Some(100),
            shadow: None,
        };
        let text = answer(
            &ownership(GAME),
            &mut connection,
            &Runs::ended(),
            &offline(),
        )
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
        assert!(
            answer("not json\n", &mut connection, &runs, &offline())
                .await
                .is_none()
        );
        assert!(
            answer("{\"id\":1}\n", &mut connection, &runs, &offline())
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
        let mut connection = Connection {
            pid: Some(200),
            shadow: None,
        };
        let reply = answer(&request_stats(), &mut connection, &Runs::ended(), &valve)
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
        connect_to(valve, &Arc::new(Runs::new()))
    }

    /// [`connect`] with the runs given, so several connections share them.
    fn connect_to(
        valve: &Arc<Fake>,
        runs: &Arc<Runs>,
    ) -> (
        BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
        tokio::io::WriteHalf<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<()>,
    ) {
        let (client, server) = tokio::io::duplex(64 * 1024);
        let served = tokio::spawn(serve_stream(server, Arc::clone(runs), Arc::clone(valve)));
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
    async fn cached_stats_answers_need_no_waiting() {
        // Once the run's stats are in, every request gets `Accepted` and
        // the event right behind it, without counting against the cap.
        let valve = Fake::new(Link::Connected, vec![Ok(recorded())]);
        let (mut lines, mut write, served) = connect(&valve);
        write
            .write_all(hello(200, PROTOCOL_VERSION).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        next(&mut lines).await;
        for _ in 0..(MAX_WAITING * 2) {
            write
                .write_all(request_stats().as_bytes())
                .await
                .unwrap_or_else(|error| panic!("{error}"));
            assert!(next(&mut lines).await.contains("accepted"));
            assert!(next(&mut lines).await.contains("stats_received"));
        }
        assert_eq!(valve.fetches(), 1);
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

    fn register(app_id: u32) -> String {
        line(6, Request::RegisterShadow { app_id })
    }

    fn cancel() -> String {
        line(6, Request::CancelShadow)
    }

    fn shadow_hello(pid: u32, app_id: u32) -> String {
        line(
            3,
            Request::ShadowHello {
                version: PROTOCOL_VERSION,
                pid,
                app_id,
            },
        )
    }

    fn shadow_session() -> Option<DaemonMessage> {
        response(
            3,
            Response::Session(Session {
                version: PROTOCOL_VERSION,
                app_id: SHADOW_GAME,
            }),
        )
    }

    fn unknown_process() -> Option<DaemonMessage> {
        response(
            3,
            Response::Refused {
                reason: RefusalReason::UnknownProcess,
            },
        )
    }

    /// Registers the shadow game as `reborn-observe arm` does, on its own
    /// connection, and checks it was accepted.
    async fn registered(runs: &Runs, app_id: u32) {
        let mut arm = Connection::default();
        let answered = reply_with(&register(app_id), &mut arm, runs, &offline()).await;
        assert_eq!(answered.map(|(m, _)| m), response(6, Response::Accepted));
        assert_eq!(arm.pid, None, "registering binds nothing");
    }

    #[tokio::test]
    async fn a_shadow_session_answers_its_own_app_read_only() {
        let runs = Runs::new();
        registered(&runs, SHADOW_GAME).await;
        let valve = Fake::new(Link::Connected, vec![Ok(recorded())]);
        let mut proxy = Connection::default();
        let hello = reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &valve,
        )
        .await;
        assert_eq!(hello.map(|(m, _)| m), shadow_session());
        assert_eq!(proxy.pid, Some(SHADOW_PID));
        assert!(proxy.shadow.is_some());

        let owned = reply_with(&ownership(SHADOW_GAME), &mut proxy, &runs, &valve).await;
        assert_eq!(
            owned.map(|(m, _)| m),
            response(4, Response::Ownership { owned: true })
        );
        // Strict: other apps, owned or not, and DLC (the shadow session
        // knows none) are refused.
        for app_id in [OTHER_OWNED, GAME, DLC, 730] {
            let refused = reply_with(&ownership(app_id), &mut proxy, &runs, &valve).await;
            assert_eq!(refused.map(|(m, _)| m), not_allowed(), "{app_id}");
        }
        // Stats are a read: accepted, fetched once for the shadow game.
        let Some((message, Some(stats))) =
            reply_with(&request_stats(), &mut proxy, &runs, &valve).await
        else {
            panic!("not accepted");
        };
        assert_eq!(Some(message), response(5, Response::Accepted));
        assert!(event_line(stats).await.is_some());
        assert_eq!(
            *valve.asked.lock().unwrap_or_else(|p| p.into_inner()),
            [SHADOW_GAME]
        );
    }

    #[tokio::test]
    async fn a_shadow_session_is_not_owned_when_the_account_lacks_the_app() {
        let runs = Runs::new();
        registered(&runs, 480).await;
        let mut proxy = Connection::default();
        assert!(matches!(
            reply_with(
                &shadow_hello(SHADOW_PID, 480),
                &mut proxy,
                &runs,
                &offline()
            )
            .await,
            Some((
                DaemonMessage::Response {
                    response: Response::Session(Session { app_id: 480, .. }),
                    ..
                },
                _
            ))
        ));
        let answered = reply_with(&ownership(480), &mut proxy, &runs, &offline()).await;
        assert_eq!(
            answered.map(|(m, _)| m),
            response(4, Response::Ownership { owned: false })
        );
    }

    #[tokio::test]
    async fn a_shadow_hello_needs_an_open_registration_for_its_app() {
        let runs = Runs::new();
        let mut proxy = Connection::default();
        assert_eq!(
            reply(&shadow_hello(SHADOW_PID, SHADOW_GAME), &mut proxy).await,
            unknown_process()
        );
        assert_eq!(proxy.pid, None);

        // Registered for another app.
        registered(&runs, 480).await;
        let refused = reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &offline(),
        )
        .await;
        assert_eq!(refused.map(|(m, _)| m), unknown_process());

        // Claimed once: a second process gets nothing.
        registered(&runs, SHADOW_GAME).await;
        let claimed = reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &offline(),
        )
        .await;
        assert_eq!(claimed.map(|(m, _)| m), shadow_session());
        let mut other = Connection::default();
        let refused = reply_with(
            &shadow_hello(SHADOW_PID + 1, SHADOW_GAME),
            &mut other,
            &runs,
            &offline(),
        )
        .await;
        assert_eq!(refused.map(|(m, _)| m), unknown_process());

        // A process the daemon launched is never a shadow.
        registered(&runs, GAME).await;
        let mut launched = Connection::default();
        let refused = reply_with(&shadow_hello(100, GAME), &mut launched, &runs, &offline()).await;
        assert_eq!(refused.map(|(m, _)| m), unknown_process());
        assert_eq!(launched.pid, None);
    }

    #[tokio::test]
    async fn an_incompatible_shadow_hello_claims_nothing() {
        let runs = Runs::new();
        registered(&runs, SHADOW_GAME).await;
        let mut proxy = Connection::default();
        let old = line(
            3,
            Request::ShadowHello {
                version: Version { major: 9, minor: 0 },
                pid: SHADOW_PID,
                app_id: SHADOW_GAME,
            },
        );
        let refused = reply_with(&old, &mut proxy, &runs, &offline()).await;
        assert_eq!(
            refused.map(|(m, _)| m),
            response(
                3,
                Response::Refused {
                    reason: RefusalReason::IncompatibleVersion
                }
            )
        );
        assert_eq!(proxy.pid, None);
        // The registration is still open.
        let claimed = reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &offline(),
        )
        .await;
        assert_eq!(claimed.map(|(m, _)| m), shadow_session());
    }

    #[tokio::test]
    async fn registering_needs_a_valve_session() {
        let runs = Runs::signed_out();
        let mut arm = Connection::default();
        let refused = reply_with(&register(SHADOW_GAME), &mut arm, &runs, &offline()).await;
        assert_eq!(
            refused.map(|(m, _)| m),
            unavailable(6, UnavailableReason::NotSignedIn)
        );
        let mut proxy = Connection::default();
        let refused = reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &offline(),
        )
        .await;
        assert_eq!(refused.map(|(m, _)| m), unknown_process());
    }

    #[tokio::test]
    async fn a_cancelled_registration_cannot_be_claimed() {
        let runs = Runs::new();
        registered(&runs, SHADOW_GAME).await;
        let mut disarm = Connection::default();
        let cancelled = reply_with(&cancel(), &mut disarm, &runs, &offline()).await;
        assert_eq!(cancelled.map(|(m, _)| m), response(6, Response::Accepted));
        let mut proxy = Connection::default();
        let refused = reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &offline(),
        )
        .await;
        assert_eq!(refused.map(|(m, _)| m), unknown_process());
    }

    #[tokio::test]
    async fn bound_connections_neither_register_nor_switch_kind() {
        let runs = Runs::new();
        // A launched game cannot register or cancel shadow sessions.
        let mut game = Connection::default();
        reply_with(&hello(100, PROTOCOL_VERSION), &mut game, &runs, &offline()).await;
        for request in [register(SHADOW_GAME), cancel()] {
            let refused = reply_with(&request, &mut game, &runs, &offline()).await;
            assert_eq!(
                refused.map(|(m, _)| m),
                unavailable(6, UnavailableReason::NotAllowed)
            );
        }
        // Nor become a shadow session.
        registered(&runs, SHADOW_GAME).await;
        let refused = reply_with(
            &shadow_hello(100, SHADOW_GAME),
            &mut game,
            &runs,
            &offline(),
        )
        .await;
        assert_eq!(refused.map(|(m, _)| m), unknown_process());
        assert!(game.shadow.is_none());

        // A shadow session cannot become a launched run's session, nor
        // register.
        let mut proxy = Connection::default();
        reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &offline(),
        )
        .await;
        for pid in [SHADOW_PID, 100] {
            let refused =
                reply_with(&hello(pid, PROTOCOL_VERSION), &mut proxy, &runs, &offline()).await;
            assert_eq!(refused.map(|(m, _)| m), unknown_process());
        }
        let refused = reply_with(&register(SHADOW_GAME), &mut proxy, &runs, &offline()).await;
        assert_eq!(
            refused.map(|(m, _)| m),
            unavailable(6, UnavailableReason::NotAllowed)
        );
        // Its own shadow hello again keeps the session.
        let again = reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &offline(),
        )
        .await;
        assert_eq!(again.map(|(m, _)| m), shadow_session());
        assert_eq!(proxy.pid, Some(SHADOW_PID));
        assert!(proxy.shadow.is_some());
    }

    #[tokio::test]
    async fn an_ended_valve_session_stops_shadow_sessions() {
        let runs = Runs::new();
        registered(&runs, SHADOW_GAME).await;
        let mut proxy = Connection::default();
        reply_with(
            &shadow_hello(SHADOW_PID, SHADOW_GAME),
            &mut proxy,
            &runs,
            &offline(),
        )
        .await;
        // What the daemon does on any logoff, kick or lost connection.
        assert!(runs.shadows.stop_all());
        let refused = reply_with(&ownership(SHADOW_GAME), &mut proxy, &runs, &offline()).await;
        assert_eq!(refused.map(|(m, _)| m), not_allowed());
        let refused = reply_with(&request_stats(), &mut proxy, &runs, &offline()).await;
        assert_eq!(
            refused.map(|(m, _)| m),
            unavailable(5, UnavailableReason::NotAllowed)
        );
    }

    #[tokio::test]
    async fn a_closed_proxy_connection_ends_its_shadow_session() {
        let valve = Fake::new(Link::Connected, Vec::new());
        let runs = Arc::new(Runs::new());
        registered(&runs, SHADOW_GAME).await;
        let (mut lines, mut write, served) = connect_to(&valve, &runs);
        write
            .write_all(shadow_hello(SHADOW_PID, SHADOW_GAME).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(next(&mut lines).await.contains("\"session\""));
        write
            .write_all(ownership(SHADOW_GAME).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            decode::<DaemonMessage>(&next(&mut lines).await).ok(),
            response(4, Response::Ownership { owned: true })
        );
        // The first claim of these runs has token 1.
        assert!(runs.shadows.get(SHADOW_PID, 1).is_some());
        drop((lines, write));
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), served)
                .await
                .is_ok()
        );
        assert!(runs.shadows.get(SHADOW_PID, 1).is_none());
    }

    // Cloud reads (ADR 0055): from the run's own cloud folder only.

    /// A made-up SteamID: the individual base plus account 123456.
    const STEAM_ID: u64 = 0x0110_0001_0000_0000 + 123_456;
    /// Cave Crawlers' recorded quota (`docs/protocol/cloud.md`).
    const QUOTA: u64 = 1_000_000;
    /// A save as Cave Crawlers names its files (no root token); made-up
    /// content.
    const SAVE: &str = "MainPlayerData.json";
    const SAVE_DATA: &[u8] = b"{\"level\":3}";

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let unique = format!(
                "rebornd-shim-cloud-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|time| time.as_nanos())
                    .unwrap_or_default()
            );
            let dir = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("{error}"));
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// The test runs, with a cloud folder for the game (process 100) that
    /// holds [`SAVE`], limited to `limit`; Bongo Cat (process 200) gets
    /// its own, empty folder below the same data folder.
    fn with_cloud(temp: &TempDir, limit: u64) -> Runs {
        let cloud = RunCloud::new(&temp.0, STEAM_ID, GAME, limit);
        std::fs::create_dir_all(cloud.dir()).unwrap_or_else(|error| panic!("{error}"));
        std::fs::write(cloud.dir().join(SAVE), SAVE_DATA).unwrap_or_else(|error| panic!("{error}"));
        let mut runs = Runs::new();
        runs.game.cloud = Some(Arc::new(cloud));
        runs.bongo.cloud = Some(Arc::new(RunCloud::new(&temp.0, STEAM_ID, BONGO_CAT, limit)));
        runs
    }

    fn cloud_size(name: &str) -> String {
        line(
            8,
            Request::CloudFileSize {
                name: name.to_owned(),
            },
        )
    }

    fn cloud_read(name: &str, max: u32) -> String {
        line(
            8,
            Request::CloudRead {
                name: name.to_owned(),
                max,
            },
        )
    }

    async fn cloud_reply(
        line: &str,
        connection: &mut Connection,
        runs: &Runs,
    ) -> Option<DaemonMessage> {
        reply_with(line, connection, runs, &offline())
            .await
            .map(|(message, _)| message)
    }

    fn file_size(size: Option<u32>) -> Option<DaemonMessage> {
        response(8, Response::FileSize { size })
    }

    fn file_data(data: Option<&[u8]>) -> Option<DaemonMessage> {
        response(
            8,
            Response::FileData {
                data: data.map(|data| Bytes(data.to_vec())),
            },
        )
    }

    fn cloud_refused(reason: UnavailableReason) -> Option<DaemonMessage> {
        unavailable(8, reason)
    }

    #[tokio::test]
    async fn cloud_reads_come_from_the_runs_folder() {
        let temp = TempDir::new("read");
        let runs = with_cloud(&temp, QUOTA);
        let mut connection = Connection::default();
        cloud_reply(&hello(100, PROTOCOL_VERSION), &mut connection, &runs).await;
        assert_eq!(
            cloud_reply(&cloud_size(SAVE), &mut connection, &runs).await,
            file_size(Some(11))
        );
        assert_eq!(
            cloud_reply(&cloud_read(SAVE, 4096), &mut connection, &runs).await,
            file_data(Some(SAVE_DATA))
        );
        // At most the game's buffer.
        assert_eq!(
            cloud_reply(&cloud_read(SAVE, 4), &mut connection, &runs).await,
            file_data(Some(&SAVE_DATA[..4]))
        );
        // A missing file is said to be missing; the shim decides.
        assert_eq!(
            cloud_reply(&cloud_size("other.json"), &mut connection, &runs).await,
            file_size(None)
        );
        assert_eq!(
            cloud_reply(&cloud_read("other.json", 10), &mut connection, &runs).await,
            file_data(None)
        );
        // Reading wrote nothing.
        let names: Vec<_> = std::fs::read_dir(
            runs.game
                .cloud
                .as_ref()
                .map(|c| c.dir())
                .unwrap_or_default(),
        )
        .map(|dir| {
            dir.filter_map(Result::ok)
                .map(|entry| entry.file_name())
                .collect()
        })
        .unwrap_or_default();
        assert_eq!(names, [SAVE]);
    }

    #[tokio::test]
    async fn cloud_reads_never_leave_the_runs_folder() {
        let temp = TempDir::new("unsafe");
        let runs = with_cloud(&temp, QUOTA);
        // Bongo Cat's session: the game's save is in another app's folder.
        let mut connection = Connection::default();
        cloud_reply(&hello(200, PROTOCOL_VERSION), &mut connection, &runs).await;
        assert_eq!(
            cloud_reply(&cloud_read(SAVE, 10), &mut connection, &runs).await,
            file_data(None)
        );
        for name in [
            "../2143030/MainPlayerData.json",
            "..\\2143030\\MainPlayerData.json",
            "/etc/passwd",
            "C:\\Windows\\win.ini",
            "%WinAppDataLocalLow%x/MainPlayerData.json",
            "",
        ] {
            assert_eq!(
                cloud_reply(&cloud_size(name), &mut connection, &runs).await,
                cloud_refused(UnavailableReason::NotAllowed),
                "{name:?}"
            );
            assert_eq!(
                cloud_reply(&cloud_read(name, 10), &mut connection, &runs).await,
                cloud_refused(UnavailableReason::NotAllowed),
                "{name:?}"
            );
        }
    }

    #[tokio::test]
    async fn cloud_files_above_the_quota_are_too_large() {
        let temp = TempDir::new("large");
        let runs = with_cloud(&temp, 4);
        let mut connection = Connection::default();
        cloud_reply(&hello(100, PROTOCOL_VERSION), &mut connection, &runs).await;
        assert_eq!(
            cloud_reply(&cloud_size(SAVE), &mut connection, &runs).await,
            cloud_refused(UnavailableReason::TooLarge)
        );
        assert_eq!(
            cloud_reply(&cloud_read(SAVE, 2), &mut connection, &runs).await,
            cloud_refused(UnavailableReason::TooLarge)
        );
    }

    #[tokio::test]
    async fn cloud_reads_need_a_session_with_a_cloud_folder() {
        let temp = TempDir::new("session");
        let runs = with_cloud(&temp, QUOTA);
        let refused = cloud_refused(UnavailableReason::NotAllowed);
        // No session on this connection.
        let mut connection = Connection::default();
        assert_eq!(
            cloud_reply(&cloud_size(SAVE), &mut connection, &runs).await,
            refused
        );
        // A refused hello binds nothing.
        cloud_reply(&hello(102, PROTOCOL_VERSION), &mut connection, &runs).await;
        assert_eq!(
            cloud_reply(&cloud_read(SAVE, 10), &mut connection, &runs).await,
            refused
        );
        // A run without shim mode has no cloud folder.
        let mut connection = Connection::default();
        cloud_reply(&hello(101, PROTOCOL_VERSION), &mut connection, &runs).await;
        assert_eq!(
            cloud_reply(&cloud_read(SAVE, 10), &mut connection, &runs).await,
            refused
        );
        // The game exited.
        let mut connection = Connection {
            pid: Some(100),
            shadow: None,
        };
        let mut ended = with_cloud(&temp, QUOTA);
        ended.ended = true;
        assert_eq!(
            cloud_reply(&cloud_read(SAVE, 10), &mut connection, &ended).await,
            refused
        );
    }

    #[tokio::test]
    async fn a_shadow_session_gets_no_cloud_reads_yet() {
        let temp = TempDir::new("shadow");
        let runs = with_cloud(&temp, QUOTA);
        registered(&runs, SHADOW_GAME).await;
        let mut proxy = Connection::default();
        assert_eq!(
            cloud_reply(&shadow_hello(SHADOW_PID, SHADOW_GAME), &mut proxy, &runs).await,
            shadow_session()
        );
        for request in [cloud_size(SAVE), cloud_read(SAVE, 10)] {
            assert_eq!(
                cloud_reply(&request, &mut proxy, &runs).await,
                cloud_refused(UnavailableReason::NotAllowed)
            );
        }
    }

    #[tokio::test]
    async fn a_served_cloud_read_longer_than_a_request_line_arrives_whole() {
        // Answers may be longer than the 64 KiB request limit.
        let temp = TempDir::new("served");
        let runs = Arc::new(with_cloud(&temp, QUOTA));
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        if let Some(cloud) = &runs.game.cloud {
            std::fs::write(cloud.dir().join("big.sav"), &big)
                .unwrap_or_else(|error| panic!("{error}"));
        }
        let (mut lines, mut write, served) = connect_to(&offline(), &runs);
        write
            .write_all(hello(100, PROTOCOL_VERSION).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(next(&mut lines).await.contains("\"session\""));
        write
            .write_all(cloud_read("big.sav", 1 << 20).as_bytes())
            .await
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            decode::<DaemonMessage>(&next(&mut lines).await).ok(),
            file_data(Some(&big))
        );
        drop((lines, write));
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), served)
                .await
                .is_ok()
        );
    }
}
