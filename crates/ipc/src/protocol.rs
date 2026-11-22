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
pub const PROTOCOL_MINOR: u16 = 13;

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

/// The text of a chat message. A plain JSON string on the wire, but
/// `Debug` prints only its length, so it cannot end up in logs (ADR 0024).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChatText(String);

impl ChatText {
    /// Wraps a text.
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ChatText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChatText(<{} chars>)", self.0.chars().count())
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
    /// Start an installed game (ADR 0016). Added in protocol 1.2.
    Launch {
        /// The app.
        app_id: u32,
        /// Library folder (`None`: Reborn's own library).
        library: Option<PathBuf>,
        /// Launch entry index (`None`: the default entry for this computer).
        option: Option<u32>,
        /// The user's answer to a [`Response::CloudConflict`]: which
        /// version to play with for every true conflict found at this
        /// launch (ADR 0055); the other stays as a backup. `None`: no
        /// answer yet, so a true conflict refuses a shim-mode launch.
        /// Ignored outside shim mode. Added in protocol 1.13.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cloud_choice: Option<CloudChoice>,
    },
    /// Remove an installed app: its game folder, download leftovers and
    /// appmanifest (ADR 0018). Permanent. Refused while the app is running,
    /// installing or verifying. Added in protocol 1.3.
    Uninstall {
        /// The app.
        app_id: u32,
        /// Library folder (`None`: Reborn's own library).
        library: Option<PathBuf>,
    },
    /// The current settings (ADR 0019). Added in protocol 1.4.
    GetSettings,
    /// Set the library folder for new installs (`None`: the built-in
    /// folder). Refused while downloads are queued or running.
    SetLibrary {
        /// Absolute folder; created if missing.
        library: Option<PathBuf>,
    },
    /// Choose the Proton build by name (`None`: automatic choice).
    SetProton {
        /// One of [`Response::Settings`]'s `protons`.
        name: Option<String>,
    },
    /// The friends list with names and status (ADR 0020). Signs in if
    /// needed. Added in protocol 1.5.
    Friends,
    /// Set the online status Reborn applies after every sign-in (`None`:
    /// Reborn does not set one), and apply it now (ADR 0021). Added in
    /// protocol 1.6.
    SetOwnStatus {
        /// The status.
        status: Option<OwnStatus>,
    },
    /// Send a friend request (ADR 0022). Added in protocol 1.7.
    AddFriend {
        /// SteamID64, profile URL `…/profiles/<SteamID64>` or friend code.
        friend: String,
    },
    /// Accept a friend request the person sent.
    AcceptFriend {
        /// SteamID64 as a decimal string.
        steam_id: String,
    },
    /// Decline a friend request the person sent.
    DeclineFriend {
        /// SteamID64 as a decimal string.
        steam_id: String,
    },
    /// The recent messages with a friend (ADR 0024). Added in protocol 1.8.
    ChatHistory {
        /// The friend's SteamID64 as a decimal string.
        steam_id: String,
    },
    /// Send a message to a friend (visible to them).
    SendChatMessage {
        /// The friend's SteamID64 as a decimal string.
        steam_id: String,
        /// The text.
        text: ChatText,
    },
    /// Tell a friend the user is typing (ADR 0026). The daemon sends at
    /// most one notice per friend every 10 seconds. Added in protocol 1.10.
    SendTyping {
        /// The friend's SteamID64 as a decimal string.
        steam_id: String,
    },
    /// Mark a conversation as read up to a message, so other clients clear
    /// their unread count. Added in protocol 1.10.
    MarkChatRead {
        /// The friend's SteamID64 as a decimal string.
        steam_id: String,
        /// Server time of the newest message read.
        timestamp: u32,
    },
    /// Conversations with unread messages. Added in protocol 1.10.
    UnreadChats,
    /// The achievements of an app for the account (ADR 0030). Only reads.
    /// Added in protocol 1.11.
    Achievements {
        /// App ID.
        app_id: u32,
        /// Language of the texts as Steam names it (`english`, `german`,
        /// ...); `None` means English.
        #[serde(default)]
        language: Option<String>,
    },
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
        /// Games started by the daemon that are still running.
        /// Added in protocol 1.2.
        #[serde(default)]
        running_games: Vec<u32>,
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
    /// Answer to [`Request::GetSettings`], [`Request::SetLibrary`] and
    /// [`Request::SetProton`].
    Settings {
        /// The library folder in use.
        library: PathBuf,
        /// Whether that is Reborn's built-in folder (nothing set).
        library_is_builtin: bool,
        /// The chosen Proton build, if any (`None`: automatic).
        proton: Option<String>,
        /// Installed Proton builds, best automatic choice first. Empty on
        /// Windows.
        protons: Vec<String>,
        /// The online status Reborn sets, if any. Added in protocol 1.6.
        #[serde(default)]
        own_status: Option<OwnStatus>,
    },
    /// Answer to [`Request::Friends`]. Names and status may still be
    /// missing; they follow as [`Event::FriendUpdated`].
    Friends {
        /// Friends and pending requests.
        friends: Vec<FriendInfo>,
    },
    /// Answer to [`Request::ChatHistory`], oldest first.
    ChatMessages {
        /// The friend's SteamID64.
        steam_id: String,
        /// The messages.
        messages: Vec<ChatMessageInfo>,
    },
    /// Answer to [`Request::SendChatMessage`]: the message as Steam stored it.
    ChatMessageSent {
        /// The message.
        message: ChatMessageInfo,
    },
    /// Answer to [`Request::UnreadChats`].
    UnreadChats {
        /// Conversations with at least one unread message.
        chats: Vec<UnreadChatInfo>,
    },
    /// Answer to [`Request::Achievements`], in the game's order.
    Achievements {
        /// The app.
        app_id: u32,
        /// Its achievements; empty if it has none.
        achievements: Vec<AchievementInfo>,
    },
    /// Answer to [`Request::AddFriend`]: Steam accepted the request.
    FriendRequestSent {
        /// SteamID64 of the person, as a decimal string.
        steam_id: String,
        /// Their profile name, if Steam sent it.
        name: Option<String>,
    },
    /// Answer to [`Request::Launch`]: the game process was started.
    Launched {
        /// The app.
        app_id: u32,
    },
    /// Answer to [`Request::Launch`] in shim mode: both the file in
    /// Reborn's cloud folder and the cloud version changed since the
    /// version last downloaded (a true conflict, ADR 0055). Nothing was
    /// written and the game was not started. The frontend asks the user
    /// which version to play with and sends the launch again with
    /// `cloud_choice`. Added in protocol 1.13.
    CloudConflict {
        /// The app.
        app_id: u32,
        /// The files in conflict.
        files: Vec<CloudConflictFile>,
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
    /// A game started by the daemon is running.
    GameStarted {
        /// The app.
        app_id: u32,
        /// App name, if known.
        name: Option<String>,
    },
    /// The settings changed (library folder or Proton choice).
    SettingsChanged,
    /// The connection to Steam was lost or is back (ADR 0025). Added in
    /// protocol 1.9.
    SteamConnection {
        /// Whether the daemon is connected now.
        connected: bool,
        /// When not connected: whether it signs in again on its own (false
        /// when another session of the account took over, or it gave up).
        reconnecting: bool,
        /// Why it is not connected, for display.
        #[serde(default)]
        reason: Option<String>,
    },
    /// A friend's name, status, game or relationship changed, or a new
    /// request arrived.
    FriendUpdated {
        /// The friend as now known.
        friend: FriendInfo,
    },
    /// A chat message arrived, or the user wrote one in another client.
    ChatMessage {
        /// The message.
        message: ChatMessageInfo,
    },
    /// A friend is typing (ADR 0026). Steam sends no "stopped typing";
    /// frontends hide the notice after a while or when a message arrives.
    /// Added in protocol 1.10.
    FriendTyping {
        /// The friend's SteamID64 as a decimal string.
        steam_id: String,
    },
    /// A relationship ended (declined request, removed friend).
    FriendRemoved {
        /// SteamID64 as a decimal string.
        steam_id: String,
    },
    /// An app was uninstalled.
    AppUninstalled {
        /// The app.
        app_id: u32,
    },
    /// A game started by the daemon exited. Games that hand over to a
    /// launcher may "exit" while the launcher keeps running.
    GameExited {
        /// The app.
        app_id: u32,
        /// Exit code, if the process returned one.
        exit_code: Option<i32>,
    },
    /// Saves missing locally were looked for in Steam Cloud before a game
    /// started (ADR 0035). Not sent when there was nothing to do. Added in
    /// protocol 1.12.
    CloudRestore {
        /// The app.
        app_id: u32,
        /// What happened.
        result: CloudRestoreResult,
    },
}

/// Which version of conflicting cloud files to play with (ADR 0055).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudChoice {
    /// The file in Reborn's cloud folder; the cloud version is kept as a
    /// backup.
    Local,
    /// The version in Steam Cloud; the local file is kept as a backup.
    Cloud,
}

/// One file in a true cloud conflict (ADR 0055), for asking the user.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudConflictFile {
    /// The file's name in the cloud.
    pub name: String,
    /// Size of the file in Reborn's folder.
    pub local_size: u64,
    /// Its change time, seconds since 1970, if known.
    pub local_modified: Option<u64>,
    /// Size of the cloud version.
    pub cloud_size: u64,
    /// Change time in the cloud, seconds since 1970.
    pub cloud_modified: u64,
}

/// What restoring saves before a game start did (ADR 0035). The game
/// starts in every case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CloudRestoreResult {
    /// Saves were downloaded into the game's save folders.
    Restored {
        /// How many files.
        files: u32,
    },
    /// Nothing was written: local saves differ from the cloud or are not
    /// in it. The game started with the local saves.
    Conflict {
        /// For display.
        message: String,
    },
    /// Steam or a download failed, or it took too long.
    Failed {
        /// For display.
        message: String,
    },
}

/// A message of a one-to-one conversation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessageInfo {
    /// The friend the conversation is with (SteamID64 as a decimal string).
    pub steam_id: String,
    /// Whether the user wrote it.
    pub from_me: bool,
    /// The text, without BBCode.
    pub text: ChatText,
    /// Server time, seconds since 1970.
    pub timestamp: u32,
    /// Orders messages with the same timestamp.
    pub ordinal: u32,
}

/// A conversation with unread messages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnreadChatInfo {
    /// The friend's SteamID64 as a decimal string.
    pub steam_id: String,
    /// How many of the friend's messages are unread.
    pub count: u32,
}

/// An achievement with the account's unlock state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AchievementInfo {
    /// The game's internal name, unique within the game.
    pub name: String,
    /// Display name.
    pub display_name: String,
    /// Description; may be empty.
    pub description: String,
    /// Whether the game hides it until it is unlocked.
    pub hidden: bool,
    /// Unlock time in seconds since the Unix epoch, `None` while locked.
    pub unlocked_at: Option<u32>,
}

/// A friend or pending friend request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendInfo {
    /// SteamID64 as a decimal string (JSON numbers lose precision in some
    /// languages).
    pub steam_id: String,
    /// Profile name, once Steam sent it.
    pub name: Option<String>,
    /// Online status.
    pub status: FriendStatus,
    /// How the account relates to this person.
    pub relationship: FriendRelationship,
    /// The Steam app being played, if any.
    pub game_app_id: Option<u32>,
    /// Name of a non-Steam game being played, if any.
    pub game_name: Option<String>,
}

/// An online status the user can choose for their own account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnStatus {
    /// Online.
    Online,
    /// Away.
    Away,
    /// Busy.
    Busy,
    /// Invisible: appear offline but stay signed in.
    Invisible,
    /// Offline.
    Offline,
}

/// Online status of a friend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendStatus {
    /// Not known yet.
    Unknown,
    /// Offline.
    Offline,
    /// Online.
    Online,
    /// Busy.
    Busy,
    /// Away.
    Away,
    /// Snooze.
    Snooze,
    /// Looking to trade.
    LookingToTrade,
    /// Looking to play.
    LookingToPlay,
}

/// How the account relates to someone on the friends list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FriendRelationship {
    /// Friends.
    Friend,
    /// They sent a friend request.
    RequestReceived,
    /// The user sent a friend request.
    RequestSent,
    /// Blocked by the user (`Blocked`, or a friend who is blocked).
    Blocked,
    /// The user ignored their request (`EFriendRelationship::Ignored`). Steam
    /// reported this briefly after declining a request and did not keep it.
    Ignored,
    /// Anything else.
    Other,
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
    fn achievements_round_trip() {
        let request: Request =
            serde_json::from_str(r#"{"type":"achievements","app_id":480}"#).unwrap();
        assert_eq!(
            request,
            Request::Achievements {
                app_id: 480,
                language: None
            }
        );
        let response = Response::Achievements {
            app_id: 480,
            achievements: vec![AchievementInfo {
                name: "ACH_WIN_ONE_GAME".to_owned(),
                display_name: "Winner".to_owned(),
                description: "Win one game.".to_owned(),
                hidden: false,
                unlocked_at: None,
            }],
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""unlocked_at":null"#), "{json}");
        assert_eq!(serde_json::from_str::<Response>(&json).unwrap(), response);
    }

    #[test]
    fn cloud_restore_events_round_trip() {
        for result in [
            CloudRestoreResult::Restored { files: 2 },
            CloudRestoreResult::Conflict {
                message: "1 save differs".to_owned(),
            },
            CloudRestoreResult::Failed {
                message: "timed out".to_owned(),
            },
        ] {
            let event = Event::CloudRestore {
                app_id: 5_191_630,
                result,
            };
            let json = serde_json::to_string(&event).unwrap();
            assert_eq!(
                serde_json::from_str::<Event>(&json).unwrap(),
                event,
                "{json}"
            );
        }
        let json = serde_json::to_string(&Event::CloudRestore {
            app_id: 1,
            result: CloudRestoreResult::Restored { files: 1 },
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"type":"cloud_restore","app_id":1,"result":{"kind":"restored","files":1}}"#
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
    fn chat_text_is_a_string_on_the_wire_but_hidden_in_debug() {
        let request = Request::SendChatMessage {
            steam_id: "292733980074049537".to_owned(),
            text: ChatText::new("see you at eight"),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""text":"see you at eight""#), "{json}");
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);
        let printed = format!("{request:?}");
        assert!(
            !printed.contains("eight") && printed.contains("16 chars"),
            "{printed}"
        );
    }

    #[test]
    fn steam_ids_are_strings_on_the_wire() {
        let friend = FriendInfo {
            steam_id: "292733980074049537".to_owned(),
            name: Some("Someone".to_owned()),
            status: FriendStatus::LookingToPlay,
            relationship: FriendRelationship::RequestReceived,
            game_app_id: None,
            game_name: None,
        };
        let json = serde_json::to_string(&Event::FriendUpdated { friend }).unwrap();
        assert_eq!(
            json,
            r#"{"type":"friend_updated","friend":{"steam_id":"292733980074049537","name":"Someone","status":"looking_to_play","relationship":"request_received","game_app_id":null,"game_name":null}}"#
        );
    }

    #[test]
    fn launch_json() {
        let frame = ClientFrame {
            id: 2,
            request: Request::Launch {
                app_id: 480,
                library: None,
                option: None,
                cloud_choice: None,
            },
        };
        // Without a choice the frame is the same as before protocol 1.13.
        assert_eq!(
            serde_json::to_string(&frame).unwrap(),
            r#"{"id":2,"request":{"type":"launch","app_id":480,"library":null,"option":null}}"#
        );
        assert_eq!(
            serde_json::from_str::<ClientFrame>(
                r#"{"id":2,"request":{"type":"launch","app_id":480,"library":null,"option":null}}"#
            )
            .unwrap(),
            frame
        );
        assert_eq!(
            serde_json::to_string(&Event::GameExited {
                app_id: 480,
                exit_code: Some(0)
            })
            .unwrap(),
            r#"{"type":"game_exited","app_id":480,"exit_code":0}"#
        );
    }

    #[test]
    fn cloud_conflict_json() {
        let request = Request::Launch {
            app_id: 2_143_030,
            library: None,
            option: None,
            cloud_choice: Some(CloudChoice::Cloud),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(
            json,
            r#"{"type":"launch","app_id":2143030,"library":null,"option":null,"cloud_choice":"cloud"}"#
        );
        assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);
        let response = Response::CloudConflict {
            app_id: 2_143_030,
            files: vec![CloudConflictFile {
                name: "MainPlayerData.json".to_owned(),
                local_size: 44_800,
                local_modified: Some(1_763_800_000),
                cloud_size: 44_850,
                cloud_modified: 1_763_800_100,
            }],
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            r#"{"type":"cloud_conflict","app_id":2143030,"files":[{"name":"MainPlayerData.json","local_size":44800,"local_modified":1763800000,"cloud_size":44850,"cloud_modified":1763800100}]}"#
        );
        assert_eq!(serde_json::from_str::<Response>(&json).unwrap(), response);
        assert_eq!(
            serde_json::to_string(&CloudChoice::Local).unwrap(),
            r#""local""#
        );
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
