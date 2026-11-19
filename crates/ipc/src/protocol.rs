//! Messages between frontends and `rebornd`.
//!
//! Wire format (one JSON object per line):
//! - client → daemon: [`ClientFrame`] `{"id": 1, "request": {"type": "status"}}`
//! - daemon → client: [`ServerFrame`], either
//!   `{"id": 1, "response": {...}}` or `{"event": {...}}`.
//!
//! The first request on a connection must be [`Request::Hello`].

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Incompatible changes increase the major version.
pub const PROTOCOL_MAJOR: u16 = 1;
/// Compatible additions increase the minor version.
pub const PROTOCOL_MINOR: u16 = 1;

/// A password or Steam Guard code on its way to the daemon.
///
/// Serialized as a plain JSON string, but `Debug` prints `<redacted>` so it
/// cannot end up in logs. There is no `Display`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// Wraps a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The raw value. Never log or print it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

/// A request with the id its response will carry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientFrame {
    /// Chosen by the client, echoed in the response.
    pub id: u64,
    /// The request.
    pub request: Request,
}

/// What the daemon sends.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ServerFrame {
    /// Answer to the request with the same id.
    Response {
        /// Id of the request.
        id: u64,
        /// The answer.
        response: Response,
    },
    /// Something happened (sent to every connected client).
    Event {
        /// The event.
        event: Event,
    },
}

/// Requests from a frontend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Request {
    /// Must be the first request; the daemon checks the protocol version.
    Hello {
        /// Client's protocol major version.
        major: u16,
        /// Client's protocol minor version.
        minor: u16,
        /// Name of the frontend, e.g. `reborn-cli 0.1.0`.
        client: String,
    },
    /// Daemon version, account and job counts.
    Status,
    /// Apps owned by the signed-in account.
    Library,
    /// Apps installed in a library folder (`None`: Reborn's own library).
    Installed {
        /// Library folder.
        library: Option<PathBuf>,
    },
    /// Queue an install, update or verification.
    Install {
        /// The app.
        app_id: u32,
        /// Library folder (`None`: Reborn's own library).
        library: Option<PathBuf>,
        /// Branch (`None`: `public`).
        branch: Option<String>,
        /// Verify and repair instead of trusting unchanged files.
        verify: bool,
    },
    /// Cancel a queued or running job. A cancelled download resumes later.
    Cancel {
        /// The job.
        job_id: u64,
    },
    /// All jobs the daemon knows.
    Jobs,
    /// Stop the daemon (running jobs are cancelled and resume next time).
    Shutdown,
    /// Sign in with account name and password (ADR 0014). While Steam
    /// Guard waits, the daemon sends [`Event::SteamGuard`]; answer it with
    /// [`Request::SubmitGuardCode`]. A new sign-in cancels a waiting one.
    SignIn {
        /// Steam account name (the login name, not the profile name).
        account: String,
        /// Password.
        password: Secret,
    },
    /// The Steam Guard code for the waiting sign-in.
    SubmitGuardCode {
        /// The code.
        code: Secret,
    },
    /// Give up the waiting sign-in.
    CancelSignIn,
    /// Remove the stored login of the current account from this computer.
    /// Refused while downloads are queued or running.
    SignOut,
}

/// Answers from the daemon.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Response {
    /// Answer to [`Request::Hello`].
    Hello {
        /// Daemon's protocol major version.
        major: u16,
        /// Daemon's protocol minor version.
        minor: u16,
        /// Daemon version.
        daemon_version: String,
    },
    /// Answer to [`Request::Status`].
    Status {
        /// Daemon version.
        daemon_version: String,
        /// Account the daemon uses, if any.
        account: Option<String>,
        /// Whether the daemon is signed in to Steam right now.
        signed_in: bool,
        /// Jobs waiting or running.
        active_jobs: usize,
        /// Whether a login (refresh token) is stored for `account`.
        /// Added in protocol 1.1.
        #[serde(default)]
        stored_login: bool,
    },
    /// Answer to [`Request::Library`].
    Library {
        /// Owned apps.
        apps: Vec<OwnedApp>,
    },
    /// Answer to [`Request::Installed`].
    Installed {
        /// Library folder that was listed.
        library: PathBuf,
        /// Installed apps.
        apps: Vec<InstalledApp>,
    },
    /// Answer to [`Request::Install`].
    JobQueued {
        /// The new job.
        job_id: u64,
    },
    /// Answer to [`Request::Jobs`].
    Jobs {
        /// Jobs, oldest first.
        jobs: Vec<Job>,
    },
    /// Answer to [`Request::SignIn`]: Steam accepted the login.
    SignedIn {
        /// The account.
        account: String,
    },
    /// The request was carried out.
    Ok,
    /// The request failed.
    Error {
        /// Human-readable reason.
        message: String,
    },
}

/// An owned app.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnedApp {
    /// App ID.
    pub app_id: u32,
    /// Name.
    pub name: String,
    /// Steam's app type in lowercase (`game`, `dlc`, ...).
    pub app_type: String,
    /// Total playtime in minutes.
    pub playtime_minutes: u32,
}

/// An app installed in a library folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledApp {
    /// App ID.
    pub app_id: u32,
    /// Name.
    pub name: Option<String>,
    /// Folder below `steamapps/common`.
    pub install_dir: String,
    /// Installed build.
    pub build_id: Option<u64>,
    /// Size on disk.
    pub size_on_disk: Option<u64>,
    /// Whether the appmanifest says "fully installed".
    pub fully_installed: bool,
}

/// What a job does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    /// Install or update.
    Install,
    /// Verify and repair.
    Verify,
}

/// Where a job stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum JobState {
    /// Waiting for earlier jobs.
    Queued,
    /// Downloading or building.
    Running {
        /// Bytes done.
        bytes_done: u64,
        /// Bytes in total.
        bytes_total: u64,
    },
    /// Finished.
    Finished {
        /// The outcome.
        result: JobResult,
    },
}

/// A job.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// Job ID.
    pub job_id: u64,
    /// The app.
    pub app_id: u32,
    /// App name once known.
    pub name: Option<String>,
    /// Install or verify.
    pub kind: JobKind,
    /// Current state.
    pub state: JobState,
}

/// How a job ended.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum JobResult {
    /// The newest version was already installed.
    UpToDate,
    /// Installed, updated or verified.
    Done {
        /// Files written.
        files_written: usize,
        /// Files left alone.
        files_unchanged: usize,
        /// Files removed.
        files_removed: usize,
        /// Bytes downloaded.
        bytes_downloaded: u64,
        /// Game folder.
        install_dir: PathBuf,
    },
    /// Cancelled; a later install resumes.
    Cancelled,
    /// Failed.
    Failed {
        /// Why.
        message: String,
    },
}

/// Pushed to every connected client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Event {
    /// A job started running.
    JobStarted {
        /// The job.
        job_id: u64,
        /// The app.
        app_id: u32,
        /// App name.
        name: String,
    },
    /// Progress of a running job (rate-limited).
    JobProgress {
        /// The job.
        job_id: u64,
        /// Bytes done.
        bytes_done: u64,
        /// Bytes in total.
        bytes_total: u64,
    },
    /// A job ended.
    JobFinished {
        /// The job.
        job_id: u64,
        /// How it ended.
        result: JobResult,
    },
    /// The daemon is shutting down.
    ShuttingDown,
    /// The waiting sign-in needs Steam Guard.
    SteamGuard {
        /// The code to enter, if a code is accepted.
        code: Option<GuardCode>,
        /// Whether approving the login in the Steam mobile app works.
        confirm_in_app: bool,
        /// Steam's hint for the code, e.g. the domain of the email address.
        hint: String,
    },
    /// An account signed in.
    SignedIn {
        /// The account.
        account: String,
    },
    /// The stored login was removed.
    SignedOut,
}

/// Which Steam Guard code is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardCode {
    /// A code Steam sent by email.
    Email,
    /// A code from the Steam Guard authenticator (mobile app).
    Authenticator,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_use_readable_json() {
        let frame = ClientFrame {
            id: 7,
            request: Request::Install {
                app_id: 480,
                library: None,
                branch: None,
                verify: false,
            },
        };
        let json = serde_json::to_string(&frame).unwrap();
        assert_eq!(
            json,
            r#"{"id":7,"request":{"type":"install","app_id":480,"library":null,"branch":null,"verify":false}}"#
        );
        assert_eq!(serde_json::from_str::<ClientFrame>(&json).unwrap(), frame);
    }

    #[test]
    fn server_frames_round_trip_and_are_told_apart() {
        let response = ServerFrame::Response {
            id: 1,
            response: Response::JobQueued { job_id: 3 },
        };
        let event = ServerFrame::Event {
            event: Event::JobFinished {
                job_id: 3,
                result: JobResult::Failed {
                    message: "no servers".to_owned(),
                },
            },
        };
        for frame in [response, event] {
            let json = serde_json::to_string(&frame).unwrap();
            assert_eq!(
                serde_json::from_str::<ServerFrame>(&json).unwrap(),
                frame,
                "{json}"
            );
        }
        assert_eq!(
            serde_json::to_string(&Response::Ok).unwrap(),
            r#"{"type":"ok"}"#
        );
    }

    #[test]
    fn secrets_are_plain_strings_on_the_wire_but_hidden_in_debug() {
        let request = Request::SignIn {
            account: "someone".to_owned(),
            password: Secret::new("hunter2"),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"type":"sign_in","account":"someone","password":"hunter2"}"#
        );
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);
        let printed = format!(
            "{request:?} {:#?}",
            Request::SubmitGuardCode {
                code: Secret::new("AB12C")
            }
        );
        assert!(
            !printed.contains("hunter2") && !printed.contains("AB12C"),
            "{printed}"
        );
    }

    #[test]
    fn status_from_a_1_0_daemon_still_parses() {
        let json = r#"{"type":"status","daemon_version":"0.1.0","account":null,"signed_in":false,"active_jobs":0}"#;
        let Response::Status { stored_login, .. } = serde_json::from_str(json).unwrap() else {
            panic!("expected status");
        };
        assert!(!stored_login);
    }

    #[test]
    fn steam_guard_event_json() {
        let event = Event::SteamGuard {
            code: Some(GuardCode::Authenticator),
            confirm_in_app: true,
            hint: String::new(),
        };
        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"type":"steam_guard","code":"authenticator","confirm_in_app":true,"hint":""}"#
        );
    }

    #[test]
    fn unknown_requests_are_rejected() {
        assert!(
            serde_json::from_str::<ClientFrame>(r#"{"id":1,"request":{"type":"format_disk"}}"#)
                .is_err()
        );
    }
}
