//! Daemon state, request handling and the job queue.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reborn_cm::confirmation::{GuardCodeKind, GuardPrompt, GuardPromptHandle};
use reborn_cm::{
    ChatEvent, ChatMessage, Friend, PUBLIC_BRANCH, Persona, PersonaState, Relationship, Session,
};
use reborn_core::CoreError;
use reborn_core::install::{InstallEvent, InstallOutcome, InstallRequest, install_app};
use reborn_core::launch::{LaunchRequest, plan_launch, spawn};
use reborn_ipc::protocol::{
    ChatMessageInfo, ChatText, Event, FriendInfo, FriendRelationship, FriendStatus, GuardCode,
    InstalledApp, Job, JobKind, JobResult, JobState, OwnStatus, OwnedApp, Request, Response,
    Secret, UnreadChatInfo,
};
use reborn_platform::settings;
use tokio::sync::{Notify, broadcast, mpsc, oneshot};
use tracing::{info, warn};

/// How often the connection to Steam is checked (ADR 0025).
const PROBE_INTERVAL: Duration = Duration::from_secs(60);

/// Longest a sign-in may take. Without a limit, a sign-in caught by a
/// network outage could hang and hold the session lock, blocking every
/// request (live 2026-11-20).
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(60);

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
    /// Friends of the signed-in account with their latest persona, kept
    /// current by the watcher (initial list plus every change).
    friends: Arc<Mutex<BTreeMap<u64, FriendInfo>>>,
    /// Whether the watcher has filled `friends` for the current session.
    friends_loaded: Arc<AtomicBool>,
    /// The task that follows persona updates of the current session.
    friends_watch: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// When the last typing notice went to each friend.
    typing_sent: Mutex<HashMap<u64, Instant>>,
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
            events: broadcast::channel(256).0,
            session: tokio::sync::Mutex::new(None),
            sign_in: Mutex::new(None),
            next_sign_in: AtomicU64::new(1),
            running: Arc::new(Mutex::new(BTreeSet::new())),
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
        let session = tokio::time::timeout(SIGN_IN_TIMEOUT, reborn_core::account::sign_in(None))
            .await
            .map_err(|_| CoreError::SignInTimeout)??;
        let session = Arc::new(session);
        info!(account = session.account(), "signed in to Steam");
        *cached = Some(Arc::clone(&session));
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
        *self.session.lock().await = None;
        if let Some(task) = lock(&self.friends_watch).take() {
            task.abort();
        }
        let reconnect = lost.allows_reconnect();
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
                return;
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
                    return;
                }
                Err(error) => warn!(%error, attempt, "reconnecting failed"),
            }
        }
        info!("reconnected to Steam");
        self.emit(Event::SteamConnection {
            connected: true,
            reconnecting: false,
            reason: None,
        });
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
            *self.session.lock().await = None;
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
    async fn forget_session_after(&self, error: &CoreError) {
        if matches!(error, CoreError::Steam(_) | CoreError::TokenRejected(_)) {
            *self.session.lock().await = None;
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
                *self.session.lock().await = Some(Arc::clone(&session));
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
        *session = None;
        drop(session);
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
        let mut child = match spawn(&plan) {
            Ok(child) => child,
            Err(error) => return error_response(&error),
        };
        self.running().insert(app_id);
        info!(app_id, program = %plan.executable.display(), proton = ?plan.proton, "game started");
        self.emit(Event::GameStarted {
            app_id,
            name: plan.name.clone(),
        });

        let events = self.events.clone();
        let running = Arc::clone(&self.running);
        tokio::task::spawn_blocking(move || {
            let exit_code = child.wait().ok().and_then(|status| status.code());
            lock(&running).remove(&app_id);
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

/// Locks a mutex; a poisoned lock only means another thread panicked while
/// holding it, the data is still usable.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn error_response(error: &CoreError) -> Response {
    Response::Error {
        message: error.to_string(),
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
