//! Daemon state, request handling and the job queue.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reborn_cm::confirmation::{GuardCodeKind, GuardPrompt, GuardPromptHandle};
use reborn_cm::{
    Achievement, ChatEvent, ChatMessage, Friend, PUBLIC_BRANCH, Persona, PersonaState,
    Relationship, Session,
};
use reborn_core::CoreError;
use reborn_core::cloud::{RestoreBlocked, RestoreOutcome, restore_missing_saves_with};
use reborn_core::install::{InstallEvent, InstallOutcome, InstallRequest, install_app};
use reborn_core::launch::{LaunchRequest, plan_launch, spawn};
use reborn_core::run_cloud::{RunCloud, quota_from_app_info};
use reborn_ipc::protocol::{
    AchievementInfo, ChatMessageInfo, ChatText, CloudRestoreResult, Event, FriendInfo,
    FriendRelationship, FriendStatus, GuardCode, InstalledApp, Job, JobKind, JobResult, JobState,
    OwnStatus, OwnedApp, Request, Response, Secret, UnreadChatInfo,
};
use reborn_platform::settings;
use tokio::sync::{Notify, broadcast, mpsc, oneshot};
use tracing::{info, warn};

use crate::shim_server::{RunOwnership, ShimHost, ShimRun};
use crate::shim_shadow::ShadowSessions;
use crate::shim_stats::{FetchError, Link, RunStats, StatsBackend};

/// How often the connection to Steam is checked (ADR 0025).
const PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// Longest a sign-in may take. Without a limit, a sign-in caught by a
/// network outage could hang and hold the session lock, blocking every
/// request (live 2026-11-20). Room for three servers that each take about
/// 20 seconds to fail (ADR 0028).
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(90);

/// How long restoring cloud saves may delay a game start (ADR 0035).
const CLOUD_RESTORE_LIMIT: Duration = Duration::from_secs(30);

/// Least time between two typing notices to the same friend (ADR 0026).
const TYPING_INTERVAL: Duration = Duration::from_secs(10);

/// Why the connection to Steam ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lost {
    /// Steam logged the session off with this EResult.
    LoggedOff(i32),
    /// The socket closed without a logoff message.
    Closed,
}

impl Lost {
    fn allows_reconnect(self) -> bool {
        match self {
            Lost::Closed => true,
            Lost::LoggedOff(eresult) => reborn_cm::logoff_allows_reconnect(eresult),
        }
    }
}

/// Wait before the n-th sign-in attempt after a lost connection: 5 s, 15 s,
/// 30 s, 1 min, 2 min, then every 5 minutes (ADR 0025).
fn reconnect_delay(attempt: u32) -> Duration {
    const DELAYS: [u64; 5] = [5, 15, 30, 60, 120];
    Duration::from_secs(DELAYS.get(attempt as usize).copied().unwrap_or(300))
}

/// How often progress events are sent at most.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);
/// Finished jobs kept for `Jobs` requests.
const FINISHED_JOBS_KEPT: usize = 50;

struct JobEntry {
    info: Job,
    request: InstallRequest,
}

#[derive(Default)]
struct Jobs {
    entries: VecDeque<JobEntry>,
    next_id: u64,
    waiting: VecDeque<u64>,
    cancelled: HashSet<u64>,
    running: Option<(u64, Arc<Notify>)>,
}

impl Jobs {
    fn entry(&mut self, job_id: u64) -> Option<&mut JobEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.info.job_id == job_id)
    }

    fn active(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| !matches!(entry.info.state, JobState::Finished { .. }))
            .count()
    }

    /// Drops the oldest finished jobs beyond the limit.
    fn prune(&mut self) {
        let finished = self
            .entries
            .iter()
            .filter(|entry| matches!(entry.info.state, JobState::Finished { .. }))
            .count();
        let mut to_remove = finished.saturating_sub(FINISHED_JOBS_KEPT);
        self.entries.retain(|entry| {
            let remove = to_remove > 0 && matches!(entry.info.state, JobState::Finished { .. });
            if remove {
                to_remove -= 1;
            }
            !remove
        });
    }
}

/// The licensed app IDs of one Valve session.
struct LicensedApps {
    /// The session they came from. A weak reference keeps the allocation,
    /// so a later session can never have the same address.
    session: std::sync::Weak<Session>,
    apps: Arc<BTreeSet<u32>>,
}

impl LicensedApps {
    fn of(&self, session: &Arc<Session>) -> Option<Arc<BTreeSet<u32>>> {
        let current = self.session.upgrade()?;
        Arc::ptr_eq(&current, session).then(|| Arc::clone(&self.apps))
    }
}

/// A password sign-in that is still running (ADR 0014).
struct PendingSignIn {
    id: u64,
    /// Taken by the first `submit_guard_code`.
    code: Option<oneshot::Sender<reborn_cm::Secret>>,
    cancel: Arc<Notify>,
}

/// Shared daemon state.
pub struct Daemon {
    events: broadcast::Sender<Event>,
    session: tokio::sync::Mutex<Option<Arc<Session>>>,
    sign_in: Mutex<Option<PendingSignIn>>,
    next_sign_in: AtomicU64,
    /// Apps whose game process (started by us) is still running.
    running: Arc<Mutex<BTreeSet<u32>>>,
    /// Process ID of each game process we started, with its app and, in
    /// shim mode, its ownership data (for shim sessions, ADR 0047, 0055).
    launched: Arc<Mutex<BTreeMap<u32, ShimRun>>>,
    /// Read-only shadow sessions for processes `reborn-observe arm`
    /// registered (ADR 0055); dropped whenever the Valve session ends.
    shadows: ShadowSessions,
    /// The app IDs the licences of one Valve session grant, loaded on the
    /// first shim-mode launch of that session (ADR 0055: one licence list
    /// per daemon session). Tied to the session it came from: a new
    /// session after a reconnect or sign-in loads it again.
    licensed_apps: tokio::sync::Mutex<Option<LicensedApps>>,
    /// Friends of the signed-in account with their latest persona, kept
    /// current by the watcher (initial list plus every change).
    friends: Arc<Mutex<BTreeMap<u64, FriendInfo>>>,
    /// Whether the watcher has filled `friends` for the current session.
    friends_loaded: Arc<AtomicBool>,
    /// The task that follows persona updates of the current session.
    friends_watch: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// When the last typing notice went to each friend.
    typing_sent: Mutex<HashMap<u64, Instant>>,
    /// Whether a lost connection is being signed in again (ADR 0025), so
    /// a shim's stats request can tell "offline" from "signed out".
    reconnecting: AtomicBool,
    /// Wakes shim stats requests that wait for a session (ADR 0055).
    session_ready: Notify,
    /// Whether the account is signed out for good: no session yet, the
    /// user signed out, Steam ended the session, or signing in again was
    /// refused. A session that was only forgotten after an error (the next
    /// request signs in again) is not signed out: a shim's stats request
    /// keeps waiting for it (ADR 0055). Kept here instead of asking the
    /// keyring for a stored login, so the shim path never reads secrets.
    signed_out: AtomicBool,
    /// Reports a lost connection to the keeper task.
    lost: mpsc::UnboundedSender<Lost>,
    /// Taken once by [`keep_connected`].
    lost_receiver: Mutex<Option<mpsc::UnboundedReceiver<Lost>>>,
    jobs: Mutex<Jobs>,
    job_added: Notify,
    shutdown: Notify,
}

impl Daemon {
    pub fn new() -> Self {
        let (lost, lost_receiver) = mpsc::unbounded_channel();
        Self {
            lost,
            lost_receiver: Mutex::new(Some(lost_receiver)),
            reconnecting: AtomicBool::new(false),
            session_ready: Notify::new(),
            signed_out: AtomicBool::new(true),
            events: broadcast::channel(256).0,
            session: tokio::sync::Mutex::new(None),
            sign_in: Mutex::new(None),
            next_sign_in: AtomicU64::new(1),
            running: Arc::new(Mutex::new(BTreeSet::new())),
            launched: Arc::new(Mutex::new(BTreeMap::new())),
            shadows: ShadowSessions::default(),
            licensed_apps: tokio::sync::Mutex::new(None),
            friends: Arc::new(Mutex::new(BTreeMap::new())),
            friends_loaded: Arc::new(AtomicBool::new(false)),
            friends_watch: Mutex::new(None),
            typing_sent: Mutex::new(HashMap::new()),
            jobs: Mutex::new(Jobs {
                next_id: 1,
                ..Jobs::default()
            }),
            job_added: Notify::new(),
            shutdown: Notify::new(),
        }
    }

    /// A receiver for events, one per connected client.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    fn emit(&self, event: Event) {
        // No receivers just means no frontend is connected.
        let _ = self.events.send(event);
    }

    fn jobs(&self) -> MutexGuard<'_, Jobs> {
        self.jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn running(&self) -> MutexGuard<'_, BTreeSet<u32>> {
        lock(&self.running)
    }

    /// The run we started the process `pid` for, while it runs (ADR 0047).
    /// Under Proton and with launchers the game's own process can differ
    /// from the one we started; such games get no shim session yet.
    pub fn shim_run(&self, pid: u32) -> Option<ShimRun> {
        lock(&self.launched).get(&pid).cloned()
    }

    /// Puts `new` into the session slot (held by the caller). Every drop or
    /// replacement of a session goes through here: the shadow sessions were
    /// registered with the old one's data, so they stop (ADR 0055). They
    /// stop even when the slot was already empty: a registration needs a
    /// session, so nothing current is lost, and no path can keep one.
    fn replace_session(
        &self,
        slot: &mut Option<Arc<Session>>,
        new: Option<Arc<Session>>,
        why: &str,
    ) {
        self.stop_shadows(why);
        *slot = new;
    }

    /// Drops the shadow registration and every shadow session (ADR 0055:
    /// any logoff or kick stops the shadow stage), saying so in the log
    /// when there was one.
    fn stop_shadows(&self, why: &str) {
        if self.shadows.stop_all() {
            warn!(why, "shadow mode stopped: shadow sessions dropped");
        }
    }

    /// The app IDs the licences of `session` grant
    /// ([`Session::licensed_app_ids`]), loaded once per Valve session.
    /// Only counts are logged.
    async fn licensed_apps(&self, session: &Arc<Session>) -> Result<Arc<BTreeSet<u32>>, CoreError> {
        let mut cached = self.licensed_apps.lock().await;
        if let Some(apps) = cached.as_ref().and_then(|cached| cached.of(session)) {
            return Ok(apps);
        }
        let apps = Arc::new(session.licensed_app_ids().await?);
        info!(apps = apps.len(), "shim mode: licensed apps loaded");
        *cached = Some(LicensedApps {
            session: Arc::downgrade(session),
            apps: Arc::clone(&apps),
        });
        Ok(apps)
    }

    fn pending_sign_in(&self) -> MutexGuard<'_, Option<PendingSignIn>> {
        self.sign_in
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Resolves once a frontend asked the daemon to stop.
    pub async fn shutdown_requested(&self) {
        self.shutdown.notified().await;
    }

    pub fn announce_shutdown(&self) {
        self.emit(Event::ShuttingDown);
    }

    /// The Steam session, signing in with the stored token on first use.
    async fn session(&self) -> Result<Arc<Session>, CoreError> {
        let mut cached = self.session.lock().await;
        if let Some(session) = cached.as_ref() {
            return Ok(Arc::clone(session));
        }
        let session =
            match tokio::time::timeout(SIGN_IN_TIMEOUT, reborn_core::account::sign_in(None))
                .await
                .map_err(|_| CoreError::SignInTimeout)
                .and_then(|result| result)
            {
                Ok(session) => session,
                Err(error) => {
                    if signs_out(&error) {
                        self.signed_out.store(true, Ordering::SeqCst);
                        drop(cached);
                        self.session_ready.notify_waiters();
                    }
                    return Err(error);
                }
            };
        let session = Arc::new(session);
        info!(account = session.account(), "signed in to Steam");
        self.replace_session(&mut cached, Some(Arc::clone(&session)), "a new session");
        self.signed_out.store(false, Ordering::SeqCst);
        drop(cached);
        self.session_ready.notify_waiters();
        self.watch_friends(Arc::clone(&session));
        Ok(session)
    }

    /// Follows the friends of a new session: fills the list, asks for
    /// persona data and forwards every change (ADR 0020). A previous
    /// watcher is stopped.
    fn watch_friends(&self, session: Arc<Session>) {
        let friends = Arc::clone(&self.friends);
        let loaded = Arc::clone(&self.friends_loaded);
        loaded.store(false, Ordering::SeqCst);
        let events = self.events.clone();
        let lost = self.lost.clone();
        let task = tokio::spawn(async move {
            let mut logged_off = Box::pin(session.logged_off());
            // The status the user chose in Reborn, if any (ADR 0021).
            if let Some(status) = chosen_own_status()
                && let Err(error) = session.set_persona_state(persona_state(status)).await
            {
                warn!(%error, "cannot set the online status");
            }
            let list = match session.friends().await {
                Ok(list) => list,
                Err(error) => {
                    warn!(%error, "cannot load the friends list");
                    return;
                }
            };
            {
                let mut known = lock(&friends);
                known.clear();
                for friend in &list {
                    known.insert(friend.steam_id, friend_info(friend));
                }
                loaded.store(true, Ordering::SeqCst);
            }
            // Subscribe before asking, so no answer is missed.
            let mut updates = Box::pin(session.persona_updates());
            let mut changes = Box::pin(session.friend_changes());
            let mut chat = Box::pin(session.chat_events());
            let ids: Vec<u64> = list.iter().map(|friend| friend.steam_id).collect();
            if let Err(error) = session.request_personas(&ids).await {
                warn!(%error, "cannot request friend data");
            }
            // No receivers just means no frontend is connected.
            loop {
                tokio::select! {
                    Some(personas) = updates.next() => {
                        for persona in personas {
                            let updated = {
                                let mut known = lock(&friends);
                                known.get_mut(&persona.steam_id).map(|info| {
                                    merge_persona(info, &persona);
                                    info.clone()
                                })
                            };
                            if let Some(friend) = updated {
                                let _ = events.send(Event::FriendUpdated { friend });
                            }
                        }
                    }
                    Some(changed) = changes.next() => {
                        let mut new_ids = Vec::new();
                        for change in changed {
                            info!(
                                steam_id = change.steam_id,
                                relationship = ?change.relationship,
                                "friend relationship changed"
                            );
                            if change.relationship == Relationship::None {
                                if lock(&friends).remove(&change.steam_id).is_some() {
                                    let steam_id = change.steam_id.to_string();
                                    let _ = events.send(Event::FriendRemoved { steam_id });
                                }
                                continue;
                            }
                            let friend = {
                                let mut known = lock(&friends);
                                let info = known.entry(change.steam_id).or_insert_with(|| {
                                    new_ids.push(change.steam_id);
                                    friend_info(&change)
                                });
                                info.relationship = relationship(change.relationship);
                                info.clone()
                            };
                            let _ = events.send(Event::FriendUpdated { friend });
                        }
                        if let Err(error) = session.request_personas(&new_ids).await {
                            warn!(%error, "cannot request friend data");
                        }
                    }
                    Some(event) = chat.next() => {
                        // The text is never logged (ADR 0024).
                        let event = match event {
                            ChatEvent::Message(message) => Event::ChatMessage {
                                message: chat_info(&message),
                            },
                            ChatEvent::Typing { friend } => Event::FriendTyping {
                                steam_id: friend.to_string(),
                            },
                        };
                        let _ = events.send(event);
                    }
                    Some(eresult) = &mut logged_off => {
                        let _ = lost.send(Lost::LoggedOff(eresult));
                        break;
                    }
                    else => break,
                }
            }
        });
        if let Some(old) = lock(&self.friends_watch).replace(task) {
            old.abort();
        }
    }

    /// Handles a lost connection (ADR 0025): drops the session, tells the
    /// frontends, and signs in again with backoff unless another session
    /// took over or the user signed out.
    async fn connection_lost(&self, lost: Lost) {
        let reconnect = lost.allows_reconnect();
        // Set before the session is dropped, so a shim's stats request
        // never sees "signed out" during a reconnect.
        self.reconnecting.store(reconnect, Ordering::SeqCst);
        {
            let mut session = self.session.lock().await;
            self.replace_session(&mut session, None, "the connection to Steam ended");
            if !reconnect {
                self.signed_out.store(true, Ordering::SeqCst);
            }
        }
        if !reconnect {
            // Waiting shim stats requests see "signed out" at once.
            self.session_ready.notify_waiters();
        }
        if let Some(task) = lock(&self.friends_watch).take() {
            task.abort();
        }
        // Dropping the session above stopped shadow mode (ADR 0055: any
        // logoff, kick or lost connection); a reconnect does not bring it
        // back.
        warn!(?lost, reconnect, "connection to Steam lost");
        self.emit(Event::SteamConnection {
            connected: false,
            reconnecting: reconnect,
            reason: Some(if reconnect {
                "lost the connection to Steam".to_owned()
            } else {
                "Steam ended this session, probably because the account signed in elsewhere"
                    .to_owned()
            }),
        });
        if !reconnect {
            return;
        }
        let reconnected = self.reconnect().await;
        if !reconnected {
            let session = self.session.lock().await;
            if session.is_none() {
                self.signed_out.store(true, Ordering::SeqCst);
            }
            drop(session);
        }
        self.reconnecting.store(false, Ordering::SeqCst);
        if !reconnected {
            self.session_ready.notify_waiters();
        }
        if !reconnected {
            return;
        }
        info!("reconnected to Steam");
        self.emit(Event::SteamConnection {
            connected: true,
            reconnecting: false,
            reason: None,
        });
    }

    /// Signs in again with backoff after a lost connection (ADR 0025);
    /// whether a session exists afterwards.
    async fn reconnect(&self) -> bool {
        for attempt in 0.. {
            tokio::time::sleep(reconnect_delay(attempt)).await;
            if self.session.lock().await.is_some() {
                // A request signed in meanwhile.
                break;
            }
            let signed_in = reborn_core::account::resolve_account(None)
                .ok()
                .is_some_and(|account| {
                    reborn_core::account::has_stored_login(&account).unwrap_or(false)
                });
            if !signed_in {
                info!("signed out, not reconnecting");
                return false;
            }
            match self.session().await {
                Ok(_) => break,
                // Only a refused login ends the attempts; network trouble
                // just waits for the next one.
                Err(error @ (CoreError::TokenRejected(_) | CoreError::NotSignedIn(_))) => {
                    warn!(%error, "cannot sign in again");
                    self.emit(Event::SteamConnection {
                        connected: false,
                        reconnecting: false,
                        reason: Some(error.to_string()),
                    });
                    return false;
                }
                Err(error) => warn!(%error, attempt, "reconnecting failed"),
            }
        }
        true
    }

    /// Stops following friends and forgets them.
    fn forget_friends(&self) {
        if let Some(task) = lock(&self.friends_watch).take() {
            task.abort();
        }
        self.friends_loaded.store(false, Ordering::SeqCst);
        lock(&self.friends).clear();
    }

    /// The recent messages with a friend (ADR 0024).
    async fn chat_history(&self, steam_id: &str) -> Response {
        let Ok(friend) = steam_id.parse::<u64>() else {
            return Response::Error {
                message: format!("`{steam_id}` is not a SteamID64"),
            };
        };
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => return error_response(&error),
        };
        let mut result = session.recent_messages(friend, 50).await;
        if result
            .as_ref()
            .is_err_and(reborn_cm::Error::is_connection_lost)
        {
            // Reading is safe to repeat: sign in again and retry once.
            self.replace_session(&mut *self.session.lock().await, None, "connection lost");
            match self.session().await {
                Ok(session) => result = session.recent_messages(friend, 50).await,
                Err(error) => return error_response(&error),
            }
        }
        match result {
            Ok(messages) => Response::ChatMessages {
                steam_id: steam_id.to_owned(),
                messages: messages.iter().map(chat_info).collect(),
            },
            Err(error) => self.steam_failure(error).await,
        }
    }

    /// Sends a message (ADR 0024). Visible to the friend; the text is not
    /// logged.
    async fn send_chat_message(&self, steam_id: &str, text: &ChatText) -> Response {
        let Ok(friend) = steam_id.parse::<u64>() else {
            return Response::Error {
                message: format!("`{steam_id}` is not a SteamID64"),
            };
        };
        if text.as_str().trim().is_empty() {
            return Response::Error {
                message: "the message is empty".to_owned(),
            };
        }
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => return error_response(&error),
        };
        // Not retried: Steam may already have the message.
        match session.send_message(friend, text.as_str()).await {
            Ok(message) => {
                info!(friend, "chat message sent");
                Response::ChatMessageSent {
                    message: chat_info(&message),
                }
            }
            Err(error) => self.steam_failure(error).await,
        }
    }

    /// Tells the friend the user is typing, at most once every
    /// [`TYPING_INTERVAL`] per friend (ADR 0026).
    async fn send_typing(&self, steam_id: &str) -> Response {
        let Ok(friend) = steam_id.parse::<u64>() else {
            return not_a_steam_id(steam_id);
        };
        if !typing_due(&mut lock(&self.typing_sent), friend, Instant::now()) {
            return Response::Ok;
        }
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => return error_response(&error),
        };
        match session.send_typing(friend).await {
            Ok(()) => Response::Ok,
            Err(error) => self.steam_failure(error).await,
        }
    }

    /// Marks a conversation as read (ADR 0026).
    async fn mark_chat_read(&self, steam_id: &str, timestamp: u32) -> Response {
        let Ok(friend) = steam_id.parse::<u64>() else {
            return not_a_steam_id(steam_id);
        };
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => return error_response(&error),
        };
        match session.mark_read(friend, timestamp).await {
            Ok(()) => Response::Ok,
            Err(error) => self.steam_failure(error).await,
        }
    }

    /// Conversations with unread messages (ADR 0026).
    async fn unread_chats(&self) -> Response {
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => return error_response(&error),
        };
        match session.unread_chats().await {
            Ok(chats) => Response::UnreadChats {
                chats: chats
                    .iter()
                    .map(|chat| UnreadChatInfo {
                        steam_id: chat.friend.to_string(),
                        count: chat.count,
                    })
                    .collect(),
            },
            Err(error) => self.steam_failure(error).await,
        }
    }

    /// The achievements of an app (ADR 0030). Only reads; reading is safe
    /// to repeat, so a lost connection is retried once after signing in
    /// again.
    async fn achievements(&self, app_id: u32, language: Option<&str>) -> Response {
        let language = language.unwrap_or(reborn_cm::FALLBACK_LANGUAGE);
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => return error_response(&error),
        };
        let mut result = session.achievements(app_id, language).await;
        if result
            .as_ref()
            .is_err_and(reborn_cm::Error::is_connection_lost)
        {
            self.replace_session(&mut *self.session.lock().await, None, "connection lost");
            match self.session().await {
                Ok(session) => result = session.achievements(app_id, language).await,
                Err(error) => return error_response(&error),
            }
        }
        match result {
            Ok(achievements) => Response::Achievements {
                app_id,
                achievements: achievements.iter().map(achievement_info).collect(),
            },
            Err(error) => self.steam_failure(error).await,
        }
    }

    /// Sends a friend request (ADR 0022).
    async fn add_friend(&self, input: &str) -> Response {
        let Some(steam_id) = reborn_cm::steam_id_from_input(input) else {
            return Response::Error {
                message: "enter a friend code, a SteamID64 or a profile URL (…/profiles/<number>)"
                    .to_owned(),
            };
        };
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => return error_response(&error),
        };
        match session.add_friend(steam_id).await {
            Ok(name) => {
                info!(steam_id, "friend request sent");
                Response::FriendRequestSent {
                    steam_id: steam_id.to_string(),
                    name,
                }
            }
            Err(reborn_cm::Error::Refused { code: 24, .. }) => Response::Error {
                message: INSUFFICIENT_PRIVILEGE.to_owned(),
            },
            Err(error) => self.steam_failure(error).await,
        }
    }

    /// Accepts a request the person sent (ADR 0022).
    async fn answer_friend_request(&self, steam_id: &str, accept: bool) -> Response {
        if !accept {
            return Response::Error {
                message: DECLINE_DISABLED.to_owned(),
            };
        }
        let pending = steam_id.parse::<u64>().ok().filter(|id| {
            lock(&self.friends)
                .get(id)
                .is_some_and(|info| info.relationship == FriendRelationship::RequestReceived)
        });
        let Some(steam_id) = pending else {
            return Response::Error {
                message: "this person has not sent you a friend request".to_owned(),
            };
        };
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => return error_response(&error),
        };
        let result = if accept {
            session.add_friend(steam_id).await.map(drop)
        } else {
            session.decline_friend(steam_id).await
        };
        match result {
            Ok(()) => {
                info!(steam_id, accept, "answered friend request");
                Response::Ok
            }
            Err(error) => self.steam_failure(error).await,
        }
    }

    /// The friends list as it is now, signing in if needed.
    async fn friends(&self) -> Result<Vec<FriendInfo>, CoreError> {
        let session = self.session().await?;
        let list = session.friends().await?;
        let loaded = self.friends_loaded.load(Ordering::SeqCst);
        Ok(current_friends(loaded, &lock(&self.friends), &list))
    }

    /// Answers a failed Steam request: forgets the session so the next
    /// request signs in again, and explains a lost connection.
    async fn steam_failure(&self, error: reborn_cm::Error) -> Response {
        let lost = error.is_connection_lost();
        let error = CoreError::from(error);
        self.forget_session_after(&error).await;
        if lost {
            return Response::Error {
                message: CONNECTION_LOST.to_owned(),
            };
        }
        error_response(&error)
    }

    /// Forgets the session after a Steam error, so the next request signs
    /// in again (the connection may have dropped).
    /// A rejected or missing login also counts as signed out (ADR 0055:
    /// waiting shim stats requests then stop at once).
    async fn forget_session_after(&self, error: &CoreError) {
        let forget = matches!(error, CoreError::Steam(_) | CoreError::TokenRejected(_));
        let sign_out = signs_out(error);
        if !forget && !sign_out {
            return;
        }
        {
            let mut session = self.session.lock().await;
            if forget {
                self.replace_session(
                    &mut session,
                    None,
                    "the session was forgotten after an error",
                );
            }
            if sign_out && session.is_none() {
                self.signed_out.store(true, Ordering::SeqCst);
            }
        }
        if sign_out {
            self.session_ready.notify_waiters();
        }
    }

    /// Answers one request (after the hello).
    pub async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Hello { .. } => hello(),
            Request::Status => {
                let account = reborn_core::account::resolve_account(None).ok();
                // A keyring error only means we cannot tell; report no login.
                let stored_login = account.as_deref().is_some_and(|account| {
                    reborn_core::account::has_stored_login(account).unwrap_or(false)
                });
                Response::Status {
                    daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
                    account,
                    signed_in: self.session.lock().await.is_some(),
                    active_jobs: self.jobs().active(),
                    stored_login,
                    running_games: self.running().iter().copied().collect(),
                }
            }
            Request::Library => match self.library().await {
                Ok(apps) => Response::Library { apps },
                Err(error) => {
                    self.forget_session_after(&error).await;
                    error_response(&error)
                }
            },
            Request::Installed { library } => match installed(library) {
                Ok((library, apps)) => Response::Installed { library, apps },
                Err(error) => error_response(&error),
            },
            Request::Install {
                app_id,
                library,
                branch,
                verify,
            } => {
                let request = InstallRequest {
                    app_id,
                    library,
                    branch: branch.unwrap_or_else(|| PUBLIC_BRANCH.to_owned()),
                    verify,
                };
                Response::JobQueued {
                    job_id: self.queue(request),
                }
            }
            Request::Cancel { job_id } => self.cancel(job_id),
            Request::Jobs => Response::Jobs {
                jobs: self
                    .jobs()
                    .entries
                    .iter()
                    .map(|entry| entry.info.clone())
                    .collect(),
            },
            Request::Shutdown => {
                self.shutdown.notify_waiters();
                Response::Ok
            }
            Request::SignIn { account, password } => self.sign_in(account, password).await,
            Request::SubmitGuardCode { code } => self.submit_guard_code(&code),
            Request::CancelSignIn => self.cancel_sign_in(),
            Request::SignOut => self.sign_out().await,
            Request::Uninstall { app_id, library } => self.uninstall(app_id, library).await,
            Request::GetSettings => current_settings(),
            Request::SetOwnStatus { status } => self.set_own_status(status).await,
            Request::AddFriend { friend } => self.add_friend(&friend).await,
            Request::ChatHistory { steam_id } => self.chat_history(&steam_id).await,
            Request::SendChatMessage { steam_id, text } => {
                self.send_chat_message(&steam_id, &text).await
            }
            Request::SendTyping { steam_id } => self.send_typing(&steam_id).await,
            Request::MarkChatRead {
                steam_id,
                timestamp,
            } => self.mark_chat_read(&steam_id, timestamp).await,
            Request::UnreadChats => self.unread_chats().await,
            Request::Achievements { app_id, language } => {
                self.achievements(app_id, language.as_deref()).await
            }
            Request::AcceptFriend { steam_id } => self.answer_friend_request(&steam_id, true).await,
            // Disabled: on today's Steam, `ClientRemoveFriend` on a pending
            // request left the sender blocked (live, 2026-11-20).
            Request::DeclineFriend { .. } => Response::Error {
                message: DECLINE_DISABLED.to_owned(),
            },
            Request::Friends => match self.friends().await {
                Ok(friends) => Response::Friends { friends },
                Err(error) => {
                    self.forget_session_after(&error).await;
                    error_response(&error)
                }
            },
            Request::SetLibrary { library } => self.set_library(library),
            Request::SetProton { name } => self.set_proton(name),
            Request::Launch {
                app_id,
                library,
                option,
            } => {
                self.launch(LaunchRequest {
                    app_id,
                    library,
                    option,
                })
                .await
            }
            _ => Response::Error {
                message: "request not supported by this daemon".to_owned(),
            },
        }
    }

    /// Signs in with password and Steam Guard. Steam Guard challenges go
    /// out as [`Event::SteamGuard`]; the code comes back through
    /// [`Daemon::submit_guard_code`]. The password is only held for the
    /// duration of this call and never logged.
    async fn sign_in(&self, account: String, password: Secret) -> Response {
        let account = account.trim().to_owned();
        if account.is_empty() || password.expose().is_empty() {
            return Response::Error {
                message: "enter account name and password".to_owned(),
            };
        }
        let (prompt, handle) = GuardPrompt::new();
        let GuardPromptHandle { challenge, code } = handle;
        let cancel = Arc::new(Notify::new());
        let id = self.next_sign_in.fetch_add(1, Ordering::Relaxed);
        let replaced = self.pending_sign_in().replace(PendingSignIn {
            id,
            code: Some(code),
            cancel: Arc::clone(&cancel),
        });
        if let Some(old) = replaced {
            old.cancel.notify_one();
        }

        info!(%account, "signing in with password");
        let password = reborn_cm::Secret::new(password.expose());
        let login = reborn_core::account::sign_in_with_password(&account, &password, prompt);
        let forward_challenge = async {
            if let Ok(challenge) = challenge.await {
                self.emit(Event::SteamGuard {
                    code: challenge.code.map(|kind| match kind {
                        GuardCodeKind::Email => GuardCode::Email,
                        GuardCodeKind::Authenticator => GuardCode::Authenticator,
                    }),
                    confirm_in_app: challenge.confirm_in_app,
                    hint: challenge.hint,
                });
            }
            std::future::pending::<()>().await;
        };
        let result = tokio::select! {
            result = login => Some(result),
            () = cancel.notified() => None,
            () = forward_challenge => None,
        };
        {
            let mut pending = self.pending_sign_in();
            if pending.as_ref().is_some_and(|pending| pending.id == id) {
                *pending = None;
            }
        }

        match result {
            None => Response::Error {
                message: "sign-in cancelled".to_owned(),
            },
            Some(Err(error)) => {
                info!(%account, %error, "sign-in failed");
                error_response(&error)
            }
            Some(Ok(session)) => {
                info!(%account, "signed in to Steam");
                let session = Arc::new(session);
                {
                    let mut current = self.session.lock().await;
                    self.replace_session(&mut current, Some(Arc::clone(&session)), "a new sign-in");
                    self.signed_out.store(false, Ordering::SeqCst);
                }
                self.session_ready.notify_waiters();
                self.watch_friends(session);
                self.emit(Event::SignedIn {
                    account: account.clone(),
                });
                Response::SignedIn { account }
            }
        }
    }

    fn submit_guard_code(&self, code: &Secret) -> Response {
        let sender = self
            .pending_sign_in()
            .as_mut()
            .and_then(|pending| pending.code.take());
        let Some(sender) = sender else {
            return Response::Error {
                message: "no sign-in is waiting for a Steam Guard code".to_owned(),
            };
        };
        match sender.send(reborn_cm::Secret::new(code.expose())) {
            Ok(()) => Response::Ok,
            Err(_) => Response::Error {
                message: "the sign-in no longer waits for a code".to_owned(),
            },
        }
    }

    fn cancel_sign_in(&self) -> Response {
        match self.pending_sign_in().take() {
            Some(pending) => {
                pending.cancel.notify_one();
                Response::Ok
            }
            None => Response::Error {
                message: "no sign-in is running".to_owned(),
            },
        }
    }

    /// Removes the stored login of the current account and drops the session.
    async fn sign_out(&self) -> Response {
        if self.jobs().active() > 0 {
            return Response::Error {
                message: "cancel the downloads first, then sign out".to_owned(),
            };
        }
        let mut session = self.session.lock().await;
        let account = match session.as_ref() {
            Some(session) => session.account().to_owned(),
            None => match reborn_core::account::resolve_account(None) {
                Ok(account) => account,
                Err(error) => return error_response(&error),
            },
        };
        if let Err(error) = reborn_core::account::sign_out(&account) {
            return error_response(&error);
        }
        self.replace_session(&mut session, None, "signed out");
        self.signed_out.store(true, Ordering::SeqCst);
        drop(session);
        self.session_ready.notify_waiters();
        self.forget_friends();
        info!(%account, "signed out");
        self.emit(Event::SignedOut);
        Response::Ok
    }

    /// Sets the library folder for new installs (ADR 0019).
    fn set_library(&self, library: Option<PathBuf>) -> Response {
        if self.jobs().active() > 0 {
            return Response::Error {
                message: "cancel the downloads first, then change the library folder".to_owned(),
            };
        }
        if let Err(error) = settings::set_library(library.as_deref()) {
            return error_response(&CoreError::from(error));
        }
        info!(library = ?library, "library folder changed");
        self.emit(Event::SettingsChanged);
        current_settings()
    }

    /// Remembers the online status and applies it to the current session
    /// (ADR 0021). Acts on the account: visible to friends.
    async fn set_own_status(&self, status: Option<OwnStatus>) -> Response {
        if let Err(error) = settings::set_own_status(status.map(own_status_name)) {
            return error_response(&CoreError::from(error));
        }
        let session = self.session.lock().await.clone();
        if let (Some(status), Some(session)) = (status, session)
            && let Err(error) = session.set_persona_state(persona_state(status)).await
        {
            return self.steam_failure(error).await;
        }
        info!(status = ?status, "own online status changed");
        self.emit(Event::SettingsChanged);
        current_settings()
    }

    /// Chooses the Proton build (ADR 0019).
    fn set_proton(&self, name: Option<String>) -> Response {
        if let Some(name) = &name
            && !reborn_core::launch::installed_protons().contains(name)
        {
            return Response::Error {
                message: format!("Proton build `{name}` is not installed"),
            };
        }
        if let Err(error) = settings::set_proton(name.as_deref()) {
            return error_response(&CoreError::from(error));
        }
        info!(proton = ?name, "Proton choice changed");
        self.emit(Event::SettingsChanged);
        current_settings()
    }

    /// Removes an installed app (ADR 0018). Deleting files needs a running
    /// or queued job and a running game to be ruled out first.
    async fn uninstall(&self, app_id: u32, library: Option<PathBuf>) -> Response {
        if self.running().contains(&app_id) {
            return Response::Error {
                message: format!("app {app_id} is running; close it first"),
            };
        }
        let busy = self.jobs().entries.iter().any(|entry| {
            entry.info.app_id == app_id && !matches!(entry.info.state, JobState::Finished { .. })
        });
        if busy {
            return Response::Error {
                message: format!("app {app_id} is being installed or verified; cancel it first"),
            };
        }
        let library = match library {
            Some(library) => library,
            None => match settings::default_library() {
                Ok(library) => library,
                Err(error) => return error_response(&CoreError::from(error)),
            },
        };
        let result = tokio::task::spawn_blocking(move || {
            reborn_core::uninstall::uninstall_app(&library, app_id)
        })
        .await;
        match result {
            Ok(Ok(report)) => {
                info!(app_id, folder = ?report.removed_dir, "app uninstalled");
                self.emit(Event::AppUninstalled { app_id });
                Response::Ok
            }
            Ok(Err(error)) => error_response(&error),
            Err(error) => Response::Error {
                message: format!("uninstalling failed: {error}"),
            },
        }
    }

    /// Starts an installed game and reports when its process exits.
    async fn launch(&self, request: LaunchRequest) -> Response {
        let app_id = request.app_id;
        if self.running().contains(&app_id) {
            return Response::Error {
                message: format!("app {app_id} is already running"),
            };
        }
        let busy = self.jobs().entries.iter().any(|entry| {
            entry.info.app_id == app_id && !matches!(entry.info.state, JobState::Finished { .. })
        });
        if busy {
            return Response::Error {
                message: format!("app {app_id} is being installed or verified"),
            };
        }
        let session = match self.session().await {
            Ok(session) => session,
            Err(error) => {
                self.forget_session_after(&error).await;
                return error_response(&error);
            }
        };
        let plan = match plan_launch(&session, &request).await {
            Ok(plan) => plan,
            Err(error) => {
                self.forget_session_after(&error).await;
                return error_response(&error);
            }
        };
        let shim_mode = crate::shim_launch::enabled();
        // Missing saves from Steam Cloud first (ADR 0035). Never stops the
        // game: a conflict or an error is only reported. The app info is
        // fetched once, for the restore and, in shim mode, for the cloud
        // quota (ADR 0055); both share the restore's time limit.
        let deadline = tokio::time::Instant::now() + CLOUD_RESTORE_LIMIT;
        let too_long = || "Steam Cloud took too long".to_owned();
        let app_info = match tokio::time::timeout_at(deadline, session.app_info_text(app_id)).await
        {
            Ok(result) => result.map_err(|error| error.to_string()),
            Err(_) => Err(too_long()),
        };
        let restore = match &app_info {
            Ok(app_info) => {
                match tokio::time::timeout_at(
                    deadline,
                    restore_missing_saves_with(
                        &session,
                        app_id,
                        request.library.as_deref(),
                        app_info,
                    ),
                )
                .await
                {
                    Ok(result) => result.map_err(|error| error.to_string()),
                    Err(_) => Err(too_long()),
                }
            }
            Err(message) => Err(message.clone()),
        };
        if let Err(message) = &restore {
            warn!(app_id, %message, "restoring cloud saves failed");
        }
        if let Some(result) = cloud_restore_result(restore) {
            self.emit(Event::CloudRestore { app_id, result });
        }
        if shim_mode && let Err(message) = crate::shim_launch::enter() {
            return Response::Error {
                message: format!("shim mode: {message}"),
            };
        }
        // In shim mode the run's ownership set, once shim mode is entered
        // (so a launch that shim mode refuses asks Valve for nothing) and
        // before the game starts (ADR 0055): without it the launch is
        // refused, so the shim never answers ownership without Valve.
        let ownership = if shim_mode {
            match self.licensed_apps(&session).await {
                Ok(licensed) => Some(Arc::new(ownership_from(&licensed, &plan.dlc_app_ids))),
                Err(error) => {
                    crate::shim_launch::leave();
                    self.forget_session_after(&error).await;
                    return error_response(&error);
                }
            }
        } else {
            None
        };
        // In shim mode the run's cloud folder (ADR 0055), with the size
        // limit from the app's quota, read from the app info fetched above
        // (no further request). Without that app info the launch is
        // refused, like without the ownership set. An app without a quota
        // has no cloud and gets no folder (its reads are refused). Reads
        // only, so far.
        let cloud = if shim_mode {
            let cloud = match &app_info {
                Ok(app_info) => {
                    run_cloud(&session, app_id, app_info).map_err(|error| error.to_string())
                }
                Err(message) => Err(message.clone()),
            };
            match cloud {
                Ok(cloud) => cloud.map(Arc::new),
                Err(message) => {
                    crate::shim_launch::leave();
                    return Response::Error {
                        message: format!("shim mode: cannot read the app's cloud quota: {message}"),
                    };
                }
            }
        } else {
            None
        };
        let mut child = match spawn(&plan) {
            Ok(child) => child,
            Err(error) => {
                if shim_mode {
                    crate::shim_launch::leave();
                }
                return error_response(&error);
            }
        };
        self.running().insert(app_id);
        let pid = child.id();
        // The run's stats are fetched only on the game's first request.
        let stats = ownership.as_ref().map(|_| Arc::new(RunStats::default()));
        lock(&self.launched).insert(
            pid,
            ShimRun {
                app_id,
                ownership,
                stats,
                cloud,
                shadow: false,
            },
        );
        info!(app_id, program = %plan.executable.display(), proton = ?plan.proton, "game started");
        self.emit(Event::GameStarted {
            app_id,
            name: plan.name.clone(),
        });

        let events = self.events.clone();
        let running = Arc::clone(&self.running);
        let launched = Arc::clone(&self.launched);
        tokio::task::spawn_blocking(move || {
            let exit_code = child.wait().ok().and_then(|status| status.code());
            lock(&running).remove(&app_id);
            lock(&launched).remove(&pid);
            if shim_mode {
                crate::shim_launch::leave();
            }
            info!(app_id, ?exit_code, "game exited");
            // No receivers just means no frontend is connected.
            let _ = events.send(Event::GameExited { app_id, exit_code });
        });
        Response::Launched { app_id }
    }

    async fn library(&self) -> Result<Vec<OwnedApp>, CoreError> {
        let session = self.session().await?;
        let apps = session.owned_apps().await?;
        Ok(apps
            .into_iter()
            .map(|app| OwnedApp {
                app_id: app.app_id,
                name: app.name,
                app_type: app.app_type,
                playtime_minutes: app.playtime_minutes,
            })
            .collect())
    }

    fn queue(&self, request: InstallRequest) -> u64 {
        let mut jobs = self.jobs();
        let job_id = jobs.next_id;
        jobs.next_id += 1;
        jobs.entries.push_back(JobEntry {
            info: Job {
                job_id,
                app_id: request.app_id,
                name: None,
                kind: if request.verify {
                    JobKind::Verify
                } else {
                    JobKind::Install
                },
                state: JobState::Queued,
            },
            request,
        });
        jobs.waiting.push_back(job_id);
        drop(jobs);
        self.job_added.notify_one();
        info!(job_id, "job queued");
        job_id
    }

    fn cancel(&self, job_id: u64) -> Response {
        let mut jobs = self.jobs();
        if let Some((running, stop)) = &jobs.running
            && *running == job_id
        {
            stop.notify_one();
            return Response::Ok;
        }
        if jobs.waiting.contains(&job_id) {
            jobs.cancelled.insert(job_id);
            return Response::Ok;
        }
        Response::Error {
            message: format!("job {job_id} is not queued or running"),
        }
    }

    fn finish(&self, job_id: u64, result: JobResult) {
        let mut jobs = self.jobs();
        if let Some(entry) = jobs.entry(job_id) {
            entry.info.state = JobState::Finished {
                result: result.clone(),
            };
        }
        if jobs.running.as_ref().is_some_and(|(id, _)| *id == job_id) {
            jobs.running = None;
        }
        jobs.prune();
        drop(jobs);
        info!(job_id, ?result, "job finished");
        self.emit(Event::JobFinished { job_id, result });
    }

    /// Takes the next waiting job, skipping cancelled ones.
    fn next_job(&self) -> Option<(u64, InstallRequest, Arc<Notify>)> {
        loop {
            let mut jobs = self.jobs();
            let job_id = jobs.waiting.pop_front()?;
            if jobs.cancelled.remove(&job_id) {
                drop(jobs);
                self.finish(job_id, JobResult::Cancelled);
                continue;
            }
            let stop = Arc::new(Notify::new());
            jobs.running = Some((job_id, Arc::clone(&stop)));
            let entry = jobs.entry(job_id)?;
            entry.info.state = JobState::Running {
                bytes_done: 0,
                bytes_total: 0,
            };
            return Some((job_id, entry.request.clone(), stop));
        }
    }

    fn on_install_event(&self, job_id: u64, event: InstallEvent, last_sent: &mut Option<Instant>) {
        match event {
            InstallEvent::Started { name, .. } => {
                let app_id = {
                    let mut jobs = self.jobs();
                    let Some(entry) = jobs.entry(job_id) else {
                        return;
                    };
                    entry.info.name = Some(name.clone());
                    entry.info.app_id
                };
                self.emit(Event::JobStarted {
                    job_id,
                    app_id,
                    name,
                });
            }
            InstallEvent::Progress(progress) => {
                if let Some(entry) = self.jobs().entry(job_id) {
                    entry.info.state = JobState::Running {
                        bytes_done: progress.bytes_done,
                        bytes_total: progress.bytes_total,
                    };
                }
                let finished = progress.bytes_done == progress.bytes_total;
                let due = last_sent.is_none_or(|sent| sent.elapsed() >= PROGRESS_INTERVAL);
                if finished || due {
                    *last_sent = Some(Instant::now());
                    self.emit(Event::JobProgress {
                        job_id,
                        bytes_done: progress.bytes_done,
                        bytes_total: progress.bytes_total,
                    });
                }
            }
            InstallEvent::SteamLibrary(library) => {
                warn!(library = %library.display(), "installing into a Steam library; the official client must be closed");
            }
            other => info!(job_id, ?other, "install event"),
        }
    }
}

/// Watches the connection to Steam: probes it every minute and handles
/// logoffs reported by the friends watcher (ADR 0025).
pub async fn keep_connected(daemon: Arc<Daemon>) {
    let Some(mut lost) = lock(&daemon.lost_receiver).take() else {
        return;
    };
    let mut probe = tokio::time::interval(PROBE_INTERVAL);
    probe.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    probe.tick().await;
    loop {
        let reason = tokio::select! {
            Some(reason) = lost.recv() => reason,
            _ = probe.tick() => {
                let session = daemon.session.lock().await.clone();
                let Some(session) = session else { continue };
                match session.probe().await {
                    Err(error) if error.is_connection_lost() => Lost::Closed,
                    _ => continue,
                }
            }
        };
        daemon.connection_lost(reason).await;
    }
}

/// Runs queued jobs one after another, forever.
pub async fn run_jobs(daemon: Arc<Daemon>) {
    loop {
        let Some((job_id, request, stop)) = daemon.next_job() else {
            daemon.job_added.notified().await;
            continue;
        };
        info!(job_id, app_id = request.app_id, "job started");
        let session = match daemon.session().await {
            Ok(session) => session,
            Err(error) => {
                daemon.finish(
                    job_id,
                    JobResult::Failed {
                        message: error.to_string(),
                    },
                );
                continue;
            }
        };
        let mut last_sent = None;
        let run = install_app(&session, &request, |event| {
            daemon.on_install_event(job_id, event, &mut last_sent);
        });
        let result = tokio::select! {
            outcome = run => match outcome {
                Ok(InstallOutcome::UpToDate { .. }) => JobResult::UpToDate,
                Ok(InstallOutcome::Done(summary)) => JobResult::Done {
                    files_written: summary.report.files,
                    files_unchanged: summary.report.files_unchanged,
                    files_removed: summary.report.files_removed,
                    bytes_downloaded: summary.report.bytes_downloaded,
                    install_dir: summary.install_dir,
                },
                Err(error) => {
                    daemon.forget_session_after(&error).await;
                    JobResult::Failed { message: error.to_string() }
                }
            },
            () = stop.notified() => JobResult::Cancelled,
        };
        daemon.finish(job_id, result);
    }
}

/// The hello answer.
pub fn hello() -> Response {
    Response::Hello {
        major: reborn_ipc::PROTOCOL_MAJOR,
        minor: reborn_ipc::PROTOCOL_MINOR,
        daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

fn installed(library: Option<PathBuf>) -> Result<(PathBuf, Vec<InstalledApp>), CoreError> {
    let library = match library {
        Some(library) => library,
        None => settings::default_library()?,
    };
    let apps = reborn_core::library::installed_apps(&library)?
        .into_iter()
        .map(|app| InstalledApp {
            app_id: app.app_id,
            fully_installed: app.is_fully_installed(),
            name: app.name,
            install_dir: app.install_dir,
            build_id: app.build_id,
            size_on_disk: app.size_on_disk,
        })
        .collect();
    Ok((library, apps))
}

/// Shown when Steam closed the connection, e.g. because the same account
/// signed in elsewhere (`reborn-cli` included; live 2026-11-20).
const CONNECTION_LOST: &str = "The connection to Steam was lost, for example because this \
     account signed in elsewhere (also reborn-cli). Try again; Reborn signs in again.";

/// Why declining is switched off until a safe way is verified.
const DECLINE_DISABLED: &str = "Declining friend requests is disabled for now: on current \
     Steam it blocked the sender instead of just declining. Ignore the request in the \
     official client or on the Steam website.";

/// Steam's answer when the account may not send friend requests
/// (`EResult::InsufficientPrivilege` = 24, live 2026-11-20 with a new test
/// account). Steam restricts new accounts that have not spent money yet;
/// they can still accept requests.
const INSUFFICIENT_PRIVILEGE: &str = "Steam does not let this account send friend requests \
     (EResult 24, insufficient privilege). Steam limits new accounts that have not spent \
     money on Steam yet; they can still accept requests.";

/// The friends as known now: once the watcher has loaded them, its map
/// (initial list plus every change since, live 2026-11-20: answering with
/// the login list hid requests that arrived later); before that, the login
/// list.
fn current_friends(
    loaded: bool,
    known: &BTreeMap<u64, FriendInfo>,
    login_list: &[Friend],
) -> Vec<FriendInfo> {
    if loaded {
        return known.values().cloned().collect();
    }
    login_list
        .iter()
        .map(|friend| {
            known
                .get(&friend.steam_id)
                .cloned()
                .unwrap_or_else(|| friend_info(friend))
        })
        .collect()
}

fn not_a_steam_id(steam_id: &str) -> Response {
    Response::Error {
        message: format!("`{steam_id}` is not a SteamID64"),
    }
}

/// Whether a typing notice to `friend` may go out now; records it if so.
fn typing_due(sent: &mut HashMap<u64, Instant>, friend: u64, now: Instant) -> bool {
    if sent
        .get(&friend)
        .is_some_and(|last| now.duration_since(*last) < TYPING_INTERVAL)
    {
        return false;
    }
    sent.insert(friend, now);
    true
}

fn chat_info(message: &ChatMessage) -> ChatMessageInfo {
    ChatMessageInfo {
        steam_id: message.friend.to_string(),
        from_me: message.from_me,
        text: ChatText::new(message.text.clone()),
        timestamp: message.timestamp,
        ordinal: message.ordinal,
    }
}

/// A list entry before Steam sent its persona.
fn friend_info(friend: &Friend) -> FriendInfo {
    FriendInfo {
        steam_id: friend.steam_id.to_string(),
        name: None,
        status: FriendStatus::Unknown,
        relationship: relationship(friend.relationship),
        game_app_id: None,
        game_name: None,
    }
}

fn relationship(relationship: Relationship) -> FriendRelationship {
    match relationship {
        Relationship::Friend => FriendRelationship::Friend,
        Relationship::RequestRecipient => FriendRelationship::RequestReceived,
        Relationship::RequestInitiator => FriendRelationship::RequestSent,
        Relationship::Blocked | Relationship::IgnoredFriend => FriendRelationship::Blocked,
        Relationship::Ignored => FriendRelationship::Ignored,
        _ => FriendRelationship::Other,
    }
}

/// Applies what Steam sent; fields it left out keep their known value
/// (node-steam-user merges the same way, `ClientPersonaState` handler).
fn merge_persona(info: &mut FriendInfo, persona: &Persona) {
    if let Some(name) = &persona.name {
        info.name = Some(name.clone());
    }
    if let Some(state) = persona.state {
        info.status = match state {
            PersonaState::Offline | PersonaState::Invisible => FriendStatus::Offline,
            PersonaState::Online => FriendStatus::Online,
            PersonaState::Busy => FriendStatus::Busy,
            PersonaState::Away => FriendStatus::Away,
            PersonaState::Snooze => FriendStatus::Snooze,
            PersonaState::LookingToTrade => FriendStatus::LookingToTrade,
            PersonaState::LookingToPlay => FriendStatus::LookingToPlay,
            PersonaState::Unknown(_) => FriendStatus::Unknown,
        };
        if info.status == FriendStatus::Offline {
            info.game_app_id = None;
            info.game_name = None;
        }
    }
    if let Some(app) = persona.game_app_id {
        info.game_app_id = (app != 0).then_some(app);
    }
    if persona.game_name.is_some() || persona.game_app_id == Some(0) {
        info.game_name = persona.game_name.clone();
    }
}

/// The settings as the daemon uses them.
fn current_settings() -> Response {
    let library = match settings::default_library() {
        Ok(library) => library,
        Err(error) => return error_response(&CoreError::from(error)),
    };
    let library_is_builtin = settings::library_setting().ok().flatten().is_none();
    Response::Settings {
        library,
        library_is_builtin,
        proton: settings::proton_setting().ok().flatten(),
        protons: reborn_core::launch::installed_protons(),
        own_status: chosen_own_status(),
    }
}

/// The online status the user chose in Reborn, if any.
fn chosen_own_status() -> Option<OwnStatus> {
    match settings::own_status_setting().ok().flatten()?.as_str() {
        "online" => Some(OwnStatus::Online),
        "away" => Some(OwnStatus::Away),
        "busy" => Some(OwnStatus::Busy),
        "invisible" => Some(OwnStatus::Invisible),
        "offline" => Some(OwnStatus::Offline),
        _ => None,
    }
}

fn own_status_name(status: OwnStatus) -> &'static str {
    match status {
        OwnStatus::Online => "online",
        OwnStatus::Away => "away",
        OwnStatus::Busy => "busy",
        OwnStatus::Invisible => "invisible",
        OwnStatus::Offline => "offline",
    }
}

fn persona_state(status: OwnStatus) -> PersonaState {
    match status {
        OwnStatus::Online => PersonaState::Online,
        OwnStatus::Away => PersonaState::Away,
        OwnStatus::Busy => PersonaState::Busy,
        OwnStatus::Invisible => PersonaState::Invisible,
        OwnStatus::Offline => PersonaState::Offline,
    }
}

/// The daemon's Valve session as the source of a shim run's stats
/// (ADR 0055). It never signs in: only the reconnect logic of ADR 0025
/// does that.
impl StatsBackend for Daemon {
    /// Never waits for the session lock: a sign-in holds it for up to
    /// [`SIGN_IN_TIMEOUT`], and the shim connection's other requests
    /// (ownership) must not stall behind it. A held lock means a sign-in
    /// or a short read: offline for now, the request waits for a session.
    async fn link(&self) -> Link {
        if self.reconnecting.load(Ordering::SeqCst) {
            return Link::Offline;
        }
        match self.session.try_lock() {
            Ok(session) if session.is_some() => Link::Connected,
            // Forgotten after an error: the next sign-in brings it back.
            Ok(_) if !self.signed_out.load(Ordering::SeqCst) => Link::Offline,
            Ok(_) => Link::SignedOut,
            Err(_) => Link::Offline,
        }
    }

    async fn fetch(&self, app_id: u32) -> Result<reborn_cm::UserStats, FetchError> {
        // Not behind a running sign-in either (see `link`).
        let session = match self.session.try_lock() {
            Ok(session) => session.clone(),
            Err(_) => None,
        };
        let Some(session) = session else {
            return Err(FetchError::NoSession);
        };
        match session.user_stats(app_id).await {
            Ok(stats) => Ok(stats),
            Err(reborn_cm::Error::Network(_)) => Err(FetchError::Network),
            Err(_) => Err(FetchError::Unreadable),
        }
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }

    /// Registers for the wakeup before it looks at the session, so a
    /// session set in between is never missed.
    async fn wait_for_session(&self, longest: Duration) {
        let notified = self.session_ready.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        // Return at once when a session exists, or when `link()` would now
        // say SignedOut (a sign-out between the caller's `link()` and
        // `enable()` above notified nobody). Only that exact case: while a
        // reconnect runs or the lock is busy, `link()` says Offline, and
        // returning then would make the caller loop without waiting.
        let signed_out =
            !self.reconnecting.load(Ordering::SeqCst) && self.signed_out.load(Ordering::SeqCst);
        match self.session.try_lock() {
            Ok(session) if session.is_some() => return,
            Ok(_) if signed_out => return,
            _ => {}
        }
        let _ = tokio::time::timeout(longest, notified).await;
    }
}

/// The shim channel's view of the daemon: its launches and its shadow
/// sessions (ADR 0047, 0055).
impl ShimHost for Daemon {
    fn launched(&self, pid: u32) -> Option<ShimRun> {
        self.shim_run(pid)
    }

    fn shadows(&self) -> &ShadowSessions {
        &self.shadows
    }

    /// Registers a shadow session for `app_id` with the owned apps of the
    /// current Valve session (ADR 0055). Never signs in and never waits
    /// behind a sign-in: without a session it is refused (`NotSignedIn`,
    /// or `Offline` while one is being made). Loading the licence list may
    /// ask Valve once per Valve session (cached as for shim-mode launches);
    /// only its size is logged.
    async fn register_shadow(&self, app_id: u32) -> reborn_shim_protocol::Response {
        use reborn_shim_protocol::{Response as ShimResponse, UnavailableReason};
        let unavailable = |reason| ShimResponse::Unavailable { reason };
        match self.link().await {
            Link::Connected => {}
            Link::Offline => return unavailable(UnavailableReason::Offline),
            Link::SignedOut => return unavailable(UnavailableReason::NotSignedIn),
        }
        let session = match self.session.try_lock() {
            Ok(session) => session.clone(),
            Err(_) => None,
        };
        let Some(session) = session else {
            return unavailable(UnavailableReason::Offline);
        };
        let licensed = match self.licensed_apps(&session).await {
            Ok(licensed) => licensed,
            Err(error) => {
                warn!(%error, "shadow mode: cannot load the licensed apps");
                return unavailable(UnavailableReason::Offline);
            }
        };
        // No DLC: the shadow session answers only the game itself
        // (`shim_shadow`).
        self.shadows
            .register(app_id, ownership_from(&licensed, &[]));
        // The Valve session may have ended while the list loaded; its
        // stop must not miss this registration.
        let current = matches!(
            self.session.try_lock(),
            Ok(now) if now.as_ref().is_some_and(|now| Arc::ptr_eq(now, &session))
        );
        if !current {
            // A claim may already have happened in between; drop it too.
            self.shadows.stop_all();
            return unavailable(UnavailableReason::Offline);
        }
        ShimResponse::Accepted
    }

    /// The persona name of the current Valve session (ADR 0056), from the
    /// account info Valve sent unasked at logon; nothing is requested.
    /// Never signs in. Never logged.
    async fn persona_name(
        &self,
    ) -> Result<reborn_cm::PersonaName, reborn_shim_protocol::UnavailableReason> {
        use reborn_shim_protocol::UnavailableReason;
        match self.link().await {
            Link::Connected => {}
            Link::Offline => return Err(UnavailableReason::Offline),
            Link::SignedOut => return Err(UnavailableReason::NotSignedIn),
        }
        let session = match self.session.try_lock() {
            Ok(session) => session.clone(),
            Err(_) => None,
        };
        let session = session.ok_or(UnavailableReason::Offline)?;
        session
            .persona_name()
            .await
            .ok_or(UnavailableReason::NotReceived)
    }
}

/// The cloud folder a shim-mode run of `app_id` reads (ADR 0055):
/// `<Reborn data>/cloud/<account ID>/<app ID>/`, limited by the app's
/// quota (from `app_info`, already fetched); `None` for an app without
/// one. Creates nothing.
fn run_cloud(
    session: &Session,
    app_id: u32,
    app_info: &str,
) -> Result<Option<RunCloud>, CoreError> {
    let Some(limit) = quota_from_app_info(app_id, app_info)? else {
        return Ok(None);
    };
    let data_dir = settings::data_dir()?;
    info!(app_id, limit, "shim mode: cloud quota loaded");
    Ok(Some(RunCloud::new(
        &data_dir,
        session.steam_id(),
        app_id,
        limit,
    )))
}

/// The ownership data a shim-mode run gets (ADR 0055): the apps the
/// account's licences grant (`licensed`, from the daemon session's cache)
/// and the DLC the app's info names.
fn ownership_from(licensed: &BTreeSet<u32>, dlc: &[u32]) -> RunOwnership {
    RunOwnership {
        owned: licensed.clone(),
        dlc: dlc.iter().copied().collect(),
    }
}

/// Locks a mutex; a poisoned lock only means another thread panicked while
/// holding it, the data is still usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether an error means there is no usable login any more (rejected or
/// missing token, no account), so the account counts as signed out.
fn signs_out(error: &CoreError) -> bool {
    matches!(
        error,
        CoreError::TokenRejected(_) | CoreError::NotSignedIn(_) | CoreError::NoAccount
    )
}

fn error_response(error: &CoreError) -> Response {
    Response::Error {
        message: error.to_string(),
    }
}

/// The event for a restore before a game start, `None` if there was
/// nothing to do (ADR 0035).
fn cloud_restore_result(outcome: Result<RestoreOutcome, String>) -> Option<CloudRestoreResult> {
    let report = match outcome {
        Err(message) => return Some(CloudRestoreResult::Failed { message }),
        Ok(RestoreOutcome::Blocked(blocked)) => {
            let message = match blocked {
                RestoreBlocked::Differs(names) => format!(
                    "{} saves differ from Steam Cloud; nothing was changed",
                    names.len()
                ),
                RestoreBlocked::LocalOnly(paths) => format!(
                    "{} local saves are not in Steam Cloud; nothing was changed",
                    paths.len()
                ),
                RestoreBlocked::RootMissing(path) => {
                    format!("{} does not exist; nothing was changed", path.display())
                }
            };
            return Some(CloudRestoreResult::Conflict { message });
        }
        Ok(RestoreOutcome::Done(report)) => report,
    };
    if let Some((name, error)) = report.failed.first() {
        return Some(CloudRestoreResult::Failed {
            message: format!(
                "{} saves could not be downloaded ({name}: {error}); {} restored",
                report.failed.len(),
                report.restored.len()
            ),
        });
    }
    let files = u32::try_from(report.restored.len()).unwrap_or(u32::MAX);
    (files > 0).then_some(CloudRestoreResult::Restored { files })
}

fn achievement_info(achievement: &Achievement) -> AchievementInfo {
    AchievementInfo {
        name: achievement.name.clone(),
        display_name: achievement.display_name.clone(),
        description: achievement.description.clone(),
        hidden: achievement.hidden,
        unlocked_at: achievement.unlocked_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(app_id: u32) -> Request {
        Request::Install {
            app_id,
            library: None,
            branch: None,
            verify: false,
        }
    }

    #[tokio::test]
    async fn queues_lists_and_cancels_jobs() {
        let daemon = Daemon::new();
        let mut events = daemon.subscribe();
        let Response::JobQueued { job_id: first } = daemon.handle(install(480)).await else {
            panic!("expected a job");
        };
        let Response::JobQueued { job_id: second } = daemon.handle(install(10)).await else {
            panic!("expected a job");
        };
        assert_eq!((first, second), (1, 2));

        let Response::Jobs { jobs } = daemon.handle(Request::Jobs).await else {
            panic!("expected jobs");
        };
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|job| job.state == JobState::Queued));

        assert_eq!(
            daemon.handle(Request::Cancel { job_id: second }).await,
            Response::Ok
        );
        assert!(matches!(
            daemon.handle(Request::Cancel { job_id: 99 }).await,
            Response::Error { .. }
        ));

        // The worker takes job 1 and skips the cancelled job 2.
        let (taken, request, _stop) = daemon.next_job().unwrap();
        assert_eq!((taken, request.app_id), (1, 480));
        assert!(daemon.next_job().is_none());
        assert_eq!(
            events.recv().await.unwrap(),
            Event::JobFinished {
                job_id: 2,
                result: JobResult::Cancelled
            }
        );
    }

    #[tokio::test]
    async fn status_reports_active_jobs() {
        let daemon = Daemon::new();
        daemon.handle(install(480)).await;
        let Response::Status {
            active_jobs,
            signed_in,
            ..
        } = daemon.handle(Request::Status).await
        else {
            panic!("expected status");
        };
        assert_eq!((active_jobs, signed_in), (1, false));
    }

    #[tokio::test]
    async fn launch_is_refused_while_running_or_installing() {
        // Both checks happen before signing in, so no network is used.
        let daemon = Daemon::new();
        let launch = |app_id| Request::Launch {
            app_id,
            library: None,
            option: None,
        };
        daemon.running().insert(480);
        assert!(matches!(
            daemon.handle(launch(480)).await,
            Response::Error { .. }
        ));
        let Response::Status { running_games, .. } = daemon.handle(Request::Status).await else {
            panic!("expected status");
        };
        assert_eq!(running_games, [480]);

        daemon.handle(install(10)).await;
        let Response::Error { message } = daemon.handle(launch(10)).await else {
            panic!("expected an error");
        };
        assert!(message.contains("being installed"), "{message}");
    }

    #[tokio::test]
    async fn uninstall_is_refused_while_running_or_installing() {
        // Both checks happen before any file is touched.
        let daemon = Daemon::new();
        let uninstall = |app_id| Request::Uninstall {
            app_id,
            library: None,
        };
        daemon.running().insert(480);
        let Response::Error { message } = daemon.handle(uninstall(480)).await else {
            panic!("expected an error");
        };
        assert!(message.contains("running"), "{message}");

        daemon.handle(install(10)).await;
        let Response::Error { message } = daemon.handle(uninstall(10)).await else {
            panic!("expected an error");
        };
        assert!(message.contains("being installed"), "{message}");
    }

    #[tokio::test]
    async fn library_changes_are_refused_while_downloads_run() {
        // Refused before any setting is written.
        let daemon = Daemon::new();
        daemon.handle(install(480)).await;
        let response = daemon
            .handle(Request::SetLibrary {
                library: Some(PathBuf::from("/never/written")),
            })
            .await;
        assert!(matches!(response, Response::Error { .. }));
    }

    #[tokio::test]
    async fn unknown_proton_builds_are_refused() {
        let daemon = Daemon::new();
        let response = daemon
            .handle(Request::SetProton {
                name: Some("Proton 0.0 that does not exist".to_owned()),
            })
            .await;
        assert!(matches!(response, Response::Error { .. }));
    }

    #[test]
    fn typing_notices_are_spaced_out() {
        let mut sent = HashMap::new();
        let start = Instant::now();
        assert!(typing_due(&mut sent, 1, start));
        assert!(!typing_due(&mut sent, 1, start + Duration::from_secs(9)));
        assert!(
            typing_due(&mut sent, 2, start),
            "other friends are separate"
        );
        assert!(typing_due(&mut sent, 1, start + TYPING_INTERVAL));
    }

    #[test]
    fn cloud_restore_events_only_when_something_happened() {
        use reborn_core::cloud::RestoreReport;

        assert_eq!(
            cloud_restore_result(Ok(RestoreOutcome::Done(RestoreReport::default()))),
            None
        );
        assert_eq!(
            cloud_restore_result(Ok(RestoreOutcome::Done(RestoreReport {
                restored: vec![PathBuf::from("a.sav"), PathBuf::from("b.sav")],
                ..RestoreReport::default()
            }))),
            Some(CloudRestoreResult::Restored { files: 2 })
        );
        assert!(matches!(
            cloud_restore_result(Ok(RestoreOutcome::Blocked(RestoreBlocked::Differs(vec![
                "x".to_owned()
            ])))),
            Some(CloudRestoreResult::Conflict { message }) if message.starts_with("1 saves differ")
        ));
        assert!(matches!(
            cloud_restore_result(Ok(RestoreOutcome::Done(RestoreReport {
                failed: vec![("a.sav".to_owned(), "HTTP 500".to_owned())],
                ..RestoreReport::default()
            }))),
            Some(CloudRestoreResult::Failed { message }) if message.contains("HTTP 500")
        ));
        assert_eq!(
            cloud_restore_result(Err("timed out".to_owned())),
            Some(CloudRestoreResult::Failed {
                message: "timed out".to_owned()
            })
        );
    }

    #[test]
    fn achievements_keep_their_unlock_state() {
        let info = achievement_info(&Achievement {
            name: "PET_1".to_owned(),
            display_name: "Bongo Beat 1".to_owned(),
            description: "1 bongo beat".to_owned(),
            hidden: true,
            unlocked_at: Some(1_795_133_048),
        });
        assert_eq!(
            info,
            AchievementInfo {
                name: "PET_1".to_owned(),
                display_name: "Bongo Beat 1".to_owned(),
                description: "1 bongo beat".to_owned(),
                hidden: true,
                unlocked_at: Some(1_795_133_048),
            }
        );
    }

    #[tokio::test]
    async fn chat_requests_are_checked_before_signing_in() {
        let daemon = Daemon::new();
        for request in [
            Request::ChatHistory {
                steam_id: "not a number".to_owned(),
            },
            Request::SendChatMessage {
                steam_id: "42".to_owned(),
                text: ChatText::new("   "),
            },
            Request::SendTyping {
                steam_id: "not a number".to_owned(),
            },
            Request::MarkChatRead {
                steam_id: "-1".to_owned(),
                timestamp: 1,
            },
        ] {
            assert!(matches!(
                daemon.handle(request).await,
                Response::Error { .. }
            ));
        }
    }

    #[tokio::test]
    async fn only_received_requests_can_be_answered() {
        // Refused before signing in, so no network is used.
        let daemon = Daemon::new();
        lock(&daemon.friends).insert(
            7,
            friend_info(&Friend {
                steam_id: 7,
                relationship: Relationship::Friend,
            }),
        );
        for request in [
            Request::AcceptFriend {
                steam_id: "7".to_owned(),
            },
            Request::DeclineFriend {
                steam_id: "7".to_owned(),
            },
            Request::AcceptFriend {
                steam_id: "not a number".to_owned(),
            },
        ] {
            assert!(matches!(
                daemon.handle(request).await,
                Response::Error { .. }
            ));
        }
        let invalid = Request::AddFriend {
            friend: "https://steamcommunity.com/id/someone".to_owned(),
        };
        assert!(matches!(
            daemon.handle(invalid).await,
            Response::Error { .. }
        ));
    }

    #[test]
    fn reconnecting_backs_off_gently() {
        let delays: Vec<u64> = (0..8).map(|n| reconnect_delay(n).as_secs()).collect();
        assert_eq!(delays, [5, 15, 30, 60, 120, 300, 300, 300]);
        assert!(Lost::Closed.allows_reconnect());
        assert!(Lost::LoggedOff(48).allows_reconnect(), "TryAnotherCM");
        assert!(!Lost::LoggedOff(34).allows_reconnect(), "session replaced");
    }

    #[tokio::test]
    async fn a_replaced_session_is_not_reconnected() {
        // Returns at once without signing in: no network is used.
        let daemon = Daemon::new();
        let mut events = daemon.subscribe();
        tokio::time::timeout(
            Duration::from_secs(1),
            daemon.connection_lost(Lost::LoggedOff(34)),
        )
        .await
        .unwrap();
        assert_eq!(
            events.recv().await.unwrap(),
            Event::SteamConnection {
                connected: false,
                reconnecting: false,
                reason: Some(
                    "Steam ended this session, probably because the account signed in elsewhere"
                        .to_owned()
                ),
            }
        );
    }

    #[test]
    fn own_status_names_round_trip() {
        for status in [
            OwnStatus::Online,
            OwnStatus::Away,
            OwnStatus::Busy,
            OwnStatus::Invisible,
            OwnStatus::Offline,
        ] {
            let name = own_status_name(status);
            let json = serde_json::to_string(&status).unwrap();
            assert_eq!(
                json,
                format!("\"{name}\""),
                "file and wire use the same names"
            );
        }
        assert_eq!(persona_state(OwnStatus::Invisible).to_raw(), 7);
    }

    #[test]
    fn the_current_list_includes_changes_since_login() {
        let at_login = [Friend {
            steam_id: 1,
            relationship: Relationship::RequestRecipient,
        }];
        let mut known = BTreeMap::new();
        // Since login: request 1 declined, request 2 arrived.
        let new_request = Friend {
            steam_id: 2,
            relationship: Relationship::RequestRecipient,
        };
        known.insert(2, friend_info(&new_request));
        let ids = |list: Vec<FriendInfo>| -> Vec<String> {
            list.into_iter().map(|friend| friend.steam_id).collect()
        };
        assert_eq!(ids(current_friends(true, &known, &at_login)), ["2"]);
        // Before the watcher loaded anything, the login list is all there is.
        assert_eq!(
            ids(current_friends(false, &BTreeMap::new(), &at_login)),
            ["1"]
        );
    }

    #[test]
    fn personas_merge_into_known_friends() {
        let friend = Friend {
            steam_id: 42,
            relationship: Relationship::Friend,
        };
        let mut info = friend_info(&friend);
        assert_eq!(info.steam_id, "42");
        assert_eq!(info.status, FriendStatus::Unknown);

        merge_persona(
            &mut info,
            &Persona {
                steam_id: 42,
                name: Some("Alice".to_owned()),
                state: Some(PersonaState::Online),
                game_app_id: Some(480),
                ..Persona::default()
            },
        );
        assert_eq!(info.name.as_deref(), Some("Alice"));
        assert_eq!(info.game_app_id, Some(480));

        // An update without name keeps the name; going offline ends the game.
        merge_persona(
            &mut info,
            &Persona {
                steam_id: 42,
                state: Some(PersonaState::Offline),
                ..Persona::default()
            },
        );
        assert_eq!(info.name.as_deref(), Some("Alice"));
        assert_eq!(info.status, FriendStatus::Offline);
        assert_eq!(info.game_app_id, None);
    }

    #[tokio::test]
    async fn sign_in_needs_account_and_password() {
        let daemon = Daemon::new();
        let response = daemon
            .handle(Request::SignIn {
                account: " ".to_owned(),
                password: Secret::new("x"),
            })
            .await;
        assert!(matches!(response, Response::Error { .. }));
        assert!(daemon.pending_sign_in().is_none());
    }

    #[tokio::test]
    async fn guard_code_goes_to_the_waiting_sign_in_once() {
        let daemon = Daemon::new();
        let (code, received) = oneshot::channel();
        *daemon.pending_sign_in() = Some(PendingSignIn {
            id: 1,
            code: Some(code),
            cancel: Arc::new(Notify::new()),
        });
        let submit = || Request::SubmitGuardCode {
            code: Secret::new("AB12C"),
        };
        assert_eq!(daemon.handle(submit()).await, Response::Ok);
        assert_eq!(received.await.unwrap().expose(), "AB12C");
        assert!(matches!(
            daemon.handle(submit()).await,
            Response::Error { .. }
        ));
    }

    #[tokio::test]
    async fn codes_and_cancels_without_a_sign_in_are_refused() {
        let daemon = Daemon::new();
        let code = Request::SubmitGuardCode {
            code: Secret::new("AB12C"),
        };
        assert!(matches!(daemon.handle(code).await, Response::Error { .. }));
        assert!(matches!(
            daemon.handle(Request::CancelSignIn).await,
            Response::Error { .. }
        ));
    }

    #[tokio::test]
    async fn cancel_wakes_the_waiting_sign_in() {
        let daemon = Daemon::new();
        let cancel = Arc::new(Notify::new());
        *daemon.pending_sign_in() = Some(PendingSignIn {
            id: 1,
            code: None,
            cancel: Arc::clone(&cancel),
        });
        assert_eq!(daemon.handle(Request::CancelSignIn).await, Response::Ok);
        // notify_one stores a permit, so a later wait returns at once.
        tokio::time::timeout(Duration::from_secs(1), cancel.notified())
            .await
            .unwrap();
        assert!(daemon.pending_sign_in().is_none());
    }

    #[tokio::test]
    async fn sign_out_is_refused_while_downloads_run() {
        // Refused before touching the keyring.
        let daemon = Daemon::new();
        daemon.handle(install(480)).await;
        assert!(matches!(
            daemon.handle(Request::SignOut).await,
            Response::Error { .. }
        ));
    }

    #[test]
    fn run_ownership_keeps_owned_apps_and_named_dlc() {
        let licensed = BTreeSet::from([480, 2_143_030]);
        let ownership = ownership_from(&licensed, &[2_444_550, 2_444_553, 2_444_550]);
        assert_eq!(ownership.owned, licensed);
        assert_eq!(
            ownership.dlc,
            BTreeSet::from([2_444_550, 2_444_553]),
            "duplicates collapse"
        );
    }

    #[test]
    fn a_daemon_knows_no_shim_run_before_a_launch() {
        assert_eq!(Daemon::new().shim_run(4242), None);
    }

    #[tokio::test]
    async fn shadow_registration_needs_a_valve_session_and_never_signs_in() {
        use reborn_shim_protocol::{Response as ShimResponse, UnavailableReason};
        let daemon = Daemon::new();
        assert_eq!(
            daemon.register_shadow(332_950).await,
            ShimResponse::Unavailable {
                reason: UnavailableReason::NotSignedIn
            }
        );
        daemon.reconnecting.store(true, Ordering::SeqCst);
        assert_eq!(
            daemon.register_shadow(332_950).await,
            ShimResponse::Unavailable {
                reason: UnavailableReason::Offline
            }
        );
        assert!(daemon.session.lock().await.is_none(), "no sign-in");
        assert!(daemon.shadows.claim(10, 332_950).is_none());
    }

    #[tokio::test]
    async fn a_forgotten_session_stops_shadow_sessions() {
        // A Steam error forgets the session (the next request signs in
        // again); the shadow sessions came from it, so they stop too. No
        // sign-in or keyring is touched: a Steam error does not sign out.
        let daemon = Daemon::new();
        daemon
            .shadows
            .register(332_950, ownership_from(&BTreeSet::from([332_950]), &[]));
        assert!(daemon.shadows.claim(10, 332_950).is_some());
        daemon
            .forget_session_after(&CoreError::Steam(reborn_cm::Error::LicenseListTimeout))
            .await;
        assert!(daemon.shadows.get(10, 1).is_none());

        // An error that forgets nothing keeps them.
        daemon
            .shadows
            .register(332_950, ownership_from(&BTreeSet::new(), &[]));
        assert!(daemon.shadows.claim(11, 332_950).is_some());
        daemon.forget_session_after(&CoreError::SignInTimeout).await;
        assert!(daemon.shadows.get(11, 2).is_some());
    }

    #[tokio::test]
    async fn any_end_of_the_valve_session_stops_shadow_sessions() {
        // Steam ended the session (EResult 6, signed in elsewhere; 34,
        // replaced). Both allow no reconnect, so no sign-in or keyring is
        // touched; a reconnecting loss drops them at the same place, before
        // it signs in again.
        for lost in [Lost::LoggedOff(6), Lost::LoggedOff(34)] {
            let daemon = Daemon::new();
            daemon
                .shadows
                .register(332_950, ownership_from(&BTreeSet::from([332_950]), &[]));
            assert!(daemon.shadows.claim(10, 332_950).is_some());
            daemon
                .shadows
                .register(480, ownership_from(&BTreeSet::new(), &[]));
            assert!(!lost.allows_reconnect());
            daemon.connection_lost(lost).await;
            assert!(daemon.shadows.get(10, 1).is_none(), "{lost:?}");
            assert!(daemon.shadows.claim(11, 480).is_none(), "{lost:?}");
        }
    }

    #[tokio::test]
    async fn shim_stats_never_sign_in() {
        // Without a session the daemon is signed out, or offline while it
        // reconnects; a fetch sends nothing and does not sign in.
        let daemon = Daemon::new();
        assert_eq!(daemon.link().await, Link::SignedOut);
        daemon.reconnecting.store(true, Ordering::SeqCst);
        assert_eq!(daemon.link().await, Link::Offline);
        assert_eq!(daemon.fetch(3_419_430).await, Err(FetchError::NoSession));
        assert!(daemon.session.lock().await.is_none());
    }

    #[tokio::test]
    async fn shim_stats_never_wait_behind_a_sign_in() {
        // A sign-in holds the session lock for up to 90 s; the shim's
        // requests answer at once anyway (Offline: they wait for the
        // session in the background).
        let daemon = Arc::new(Daemon::new());
        let held = daemon.session.lock().await;
        let quick = Duration::from_millis(500);
        for reconnecting in [false, true] {
            daemon.reconnecting.store(reconnecting, Ordering::SeqCst);
            assert_eq!(
                tokio::time::timeout(quick, daemon.link()).await,
                Ok(Link::Offline)
            );
            assert_eq!(
                tokio::time::timeout(quick, daemon.fetch(3_419_430)).await,
                Ok(Err(FetchError::NoSession))
            );
        }
        drop(held);

        // A new session wakes a waiting request before its longest wait.
        let waiter = {
            let daemon = Arc::clone(&daemon);
            tokio::spawn(async move {
                daemon.wait_for_session(Duration::from_secs(300)).await;
            })
        };
        // Notify until the waiter has registered and woken, without
        // relying on when it first runs.
        let woken = tokio::time::timeout(quick, async {
            while !waiter.is_finished() {
                daemon.session_ready.notify_waiters();
                tokio::task::yield_now().await;
            }
        });
        assert!(woken.await.is_ok());
    }

    #[tokio::test]
    async fn a_forgotten_session_is_offline_not_signed_out() {
        // After a session existed, an error that only forgets it (the next
        // request signs in again) keeps shim stats requests waiting.
        let daemon = Daemon::new();
        assert_eq!(daemon.link().await, Link::SignedOut);
        daemon.signed_out.store(false, Ordering::SeqCst);
        assert_eq!(daemon.link().await, Link::Offline);
        assert_eq!(daemon.fetch(3_419_430).await, Err(FetchError::NoSession));
        // Steam ending the session (EResult 6, signed in elsewhere: no
        // reconnect, so no sign-in or keyring is touched) signs out.
        assert!(!Lost::LoggedOff(6).allows_reconnect());
        daemon.connection_lost(Lost::LoggedOff(6)).await;
        assert_eq!(daemon.link().await, Link::SignedOut);
    }

    #[tokio::test]
    async fn a_sign_out_already_done_ends_the_wait_at_once() {
        // Signed out before the wait starts (no notify will come): the wait
        // returns at once, so the caller's next `link()` sees SignedOut.
        let daemon = Daemon::new();
        assert_eq!(daemon.link().await, Link::SignedOut);
        let waited = tokio::time::timeout(
            Duration::from_secs(1),
            daemon.wait_for_session(Duration::from_secs(300)),
        )
        .await;
        assert!(waited.is_ok());
    }

    #[tokio::test]
    async fn the_wait_does_not_end_early_while_offline() {
        // While a reconnect runs, or the session lock is busy, `link()`
        // says Offline; the wait must then really wait (no loop without
        // waiting), even with the signed-out flag set.
        let daemon = Daemon::new();
        daemon.reconnecting.store(true, Ordering::SeqCst);
        assert_eq!(daemon.link().await, Link::Offline);
        let short = Duration::from_millis(200);
        let started = Instant::now();
        daemon.wait_for_session(short).await;
        assert!(started.elapsed() >= short);

        daemon.reconnecting.store(false, Ordering::SeqCst);
        let held = daemon.session.lock().await;
        let started = Instant::now();
        daemon.wait_for_session(short).await;
        assert!(started.elapsed() >= short);
        drop(held);
    }

    /// Runs `action` while a shim stats request waits for a session, and
    /// whether the wait ended early (instead of after its 300 s).
    async fn wakes_a_waiting_request(daemon: &Daemon, action: impl Future<Output = ()>) -> bool {
        let both = async {
            tokio::join!(daemon.wait_for_session(Duration::from_secs(300)), async {
                // Let the wait register first.
                for _ in 0..3 {
                    tokio::task::yield_now().await;
                }
                action.await;
            })
        };
        tokio::time::timeout(Duration::from_secs(1), both)
            .await
            .is_ok()
    }

    #[tokio::test]
    async fn a_lost_login_signs_out_and_wakes_waiting_requests() {
        // A missing or rejected login counts as signed out, and waiting
        // shim stats requests are woken to see it at once.
        for error in [
            CoreError::NotSignedIn("test".to_owned()),
            CoreError::NoAccount,
        ] {
            let daemon = Daemon::new();
            daemon.signed_out.store(false, Ordering::SeqCst);
            assert_eq!(daemon.link().await, Link::Offline);
            assert!(wakes_a_waiting_request(&daemon, daemon.forget_session_after(&error)).await);
            assert_eq!(daemon.link().await, Link::SignedOut);
        }
        // Other errors only forget: still waiting for a session.
        let daemon = Daemon::new();
        daemon.signed_out.store(false, Ordering::SeqCst);
        daemon.forget_session_after(&CoreError::SignInTimeout).await;
        assert_eq!(daemon.link().await, Link::Offline);
    }

    #[tokio::test]
    async fn steam_ending_the_session_wakes_waiting_requests() {
        let daemon = Daemon::new();
        daemon.signed_out.store(false, Ordering::SeqCst);
        assert!(wakes_a_waiting_request(&daemon, daemon.connection_lost(Lost::LoggedOff(6))).await);
        assert_eq!(daemon.link().await, Link::SignedOut);
    }

    #[test]
    fn finished_jobs_are_pruned() {
        let mut jobs = Jobs::default();
        for job_id in 0..(FINISHED_JOBS_KEPT as u64 + 5) {
            jobs.entries.push_back(JobEntry {
                info: Job {
                    job_id,
                    app_id: 1,
                    name: None,
                    kind: JobKind::Install,
                    state: JobState::Finished {
                        result: JobResult::UpToDate,
                    },
                },
                request: InstallRequest {
                    app_id: 1,
                    library: None,
                    branch: PUBLIC_BRANCH.to_owned(),
                    verify: false,
                },
            });
        }
        jobs.prune();
        assert_eq!(jobs.entries.len(), FINISHED_JOBS_KEPT);
        assert_eq!(jobs.entries.front().unwrap().info.job_id, 5);
    }
}
