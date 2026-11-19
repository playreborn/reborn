//! Daemon state, request handling and the job queue.

use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reborn_cm::{PUBLIC_BRANCH, Session};
use reborn_core::CoreError;
use reborn_core::install::{InstallEvent, InstallOutcome, InstallRequest, install_app};
use reborn_ipc::protocol::{
    Event, InstalledApp, Job, JobKind, JobResult, JobState, OwnedApp, Request, Response,
};
use reborn_platform::settings;
use tokio::sync::{Notify, broadcast};
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

/// Shared daemon state.
pub struct Daemon {
    events: broadcast::Sender<Event>,
    session: tokio::sync::Mutex<Option<Arc<Session>>>,
    jobs: Mutex<Jobs>,
    job_added: Notify,
    shutdown: Notify,
}

impl Daemon {
    pub fn new() -> Self {
        Self {
            events: broadcast::channel(256).0,
            session: tokio::sync::Mutex::new(None),
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
            Request::Status => Response::Status {
                daemon_version: env!("CARGO_PKG_VERSION").to_owned(),
                account: reborn_core::account::resolve_account(None).ok(),
                signed_in: self.session.lock().await.is_some(),
                active_jobs: self.jobs().active(),
            },
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
            _ => Response::Error {
                message: "request not supported by this daemon".to_owned(),
            },
        }
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
