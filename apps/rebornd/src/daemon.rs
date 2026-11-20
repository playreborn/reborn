//! Daemon state, request handling and the job queue.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reborn_cm::confirmation::{GuardCodeKind, GuardPrompt, GuardPromptHandle};
use reborn_cm::{PUBLIC_BRANCH, Session};
use reborn_core::CoreError;
use reborn_core::install::{InstallEvent, InstallOutcome, InstallRequest, install_app};
use reborn_core::launch::{LaunchRequest, plan_launch, spawn};
use reborn_ipc::protocol::{
    Event, GuardCode, InstalledApp, Job, JobKind, JobResult, JobState, OwnedApp, Request, Response,
    Secret,
};
use reborn_platform::settings;
use tokio::sync::{Notify, broadcast, oneshot};
use tracing::{info, warn};

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
    jobs: Mutex<Jobs>,
    job_added: Notify,
    shutdown: Notify,
}

impl Daemon {
    pub fn new() -> Self {
        Self {
            events: broadcast::channel(256).0,
            session: tokio::sync::Mutex::new(None),
            sign_in: Mutex::new(None),
            next_sign_in: AtomicU64::new(1),
            running: Arc::new(Mutex::new(BTreeSet::new())),
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
        let session = Arc::new(reborn_core::account::sign_in(None).await?);
        info!(account = session.account(), "signed in to Steam");
        *cached = Some(Arc::clone(&session));
        Ok(session)
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
                *self.session.lock().await = Some(Arc::new(session));
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
        info!(%account, "signed out");
        self.emit(Event::SignedOut);
        Response::Ok
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
