//! The account's stats for a shim run (ADR 0055, stage 0): the daemon side
//! of `RequestStats`. Reading only: nothing here writes stats or sends a
//! store message (red line 4).
//!
//! Rules, all from ADR 0055:
//!
//! - **Only the session's own app.** `RequestStats` carries no app ID; the
//!   daemon asks Valve for the app of the connection's run, never another.
//!   A connection without a session, or a run not started in shim mode,
//!   gets `NotAllowed` (checked in `shim_server` before anything else).
//! - **At most one `ClientGetUserStats` per run** ("Rate limits"): the
//!   first request of a run fetches; every later one is answered from the
//!   run's cache, or joins the fetch that is running. The cache is kept in
//!   memory for the run only and dropped when the game exits.
//! - **Not signed in:** no Valve session and no reconnect running gives
//!   `NotSignedIn`; nothing is fetched.
//! - **Offline** ("Offline, not signed in, daemon gone"): a request that
//!   has not reached Valve yet waits while `rebornd` reconnects (ADR
//!   0025), also when the connection is already lost at the request: it is
//!   accepted and fetched once a session exists. It never signs in itself
//!   (the reconnect logic stays the only place that does), and wakes as
//!   soon as a new session is there. Network failures back off from 1 s,
//!   doubling, at most 5 min, and give up after 5 tries ("Rate limits").
//! - **Only while the game runs:** the fetch holds the run only weakly and
//!   stops before any further request to Valve once the run has ended.
//! - **Honest failure:** a request that gives up, or a Valve answer
//!   Reborn cannot read, sends **no** event: the shim never gets a
//!   0x44d with an invented result, and delivering a failure `EResult`
//!   instead waits for an observation (ADR 0055). The run's stats are then
//!   failed for good: later requests get `Unavailable` instead of asking
//!   Valve again (`NotSignedIn` after a sign-out, otherwise `Offline`;
//!   ADR 0055 names no reason for an unreadable answer, so it is `Offline`
//!   too, see `docs/protocol/achievements.md`).
//! - A result other than OK from Valve is Valve's real answer: it is sent
//!   as `StatsReceived` with that result and no names.
//! - Stats and achievement names are account data: only counts and the
//!   result are logged.

use std::sync::{Arc, Weak};
use std::time::Duration;

use reborn_cm::{FALLBACK_LANGUAGE, UserStats};
use reborn_shim_protocol::{AchievementName, Event, Response, UnavailableReason};
use tokio::sync::watch;
use tracing::{info, warn};

/// Valve requests one stats request may make before it gives up
/// (ADR 0055, "Rate limits").
const MAX_TRIES: u32 = 5;

/// Longest wait between two tries (ADR 0055: at most 5 min).
const MAX_DELAY: Duration = Duration::from_secs(300);

/// Wait before the next try after the n-th failed one: 1 s, doubling, at
/// most 5 min (ADR 0055, "Rate limits").
pub fn retry_delay(failed: u32) -> Duration {
    Duration::from_secs(1u64 << failed.min(9)).min(MAX_DELAY)
}

/// Whether `rebornd` has a Valve session right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// Signed in, with a connection.
    Connected,
    /// No usable session at the moment, but `rebornd` is getting one: the
    /// connection is lost and it reconnects, or a sign-in is running.
    Offline,
    /// No session and no reconnect running (signed out, or Steam ended the
    /// session).
    SignedOut,
}

/// Why one fetch did not give an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// No Valve session at the moment; nothing was sent.
    NoSession,
    /// The request was sent but failed on the network.
    Network,
    /// Valve answered, but not readably (no result, another game, a broken
    /// schema). Trying again would give the same.
    Unreadable,
}

/// Where the stats come from: the daemon's Valve session, or a fake in
/// tests.
pub trait StatsBackend: Send + Sync + 'static {
    /// Whether there is a Valve session now. Answers at once; never waits
    /// for a sign-in.
    fn link(&self) -> impl Future<Output = Link> + Send;
    /// One `ClientGetUserStats` for `app_id` on the current session,
    /// without signing in.
    fn fetch(&self, app_id: u32) -> impl Future<Output = Result<UserStats, FetchError>> + Send;
    /// Waits between two tries after a network failure.
    fn sleep(&self, duration: Duration) -> impl Future<Output = ()> + Send;
    /// Waits for a new Valve session, at most `longest`.
    fn wait_for_session(&self, longest: Duration) -> impl Future<Output = ()> + Send;
}

/// Where a run's stats stand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatsState {
    /// Nobody asked yet.
    Idle,
    /// The request to Valve runs.
    Fetching,
    /// Valve answered: the event for the shim, kept for the run.
    Received(Event),
    /// No answer could be had for this run; why, for later requests.
    Failed(UnavailableReason),
}

/// The stats of one shim run, in memory only (ADR 0055, "What is cached").
/// The daemon's launch record holds it; once the game exits it is gone.
pub struct RunStats {
    state: watch::Sender<StatsState>,
}

/// Only the state's kind: the event holds account data.
impl std::fmt::Debug for RunStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match &*self.state.borrow() {
            StatsState::Idle => "idle",
            StatsState::Fetching => "fetching",
            StatsState::Received(_) => "received",
            StatsState::Failed(_) => "failed",
        };
        f.debug_struct("RunStats").field("state", &kind).finish()
    }
}

impl Default for RunStats {
    fn default() -> Self {
        Self {
            state: watch::channel(StatsState::Idle).0,
        }
    }
}

/// Runs are compared by identity: two runs never share their stats.
impl PartialEq for RunStats {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for RunStats {}

impl RunStats {
    /// The current state, for tests.
    #[cfg(test)]
    pub fn state(&self) -> StatsState {
        self.state.borrow().clone()
    }
}

/// The answer to `RequestStats` for a run of `app_id`, and, when accepted,
/// where its event will show up ([`event_when_received`]). Starts the
/// fetch on the run's first request.
pub async fn request_stats<B: StatsBackend>(
    backend: &Arc<B>,
    app_id: u32,
    stats: &Arc<RunStats>,
) -> (Response, Option<watch::Receiver<StatsState>>) {
    let unavailable = |reason| (Response::Unavailable { reason }, None);
    let current = stats.state.borrow().clone();
    match current {
        StatsState::Received(_) | StatsState::Fetching => {
            return (Response::Accepted, Some(stats.state.subscribe()));
        }
        StatsState::Failed(reason) => return unavailable(reason),
        StatsState::Idle => {}
    }
    // Offline: accepted, the fetch waits for the reconnect (ADR 0055).
    if backend.link().await == Link::SignedOut {
        return unavailable(UnavailableReason::NotSignedIn);
    }
    let receiver = stats.state.subscribe();
    // Only the request that moves the run from idle starts the fetch.
    let started = stats.state.send_if_modified(|state| {
        if *state == StatsState::Idle {
            *state = StatsState::Fetching;
            true
        } else {
            false
        }
    });
    if started {
        tokio::spawn(fetch_for_run(
            Arc::clone(backend),
            app_id,
            Arc::downgrade(stats),
        ));
    } else if let StatsState::Failed(reason) = *stats.state.borrow() {
        return unavailable(reason);
    }
    (Response::Accepted, Some(receiver))
}

/// The event once the run's stats are received; `None` when the run's
/// request failed or the run ended (no event is sent then).
pub async fn event_when_received(mut receiver: watch::Receiver<StatsState>) -> Option<Event> {
    let state = receiver
        .wait_for(|state| matches!(state, StatsState::Received(_) | StatsState::Failed(_)))
        .await
        .ok()?;
    match &*state {
        StatsState::Received(event) => Some(event.clone()),
        _ => None,
    }
}

/// Marks a run's stats failed if its fetch ends without an outcome (a
/// panic, or the task being dropped), so they never stay `Fetching`.
struct FetchGuard {
    stats: Weak<RunStats>,
    done: bool,
}

impl Drop for FetchGuard {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        if let Some(stats) = self.stats.upgrade() {
            stats.state.send_if_modified(|state| {
                if *state == StatsState::Fetching {
                    *state = StatsState::Failed(UnavailableReason::Offline);
                    true
                } else {
                    false
                }
            });
        }
    }
}

/// Fetches the run's stats with the retry rules and stores the outcome,
/// while the run exists.
async fn fetch_for_run<B: StatsBackend>(backend: Arc<B>, app_id: u32, stats: Weak<RunStats>) {
    let mut guard = FetchGuard {
        stats: stats.clone(),
        done: false,
    };
    let outcome = match fetch_with_retries(backend.as_ref(), app_id, &stats).await {
        Ok(answer) => match stats_event(&answer) {
            Some(event) => {
                let Event::StatsReceived {
                    result,
                    achievement_names,
                } = &event;
                info!(
                    app_id,
                    result,
                    achievements = achievement_names.len(),
                    stats = answer.stats.len(),
                    "shim stats received"
                );
                StatsState::Received(event)
            }
            None => {
                warn!(
                    app_id,
                    "shim stats: Valve's answer is unreadable, no event sent"
                );
                StatsState::Failed(UnavailableReason::Offline)
            }
        },
        Err(Stop::Ended) => {
            info!(app_id, "shim stats: the run ended, request dropped");
            guard.done = true;
            return;
        }
        Err(Stop::Failed(reason)) => {
            warn!(
                app_id,
                ?reason,
                "shim stats: no answer from Valve, no event sent"
            );
            StatsState::Failed(reason)
        }
    };
    if let Some(stats) = stats.upgrade() {
        stats.state.send_replace(outcome);
    }
    guard.done = true;
}

/// Why the retries stopped without an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stop {
    /// The run ended; nobody needs the answer.
    Ended,
    /// Given up, with the reason later requests get.
    Failed(UnavailableReason),
}

/// Up to [`MAX_TRIES`] requests to Valve, each only while the run still
/// exists. While there is no session the request waits for one (without
/// counting a try) and gives up once `rebornd` is signed out.
async fn fetch_with_retries<B: StatsBackend>(
    backend: &B,
    app_id: u32,
    run: &Weak<RunStats>,
) -> Result<UserStats, Stop> {
    let mut tries = 0;
    let mut waits = 0;
    loop {
        if run.strong_count() == 0 {
            return Err(Stop::Ended);
        }
        match backend.fetch(app_id).await {
            Ok(answer) => return Ok(answer),
            Err(FetchError::Unreadable) => {
                return Err(Stop::Failed(UnavailableReason::Offline));
            }
            Err(FetchError::Network) => {
                tries += 1;
                if tries >= MAX_TRIES {
                    return Err(Stop::Failed(UnavailableReason::Offline));
                }
                backend.sleep(retry_delay(tries - 1)).await;
            }
            Err(FetchError::NoSession) => {
                if backend.link().await == Link::SignedOut {
                    return Err(Stop::Failed(UnavailableReason::NotSignedIn));
                }
                backend.wait_for_session(retry_delay(waits)).await;
                waits += 1;
            }
        }
    }
}

/// The event for Valve's answer, `None` if it cannot be read. Names come
/// from the schema in English (ADR 0055, "Language of achievement names").
pub fn stats_event(answer: &UserStats) -> Option<Event> {
    if !answer.is_ok() {
        return Some(Event::StatsReceived {
            result: answer.result,
            achievement_names: Vec::new(),
        });
    }
    let achievements = answer.achievements(FALLBACK_LANGUAGE).ok()?;
    Some(Event::StatsReceived {
        result: answer.result,
        achievement_names: achievements
            .into_iter()
            .map(|achievement| AchievementName {
                name: achievement.name,
                display_name: achievement.display_name,
            })
            .collect(),
    })
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};

    use reborn_cm::user_stats_from_bytes;

    /// Recorded with the first test account: Bongo Cat (3419430), 28
    /// achievements (`crates/cm/tests/data/README.md`).
    const RECORDED: &[u8] = include_bytes!("../../../crates/cm/tests/data/user_stats.bin");
    pub const BONGO_CAT: u32 = 3_419_430;

    pub fn recorded() -> UserStats {
        user_stats_from_bytes(RECORDED, BONGO_CAT)
            .unwrap_or_else(|error| panic!("fixture: {error}"))
    }

    /// A fake Valve session: answers in the given order, then the last one
    /// again; counts the requests and waits.
    pub struct Fake {
        pub link: Mutex<Link>,
        pub answers: Mutex<Vec<Result<UserStats, FetchError>>>,
        pub fetches: AtomicU32,
        pub sleeps: Mutex<Vec<Duration>>,
        pub asked: Mutex<Vec<u32>>,
        /// How long each wait for a new session could have been at most.
        pub session_waits: Mutex<Vec<Duration>>,
        /// Each fetch takes one permit for good before it answers.
        pub gate: tokio::sync::Semaphore,
        /// Each backoff wait takes one permit for good before it ends.
        pub sleep_gate: tokio::sync::Semaphore,
    }

    impl Fake {
        pub fn new(link: Link, answers: Vec<Result<UserStats, FetchError>>) -> Arc<Self> {
            Arc::new(Self {
                link: Mutex::new(link),
                answers: Mutex::new(answers),
                fetches: AtomicU32::new(0),
                sleeps: Mutex::new(Vec::new()),
                asked: Mutex::new(Vec::new()),
                session_waits: Mutex::new(Vec::new()),
                gate: tokio::sync::Semaphore::new(tokio::sync::Semaphore::MAX_PERMITS),
                sleep_gate: tokio::sync::Semaphore::new(tokio::sync::Semaphore::MAX_PERMITS),
            })
        }

        pub fn fetches(&self) -> u32 {
            self.fetches.load(Ordering::SeqCst)
        }

        /// Lets no fetch through until [`Fake::open`].
        pub fn close(&self) {
            self.gate.forget_permits(self.gate.available_permits());
        }

        /// Lets `fetches` more fetches through.
        pub fn open(&self, fetches: usize) {
            self.gate.add_permits(fetches);
        }

        pub fn set_link(&self, link: Link) {
            *self.link.lock().unwrap_or_else(|p| p.into_inner()) = link;
        }
    }

    impl StatsBackend for Fake {
        async fn link(&self) -> Link {
            *self.link.lock().unwrap_or_else(|p| p.into_inner())
        }

        async fn fetch(&self, app_id: u32) -> Result<UserStats, FetchError> {
            if let Ok(permit) = self.gate.acquire().await {
                permit.forget();
            }
            self.asked
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(app_id);
            let mut answers = self.answers.lock().unwrap_or_else(|p| p.into_inner());
            let answer = if answers.len() > 1 {
                answers.remove(0)
            } else {
                answers.first().cloned().unwrap_or(Err(FetchError::Network))
            };
            // A request without a session never reaches Valve.
            if answer != Err(FetchError::NoSession) {
                self.fetches.fetch_add(1, Ordering::SeqCst);
            }
            answer
        }

        async fn sleep(&self, duration: Duration) {
            self.sleeps
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(duration);
            if let Ok(permit) = self.sleep_gate.acquire().await {
                permit.forget();
            }
        }

        async fn wait_for_session(&self, longest: Duration) {
            self.session_waits
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .push(longest);
            tokio::task::yield_now().await;
        }
    }

    /// Lets spawned tasks run on the test's runtime.
    async fn run_tasks() {
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
    }

    async fn settle(stats: &RunStats) -> StatsState {
        let mut receiver = stats.state.subscribe();
        receiver
            .wait_for(|state| !matches!(state, StatsState::Fetching | StatsState::Idle))
            .await
            .map(|state| state.clone())
            .unwrap_or(StatsState::Idle)
    }

    #[test]
    fn delays_double_from_one_second_up_to_five_minutes() {
        let delays: Vec<u64> = (0..12).map(|n| retry_delay(n).as_secs()).collect();
        assert_eq!(delays, [1, 2, 4, 8, 16, 32, 64, 128, 256, 300, 300, 300]);
    }

    #[test]
    fn the_recorded_answer_gives_english_names() {
        let Some(Event::StatsReceived {
            result,
            achievement_names,
        }) = stats_event(&recorded())
        else {
            panic!("no event");
        };
        assert_eq!(result, 1);
        assert_eq!(achievement_names.len(), 28);
        assert_eq!(
            achievement_names[0],
            AchievementName {
                name: "PET_1".to_owned(),
                display_name: "Bongo Beat 1".to_owned(),
            }
        );
        assert_eq!(achievement_names[27].display_name, "Merchant");
    }

    #[test]
    fn a_refusal_is_sent_with_its_result_and_no_names() {
        let mut refused = recorded();
        refused.result = 15;
        assert_eq!(
            stats_event(&refused),
            Some(Event::StatsReceived {
                result: 15,
                achievement_names: Vec::new(),
            })
        );
        let mut broken = recorded();
        broken.schema = vec![0, b'4'];
        assert_eq!(stats_event(&broken), None);
    }

    #[tokio::test]
    async fn one_fetch_per_run_then_answers_from_the_cache() {
        let fake = Fake::new(Link::Connected, vec![Ok(recorded())]);
        let stats = Arc::new(RunStats::default());
        let (response, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        assert_eq!(response, Response::Accepted);
        let event = event_when_received(receiver.unwrap_or_else(|| panic!("no receiver"))).await;
        assert!(matches!(
            event,
            Some(Event::StatsReceived { result: 1, .. })
        ));

        // Asked again, even after the connection is gone: from the cache.
        fake.set_link(Link::SignedOut);
        for _ in 0..3 {
            let (response, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
            assert_eq!(response, Response::Accepted);
            let again = event_when_received(receiver.unwrap_or_else(|| panic!("no receiver")));
            assert_eq!(again.await, event);
        }
        assert_eq!(fake.fetches(), 1);
        assert_eq!(
            *fake.asked.lock().unwrap_or_else(|p| p.into_inner()),
            [BONGO_CAT]
        );
    }

    #[tokio::test]
    async fn requests_while_fetching_join_the_running_fetch() {
        let fake = Fake::new(Link::Connected, vec![Ok(recorded())]);
        fake.close();
        let stats = Arc::new(RunStats::default());
        let (first, first_receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        let (second, second_receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        assert_eq!((first, second), (Response::Accepted, Response::Accepted));
        assert_eq!(stats.state(), StatsState::Fetching);
        fake.open(1);
        for receiver in [first_receiver, second_receiver] {
            let event = event_when_received(receiver.unwrap_or_else(|| panic!("no receiver")));
            assert!(matches!(event.await, Some(Event::StatsReceived { .. })));
        }
        assert_eq!(fake.fetches(), 1);
    }

    #[tokio::test]
    async fn not_signed_in_fetches_nothing() {
        let fake = Fake::new(Link::SignedOut, vec![Ok(recorded())]);
        let stats = Arc::new(RunStats::default());
        let (response, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        assert_eq!(
            response,
            Response::Unavailable {
                reason: UnavailableReason::NotSignedIn
            }
        );
        assert!(receiver.is_none());
        assert_eq!(stats.state(), StatsState::Idle);
        assert_eq!(fake.fetches(), 0);
    }

    #[tokio::test]
    async fn offline_at_the_request_waits_for_the_reconnect() {
        // ADR 0055: a request that has not reached Valve waits while
        // rebornd reconnects. No session twice, then the new one answers.
        let fake = Fake::new(
            Link::Offline,
            vec![
                Err(FetchError::NoSession),
                Err(FetchError::NoSession),
                Ok(recorded()),
            ],
        );
        let stats = Arc::new(RunStats::default());
        let (response, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        assert_eq!(response, Response::Accepted);
        let receiver = receiver.unwrap_or_else(|| panic!("no receiver"));
        assert!(event_when_received(receiver).await.is_some());
        assert_eq!(fake.fetches(), 1);
        // It waited for a session (woken early by a new one), with the
        // backoff as the longest wait; no network backoff was needed.
        let waits: Vec<u64> = fake
            .session_waits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(Duration::as_secs)
            .collect();
        assert_eq!(waits, [1, 2]);
        assert!(
            fake.sleeps
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .is_empty()
        );
    }

    #[tokio::test]
    async fn no_request_reaches_valve_once_the_run_ended() {
        // The first try fails on the network; the game exits while the
        // fetch backs off: the retry is never sent.
        let fake = Fake::new(
            Link::Connected,
            vec![Err(FetchError::Network), Ok(recorded())],
        );
        fake.sleep_gate
            .forget_permits(fake.sleep_gate.available_permits());
        let stats = Arc::new(RunStats::default());
        let (_, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        let receiver = receiver.unwrap_or_else(|| panic!("no receiver"));
        run_tasks().await;
        // The first try failed; the fetch is in its backoff.
        assert_eq!(fake.fetches(), 1);
        drop(stats);
        fake.sleep_gate.add_permits(1);
        run_tasks().await;
        assert_eq!(fake.fetches(), 1);
        // The waiting side ends without an event.
        assert_eq!(event_when_received(receiver).await, None);

        // A run that ended before its fetch started asks nothing.
        let fake = Fake::new(Link::Connected, vec![Ok(recorded())]);
        let stats = Arc::new(RunStats::default());
        request_stats(&fake, BONGO_CAT, &stats).await;
        drop(stats);
        run_tasks().await;
        assert_eq!(fake.fetches(), 0);
    }

    #[test]
    fn a_fetch_that_ends_without_an_outcome_fails_the_run() {
        // As after a panic or a dropped task: never left fetching.
        let stats = Arc::new(RunStats::default());
        stats.state.send_replace(StatsState::Fetching);
        drop(FetchGuard {
            stats: Arc::downgrade(&stats),
            done: false,
        });
        assert_eq!(
            stats.state(),
            StatsState::Failed(UnavailableReason::Offline)
        );
        // A finished fetch keeps its outcome.
        let event = Event::StatsReceived {
            result: 1,
            achievement_names: Vec::new(),
        };
        stats
            .state
            .send_replace(StatsState::Received(event.clone()));
        drop(FetchGuard {
            stats: Arc::downgrade(&stats),
            done: false,
        });
        assert_eq!(stats.state(), StatsState::Received(event));
    }

    #[test]
    fn run_stats_debug_shows_no_account_data() {
        let stats = RunStats::default();
        stats
            .state
            .send_replace(StatsState::Received(Event::StatsReceived {
                result: 1,
                achievement_names: vec![AchievementName {
                    name: "PET_1".to_owned(),
                    display_name: "Bongo Beat 1".to_owned(),
                }],
            }));
        let text = format!("{stats:?}");
        assert_eq!(text, "RunStats { state: \"received\" }");
    }

    #[tokio::test]
    async fn network_failures_back_off_and_give_up_after_five_tries() {
        let fake = Fake::new(Link::Connected, vec![Err(FetchError::Network)]);
        let stats = Arc::new(RunStats::default());
        let (response, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        assert_eq!(response, Response::Accepted);
        // No event: nothing is made up.
        let receiver = receiver.unwrap_or_else(|| panic!("no receiver"));
        assert_eq!(event_when_received(receiver).await, None);
        assert_eq!(fake.fetches(), 5);
        let sleeps: Vec<u64> = fake
            .sleeps
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(Duration::as_secs)
            .collect();
        assert_eq!(sleeps, [1, 2, 4, 8]);
        // Failed for the run: later requests do not ask Valve again.
        assert_eq!(
            request_stats(&fake, BONGO_CAT, &stats).await.0,
            Response::Unavailable {
                reason: UnavailableReason::Offline
            }
        );
        assert_eq!(fake.fetches(), 5);
    }

    #[tokio::test]
    async fn a_lost_connection_waits_for_the_reconnect() {
        // Lost on the first try, then no session twice (reconnecting),
        // then the new session answers.
        let fake = Fake::new(
            Link::Connected,
            vec![
                Err(FetchError::Network),
                Err(FetchError::NoSession),
                Err(FetchError::NoSession),
                Ok(recorded()),
            ],
        );
        let stats = Arc::new(RunStats::default());
        let (response, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        assert_eq!(response, Response::Accepted);
        fake.set_link(Link::Offline);
        let receiver = receiver.unwrap_or_else(|| panic!("no receiver"));
        assert!(event_when_received(receiver).await.is_some());
        // Two requests reached Valve; the waits without a session did not.
        assert_eq!(fake.fetches(), 2);
    }

    #[tokio::test]
    async fn signing_out_while_waiting_gives_up() {
        let fake = Fake::new(Link::Connected, vec![Err(FetchError::NoSession)]);
        let stats = Arc::new(RunStats::default());
        request_stats(&fake, BONGO_CAT, &stats).await;
        fake.set_link(Link::SignedOut);
        assert_eq!(
            settle(&stats).await,
            StatsState::Failed(UnavailableReason::NotSignedIn)
        );
        assert_eq!(fake.fetches(), 0);
    }

    #[tokio::test]
    async fn an_unreadable_answer_is_not_retried_and_sends_no_event() {
        let mut broken = recorded();
        broken.schema = vec![0, b'4'];
        for answer in [Err(FetchError::Unreadable), Ok(broken)] {
            let fake = Fake::new(Link::Connected, vec![answer]);
            let stats = Arc::new(RunStats::default());
            let (_, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
            let receiver = receiver.unwrap_or_else(|| panic!("no receiver"));
            assert_eq!(event_when_received(receiver).await, None);
            assert_eq!(fake.fetches(), 1);
            assert!(
                fake.sleeps
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .is_empty()
            );
        }
    }

    #[tokio::test]
    async fn a_refusal_from_valve_is_delivered_once() {
        let mut refused = recorded();
        refused.result = 15;
        let fake = Fake::new(Link::Connected, vec![Ok(refused)]);
        let stats = Arc::new(RunStats::default());
        let (_, receiver) = request_stats(&fake, BONGO_CAT, &stats).await;
        let receiver = receiver.unwrap_or_else(|| panic!("no receiver"));
        assert_eq!(
            event_when_received(receiver).await,
            Some(Event::StatsReceived {
                result: 15,
                achievement_names: Vec::new()
            })
        );
        assert_eq!(fake.fetches(), 1);
    }
}
