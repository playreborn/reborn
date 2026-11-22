//! App state, messages, update logic and views.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::widget::{
    Column, button, column, container, pick_list, progress_bar, row, rule, scrollable, space, text,
    text_input,
};
use iced::{Alignment, Element, Fill, Subscription, Task};
use reborn_ipc::protocol::{
    AchievementInfo, ChatMessageInfo, CloudAnswer, CloudChoice, CloudConflictFile,
    CloudRestoreResult, FriendInfo, FriendRelationship, FriendStatus, GuardCode, InstalledApp, Job,
    JobKind, JobResult, JobState, OwnStatus, OwnedApp,
};
use reborn_ipc::{ChatText, Client, Event, Request, Response, Secret};

use crate::daemon;

/// The two main views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Library,
    Downloads,
    Friends,
    Settings,
}

/// How long "is typing" stays after a friend's typing notice; Steam sends
/// no "stopped typing" (ADR 0026).
const TYPING_SHOWN: Duration = Duration::from_secs(15);

/// Least time between two typing requests to the daemon (it spaces them out
/// to every 10 seconds per friend anyway).
const TYPING_RESEND: Duration = Duration::from_secs(10);

/// Label of the automatic Proton choice in the settings.
const AUTOMATIC_PROTON: &str = "Automatic (newest installed)";

/// The daemon's settings as shown in the settings tab.
#[derive(Clone, Debug, Default)]
struct SettingsView {
    library: PathBuf,
    library_is_builtin: bool,
    proton: Option<String>,
    protons: Vec<String>,
    own_status: Option<OwnStatus>,
}

/// The choices for the own online status (ADR 0021).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusChoice(Option<OwnStatus>);

impl StatusChoice {
    const ALL: [StatusChoice; 6] = [
        StatusChoice(None),
        StatusChoice(Some(OwnStatus::Online)),
        StatusChoice(Some(OwnStatus::Away)),
        StatusChoice(Some(OwnStatus::Busy)),
        StatusChoice(Some(OwnStatus::Invisible)),
        StatusChoice(Some(OwnStatus::Offline)),
    ];
}

impl std::fmt::Display for StatusChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            None => "Not set by Reborn",
            Some(OwnStatus::Online) => "Online",
            Some(OwnStatus::Away) => "Away",
            Some(OwnStatus::Busy) => "Busy",
            Some(OwnStatus::Invisible) => "Invisible",
            Some(OwnStatus::Offline) => "Offline",
        })
    }
}

/// Connection to the daemon.
#[derive(Clone, Debug)]
enum Connection {
    Connecting,
    Connected { version: String },
    Failed(String),
}

/// Where the sign-in stands (ADR 0014).
#[derive(Clone, Debug, PartialEq, Eq)]
enum SignIn {
    /// Waiting for account name and password.
    Form,
    /// Password sent; Steam has not asked for anything yet.
    Signing,
    /// Steam Guard needs a code or approval in the mobile app.
    Guard {
        code: Option<GuardCode>,
        confirm_in_app: bool,
        hint: String,
    },
    /// The code was sent; waiting for Steam.
    CodeSent,
}

#[derive(Clone, Debug)]
pub enum Message {
    Connected(Result<Arc<Client>, String>),
    Retry,
    EventsConnected,
    EventsLost,
    Event(Event),
    StatusLoaded(Result<Response, String>),
    LibraryLoaded(Result<Response, String>),
    InstalledLoaded(Result<Response, String>),
    JobsLoaded(Result<Response, String>),
    RequestDone(Result<Response, String>),
    Refresh,
    SwitchTab(Tab),
    Filter(String),
    Install(u32),
    Verify(u32),
    Cancel(u64),
    AccountInput(String),
    PasswordInput(Secret),
    CodeInput(Secret),
    SignIn,
    SignInDone(Result<Response, String>),
    SubmitCode,
    CodeDone(Result<Response, String>),
    CancelSignIn,
    SignOut,
    SignOutDone(Result<Response, String>),
    Quit,
    QuitDone(Result<Response, String>),
    Play(u32),
    /// The user answered a cloud conflict (ADR 0055): start again with this
    /// version; `None` cancels.
    CloudChoice(u32, Option<CloudChoice>),
    PlayDone(u32, Result<Response, String>),
    Uninstall(u32),
    UninstallDone(Result<Response, String>),
    SettingsLoaded(Result<Response, String>),
    FriendsLoaded(Result<Response, String>),
    OwnStatusPicked(StatusChoice),
    AddFriendInput(String),
    SendFriendRequest,
    FriendRequestDone(Result<Response, String>),
    AcceptFriend(String),
    OpenChat(String),
    CloseChat,
    ChatLoaded(Result<Response, String>),
    ChatInput(ChatText),
    SendChat,
    ChatSent(Result<Response, String>),
    UnreadLoaded(Result<Response, String>),
    /// Answer to a typing or read notice; failures only matter in the log.
    ChatNoticeDone(Result<Response, String>),
    /// A typing notice may have run out.
    TypingExpired(String),
    FriendActionDone(Result<Response, String>),
    LibraryInput(String),
    SaveLibrary,
    ResetLibrary,
    ProtonPicked(String),
    /// Open the achievements of an app (ADR 0030).
    OpenAchievements(u32),
    CloseAchievements,
    AchievementsLoaded(Result<Response, String>),
}

pub struct App {
    client: Option<Arc<Client>>,
    connection: Connection,
    tab: Tab,
    account: Option<String>,
    /// `None` until the first status arrived.
    stored_login: Option<bool>,
    owned: Vec<OwnedApp>,
    library_loading: bool,
    installed: HashMap<u32, InstalledApp>,
    library_path: Option<PathBuf>,
    jobs: Vec<Job>,
    filter: String,
    notice: Option<String>,
    sign_in: SignIn,
    account_input: String,
    /// Cleared as soon as the sign-in request is sent.
    password_input: String,
    code_input: String,
    sign_in_error: Option<String>,
    /// The first click on "Quit" while downloads run only asks for a second.
    quit_armed: bool,
    /// Games started through the daemon that are still running.
    running: BTreeSet<u32>,
    /// Games whose launch request is on its way.
    starting: BTreeSet<u32>,
    /// The app whose "Uninstall" was clicked once; a second click deletes.
    confirm_uninstall: Option<u32>,
    /// A shim-mode launch refused by a true cloud conflict, waiting for the
    /// user to pick a version (ADR 0055).
    cloud_conflict: Option<(u32, Vec<CloudConflictFile>)>,
    settings: Option<SettingsView>,
    library_input: String,
    /// `None` until loaded.
    friends: Option<Vec<FriendInfo>>,
    add_friend_input: String,
    /// The friend whose conversation is open (SteamID64).
    chat_with: Option<String>,
    /// Loaded conversations by friend, oldest first.
    chats: HashMap<String, Vec<ChatMessageInfo>>,
    /// Messages received while their conversation was not open.
    unread: HashMap<String, usize>,
    chat_input: String,
    sending_chat: bool,
    /// When each friend's last typing notice arrived.
    typing: HashMap<String, Instant>,
    /// The friend and time of the last typing notice sent.
    typing_sent: Option<(String, Instant)>,
    /// The app whose achievements are open.
    achievements_for: Option<u32>,
    /// Its achievements, or why they could not be loaded; `None` while
    /// loading.
    achievements: Option<Result<Vec<AchievementInfo>, String>>,
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let app = Self {
            client: None,
            connection: Connection::Connecting,
            tab: Tab::Library,
            account: None,
            stored_login: None,
            owned: Vec::new(),
            library_loading: false,
            installed: HashMap::new(),
            library_path: None,
            jobs: Vec::new(),
            filter: String::new(),
            notice: None,
            sign_in: SignIn::Form,
            account_input: String::new(),
            password_input: String::new(),
            code_input: String::new(),
            sign_in_error: None,
            quit_armed: false,
            running: BTreeSet::new(),
            starting: BTreeSet::new(),
            confirm_uninstall: None,
            cloud_conflict: None,
            settings: None,
            library_input: String::new(),
            friends: None,
            add_friend_input: String::new(),
            chat_with: None,
            chats: HashMap::new(),
            unread: HashMap::new(),
            chat_input: String::new(),
            sending_chat: false,
            typing: HashMap::new(),
            typing_sent: None,
            achievements_for: None,
            achievements: None,
        };
        (
            app,
            Task::perform(daemon::connect_or_start(), Message::Connected),
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        if self.client.is_some() {
            Subscription::run(daemon::events)
        } else {
            Subscription::none()
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Connected(Ok(client)) => {
                self.connection = Connection::Connected {
                    version: client.daemon_version().to_owned(),
                };
                self.client = Some(client);
                self.notice = None;
                // The library is loaded once the status says a login exists.
                Task::batch([
                    self.send(Request::Status, Message::StatusLoaded),
                    self.send(
                        Request::Installed { library: None },
                        Message::InstalledLoaded,
                    ),
                    self.send(Request::Jobs, Message::JobsLoaded),
                    self.send(Request::GetSettings, Message::SettingsLoaded),
                ])
            }
            Message::Connected(Err(error)) => {
                self.connection = Connection::Failed(error);
                Task::none()
            }
            Message::Retry => {
                self.connection = Connection::Connecting;
                Task::perform(daemon::connect_or_start(), Message::Connected)
            }
            Message::EventsConnected => Task::none(),
            Message::EventsLost => {
                self.connection = Connection::Failed("lost the connection to rebornd".to_owned());
                self.client = None;
                Task::none()
            }
            Message::Event(event) => self.on_event(event),
            Message::StatusLoaded(result) => {
                match result {
                    Ok(Response::Status {
                        account,
                        stored_login,
                        running_games,
                        ..
                    }) => {
                        let first = self.stored_login.is_none();
                        self.running = running_games.into_iter().collect();
                        self.account = account;
                        self.stored_login = Some(stored_login);
                        if stored_login && self.account_input.is_empty() {
                            self.account_input = self.account.clone().unwrap_or_default();
                        }
                        if stored_login && first {
                            return self.load_library();
                        }
                    }
                    Ok(_) => {}
                    Err(error) => self.notice = Some(error),
                }
                Task::none()
            }
            Message::LibraryLoaded(result) => {
                self.library_loading = false;
                match result {
                    Ok(Response::Library { mut apps }) => {
                        apps.sort_by_cached_key(|app| app.name.to_lowercase());
                        self.owned = apps;
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.notice = Some(format!(
                            "Library: {error}. If your login expired, sign out and sign in again."
                        ));
                    }
                }
                Task::none()
            }
            Message::InstalledLoaded(result) => {
                match result {
                    Ok(Response::Installed { library, apps }) => {
                        self.library_path = Some(library);
                        self.installed = apps.into_iter().map(|app| (app.app_id, app)).collect();
                    }
                    Ok(_) => {}
                    Err(error) => self.notice = Some(error),
                }
                Task::none()
            }
            Message::JobsLoaded(result) => {
                match result {
                    Ok(Response::Jobs { jobs }) => self.jobs = jobs,
                    Ok(_) => {}
                    Err(error) => self.notice = Some(error),
                }
                Task::none()
            }
            Message::RequestDone(result) => {
                if let Err(error) = result {
                    self.notice = Some(error);
                }
                self.send(Request::Jobs, Message::JobsLoaded)
            }
            Message::Refresh => Task::batch([
                self.load_library(),
                self.send(
                    Request::Installed { library: None },
                    Message::InstalledLoaded,
                ),
                self.send(Request::Jobs, Message::JobsLoaded),
            ]),
            Message::SwitchTab(tab) => {
                self.tab = tab;
                if tab == Tab::Friends && self.friends.is_none() {
                    return self.send(Request::Friends, Message::FriendsLoaded);
                }
                Task::none()
            }
            Message::AddFriendInput(input) => {
                self.add_friend_input = input;
                Task::none()
            }
            Message::SendFriendRequest => {
                let friend = self.add_friend_input.trim().to_owned();
                if friend.is_empty() {
                    return Task::none();
                }
                self.send(Request::AddFriend { friend }, Message::FriendRequestDone)
            }
            Message::FriendRequestDone(result) => {
                match result {
                    Ok(Response::FriendRequestSent { name, .. }) => {
                        self.add_friend_input.clear();
                        self.notice = Some(format!(
                            "Friend request sent to {}.",
                            name.as_deref().unwrap_or("that account")
                        ));
                    }
                    Ok(_) => {}
                    Err(error) => self.notice = Some(format!("Friend request: {error}")),
                }
                Task::none()
            }
            Message::OpenChat(steam_id) => {
                self.unread.remove(&steam_id);
                self.chat_with = Some(steam_id.clone());
                if self.chats.contains_key(&steam_id) {
                    return self.mark_read(&steam_id);
                }
                self.send(Request::ChatHistory { steam_id }, Message::ChatLoaded)
            }
            Message::CloseChat => {
                self.chat_with = None;
                Task::none()
            }
            Message::OpenAchievements(app_id) => {
                self.achievements_for = Some(app_id);
                self.achievements = None;
                self.send(
                    Request::Achievements {
                        app_id,
                        language: None,
                    },
                    Message::AchievementsLoaded,
                )
            }
            Message::CloseAchievements => {
                self.achievements_for = None;
                self.achievements = None;
                Task::none()
            }
            Message::AchievementsLoaded(result) => {
                match result {
                    Ok(Response::Achievements {
                        app_id,
                        achievements,
                    }) if self.achievements_for == Some(app_id) => {
                        self.achievements = Some(Ok(achievements));
                    }
                    Ok(_) => {}
                    // Errors carry no app ID; only show one while loading.
                    Err(error)
                        if self.achievements_for.is_some() && self.achievements.is_none() =>
                    {
                        self.achievements = Some(Err(error));
                    }
                    Err(_) => {}
                }
                Task::none()
            }
            Message::ChatLoaded(result) => {
                match result {
                    Ok(Response::ChatMessages { steam_id, messages }) => {
                        // Keep messages that arrived while the history loaded.
                        let mut all = messages;
                        for message in self.chats.remove(&steam_id).unwrap_or_default() {
                            add_chat_message(&mut all, message);
                        }
                        self.chats.insert(steam_id.clone(), all);
                        if self.chat_with.as_deref() == Some(steam_id.as_str()) {
                            return self.mark_read(&steam_id);
                        }
                    }
                    Ok(_) => {}
                    Err(error) => self.notice = Some(format!("Chat: {error}")),
                }
                Task::none()
            }
            Message::ChatInput(text) => {
                self.chat_input = text.as_str().to_owned();
                let Some(steam_id) = self.chat_with.clone() else {
                    return Task::none();
                };
                if self.chat_input.trim().is_empty() {
                    return Task::none();
                }
                // The daemon spaces notices out too; this saves requests.
                let now = Instant::now();
                let due = self.typing_sent.as_ref().is_none_or(|(friend, at)| {
                    *friend != steam_id || now.duration_since(*at) >= TYPING_RESEND
                });
                if !due {
                    return Task::none();
                }
                self.typing_sent = Some((steam_id.clone(), now));
                self.send(Request::SendTyping { steam_id }, Message::ChatNoticeDone)
            }
            Message::UnreadLoaded(result) => {
                if let Ok(Response::UnreadChats { chats }) = result {
                    self.unread = chats
                        .into_iter()
                        .filter(|chat| self.chat_with.as_deref() != Some(chat.steam_id.as_str()))
                        .map(|chat| (chat.steam_id, chat.count as usize))
                        .collect();
                }
                Task::none()
            }
            Message::ChatNoticeDone(result) => {
                if let Err(error) = result {
                    tracing::warn!(%error, "chat notice failed");
                }
                Task::none()
            }
            Message::TypingExpired(steam_id) => {
                if self
                    .typing
                    .get(&steam_id)
                    .is_some_and(|at| at.elapsed() >= TYPING_SHOWN)
                {
                    self.typing.remove(&steam_id);
                }
                Task::none()
            }
            Message::SendChat => {
                let Some(steam_id) = self.chat_with.clone() else {
                    return Task::none();
                };
                if self.chat_input.trim().is_empty() || self.sending_chat {
                    return Task::none();
                }
                self.sending_chat = true;
                let text = ChatText::new(std::mem::take(&mut self.chat_input));
                self.send(
                    Request::SendChatMessage { steam_id, text },
                    Message::ChatSent,
                )
            }
            Message::ChatSent(result) => {
                self.sending_chat = false;
                match result {
                    Ok(Response::ChatMessageSent { message }) => {
                        let chat = self.chats.entry(message.steam_id.clone()).or_default();
                        add_chat_message(chat, message);
                    }
                    Ok(_) => {}
                    Err(error) => self.notice = Some(format!("Message not sent: {error}")),
                }
                Task::none()
            }
            Message::AcceptFriend(steam_id) => self.send(
                Request::AcceptFriend { steam_id },
                Message::FriendActionDone,
            ),
            Message::FriendActionDone(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("Friend request: {error}"));
                }
                Task::none()
            }
            Message::OwnStatusPicked(StatusChoice(status)) => {
                self.send(Request::SetOwnStatus { status }, Message::SettingsLoaded)
            }
            Message::FriendsLoaded(result) => {
                match result {
                    Ok(Response::Friends { friends }) => {
                        self.friends = Some(friends);
                        return self.send(Request::UnreadChats, Message::UnreadLoaded);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.friends = Some(Vec::new());
                        self.notice = Some(format!("Friends: {error}"));
                    }
                }
                Task::none()
            }
            Message::Filter(filter) => {
                self.filter = filter;
                Task::none()
            }
            Message::Install(app_id) => self.queue(app_id, false),
            Message::Verify(app_id) => self.queue(app_id, true),
            Message::Cancel(job_id) => self.send(Request::Cancel { job_id }, Message::RequestDone),
            Message::AccountInput(account) => {
                self.account_input = account;
                Task::none()
            }
            Message::PasswordInput(password) => {
                password.expose().clone_into(&mut self.password_input);
                Task::none()
            }
            Message::CodeInput(code) => {
                code.expose().clone_into(&mut self.code_input);
                Task::none()
            }
            Message::SignIn => {
                if !self.can_sign_in() {
                    return Task::none();
                }
                self.sign_in = SignIn::Signing;
                self.sign_in_error = None;
                let request = Request::SignIn {
                    account: self.account_input.trim().to_owned(),
                    password: Secret::new(std::mem::take(&mut self.password_input)),
                };
                self.send(request, Message::SignInDone)
            }
            Message::SignInDone(result) => {
                self.code_input.clear();
                match result {
                    Ok(Response::SignedIn { account }) => {
                        self.sign_in = SignIn::Form;
                        self.account = Some(account);
                        self.stored_login = Some(true);
                        self.notice = None;
                        return self.load_library();
                    }
                    Ok(_) => self.sign_in = SignIn::Form,
                    Err(error) => {
                        self.sign_in = SignIn::Form;
                        self.sign_in_error = Some(error);
                    }
                }
                Task::none()
            }
            Message::SubmitCode => {
                if self.code_input.trim().is_empty() {
                    return Task::none();
                }
                self.sign_in = SignIn::CodeSent;
                let code = Secret::new(std::mem::take(&mut self.code_input));
                self.send(Request::SubmitGuardCode { code }, Message::CodeDone)
            }
            Message::CodeDone(result) => {
                if let Err(error) = result {
                    self.sign_in_error = Some(error);
                }
                Task::none()
            }
            Message::CancelSignIn => self.send(Request::CancelSignIn, Message::CodeDone),
            Message::SignOut => self.send(Request::SignOut, Message::SignOutDone),
            Message::SignOutDone(result) => {
                if let Err(error) = result {
                    self.notice = Some(error);
                }
                Task::none()
            }
            Message::Quit => {
                if self.active_jobs() > 0 && !self.quit_armed {
                    self.quit_armed = true;
                    self.notice = Some(
                        "Downloads are running. Click Quit again to stop them and rebornd; \
                         they resume the next time you install."
                            .to_owned(),
                    );
                    return Task::none();
                }
                match &self.client {
                    Some(_) => self.send(Request::Shutdown, Message::QuitDone),
                    None => iced::exit(),
                }
            }
            Message::Play(app_id) => self.launch(app_id, None),
            Message::CloudChoice(app_id, choice) => {
                // The answer names exactly the conflicts the user was shown;
                // any other the daemon finds is asked again.
                let shown = self.cloud_conflict.take();
                match (choice, shown) {
                    (Some(choice), Some((shown_app, files))) if shown_app == app_id => {
                        let answer = CloudAnswer::for_conflicts(choice, &files);
                        self.launch(app_id, Some(answer))
                    }
                    _ => Task::none(),
                }
            }
            Message::Uninstall(app_id) => {
                if self.confirm_uninstall != Some(app_id) {
                    self.confirm_uninstall = Some(app_id);
                    return Task::none();
                }
                self.confirm_uninstall = None;
                self.send(
                    Request::Uninstall {
                        app_id,
                        library: None,
                    },
                    Message::UninstallDone,
                )
            }
            Message::UninstallDone(result) => {
                if let Err(error) = result {
                    self.notice = Some(format!("Cannot uninstall: {error}"));
                }
                self.send(
                    Request::Installed { library: None },
                    Message::InstalledLoaded,
                )
            }
            Message::SettingsLoaded(result) => {
                match result {
                    Ok(Response::Settings {
                        library,
                        library_is_builtin,
                        proton,
                        protons,
                        own_status,
                    }) => {
                        self.library_input = library.display().to_string();
                        self.settings = Some(SettingsView {
                            library,
                            library_is_builtin,
                            proton,
                            protons,
                            own_status,
                        });
                    }
                    Ok(_) => {}
                    Err(error) => self.notice = Some(error),
                }
                Task::none()
            }
            Message::LibraryInput(input) => {
                self.library_input = input;
                Task::none()
            }
            Message::SaveLibrary => {
                let library = PathBuf::from(self.library_input.trim());
                self.send(
                    Request::SetLibrary {
                        library: Some(library),
                    },
                    Message::SettingsLoaded,
                )
            }
            Message::ResetLibrary => self.send(
                Request::SetLibrary { library: None },
                Message::SettingsLoaded,
            ),
            Message::ProtonPicked(choice) => {
                let name = (choice != AUTOMATIC_PROTON).then_some(choice);
                self.send(Request::SetProton { name }, Message::SettingsLoaded)
            }
            Message::PlayDone(app_id, result) => {
                self.starting.remove(&app_id);
                match result {
                    Ok(Response::Launched { app_id }) => {
                        self.running.insert(app_id);
                    }
                    Ok(Response::CloudConflict { app_id, files }) => {
                        self.cloud_conflict = Some((app_id, files));
                    }
                    Ok(_) => {}
                    Err(error) => self.notice = Some(format!("Cannot start the game: {error}")),
                }
                Task::none()
            }
            Message::QuitDone(result) => {
                // Close the window even if the daemon did not answer.
                if let Err(error) = result {
                    tracing::warn!(%error, "stopping rebornd failed");
                }
                iced::exit()
            }
        }
    }

    /// The name of an owned app, or `App <ID>`.
    fn app_name(&self, app_id: u32) -> String {
        self.owned
            .iter()
            .find(|app| app.app_id == app_id)
            .map(|app| app.name.clone())
            .unwrap_or_else(|| format!("App {app_id}"))
    }

    /// Asks the daemon to start a game, with the user's answer to a cloud
    /// conflict if there was one.
    fn launch(&mut self, app_id: u32, cloud_choice: Option<CloudAnswer>) -> Task<Message> {
        self.starting.insert(app_id);
        self.notice = None;
        match &self.client {
            Some(client) => Task::perform(
                daemon::request(
                    Arc::clone(client),
                    Request::Launch {
                        app_id,
                        library: None,
                        option: None,
                        cloud_choice,
                    },
                ),
                move |result| Message::PlayDone(app_id, result),
            ),
            None => Task::none(),
        }
    }

    /// The question after a true cloud conflict (ADR 0055): which version
    /// to play with; the other is kept as a backup.
    fn cloud_conflict_view(&self) -> Option<Element<'_, Message>> {
        let (app_id, files) = self.cloud_conflict.as_ref()?;
        let app_id = *app_id;
        let name = self.app_name(app_id);
        let mut question =
            column![text(cloud_conflict_question(&name, files)).style(text::warning)].spacing(6);
        for file in files {
            question = question.push(text(cloud_conflict_line(file)).size(12));
        }
        question = question.push(
            row![
                button("Play with Reborn's saves")
                    .on_press(Message::CloudChoice(app_id, Some(CloudChoice::Local))),
                button("Play with the Steam Cloud saves")
                    .on_press(Message::CloudChoice(app_id, Some(CloudChoice::Cloud))),
                button("Cancel").on_press(Message::CloudChoice(app_id, None)),
            ]
            .spacing(8),
        );
        Some(question.into())
    }

    fn can_sign_in(&self) -> bool {
        self.client.is_some()
            && !self.account_input.trim().is_empty()
            && !self.password_input.is_empty()
            && !matches!(
                self.sign_in,
                SignIn::Signing | SignIn::Guard { .. } | SignIn::CodeSent
            )
    }

    fn load_library(&mut self) -> Task<Message> {
        if self.stored_login != Some(true) {
            return Task::none();
        }
        self.library_loading = true;
        Task::batch([
            self.send(Request::Library, Message::LibraryLoaded),
            // Signing in may have happened on the daemon's side.
            self.send(Request::Status, Message::StatusLoaded),
        ])
    }

    fn queue(&mut self, app_id: u32, verify: bool) -> Task<Message> {
        self.tab = Tab::Downloads;
        self.send(
            Request::Install {
                app_id,
                library: None,
                branch: None,
                verify,
            },
            Message::RequestDone,
        )
    }

    /// Tells Steam the open conversation is read up to its newest message.
    fn mark_read(&self, steam_id: &str) -> Task<Message> {
        let Some(newest) = self.chats.get(steam_id).and_then(|chat| chat.last()) else {
            return Task::none();
        };
        self.send(
            Request::MarkChatRead {
                steam_id: steam_id.to_owned(),
                timestamp: newest.timestamp,
            },
            Message::ChatNoticeDone,
        )
    }

    /// Sends a request if connected.
    fn send(
        &self,
        request: Request,
        to_message: fn(Result<Response, String>) -> Message,
    ) -> Task<Message> {
        match &self.client {
            Some(client) => Task::perform(daemon::request(Arc::clone(client), request), to_message),
            None => Task::none(),
        }
    }

    fn on_event(&mut self, event: Event) -> Task<Message> {
        match event {
            Event::JobStarted { job_id, name, .. } => {
                if let Some(job) = self.job(job_id) {
                    job.name = Some(name);
                    job.state = JobState::Running {
                        bytes_done: 0,
                        bytes_total: 0,
                    };
                    return Task::none();
                }
                self.send(Request::Jobs, Message::JobsLoaded)
            }
            Event::JobProgress {
                job_id,
                bytes_done,
                bytes_total,
            } => {
                if let Some(job) = self.job(job_id) {
                    job.state = JobState::Running {
                        bytes_done,
                        bytes_total,
                    };
                }
                Task::none()
            }
            Event::JobFinished { job_id, result } => {
                if let JobResult::Failed { message } = &result {
                    self.notice = Some(format!("Job {job_id} failed: {message}"));
                }
                if let Some(job) = self.job(job_id) {
                    job.state = JobState::Finished { result };
                }
                if self.active_jobs() == 0 {
                    self.quit_armed = false;
                }
                // The installed list may have changed.
                self.send(
                    Request::Installed { library: None },
                    Message::InstalledLoaded,
                )
            }
            Event::SteamGuard {
                code,
                confirm_in_app,
                hint,
            } => {
                // Only the frontend that is signing in shows the prompt.
                if self.sign_in != SignIn::Form {
                    self.sign_in = SignIn::Guard {
                        code,
                        confirm_in_app,
                        hint,
                    };
                }
                Task::none()
            }
            Event::SignedIn { account } => {
                // Another frontend may have signed in.
                self.account = Some(account);
                self.stored_login = Some(true);
                if self.owned.is_empty() && !self.library_loading {
                    return self.load_library();
                }
                Task::none()
            }
            Event::SignedOut => {
                self.stored_login = Some(false);
                self.owned.clear();
                self.friends = None;
                self.chat_with = None;
                self.chats.clear();
                self.unread.clear();
                self.typing.clear();
                self.typing_sent = None;
                self.achievements_for = None;
                self.achievements = None;
                self.sign_in = SignIn::Form;
                self.send(Request::Status, Message::StatusLoaded)
            }
            Event::ChatMessage { message } => {
                let steam_id = message.steam_id.clone();
                let open = self.chat_with.as_deref() == Some(steam_id.as_str());
                if !message.from_me {
                    // The message ends the friend's typing.
                    self.typing.remove(&steam_id);
                    if !open {
                        *self.unread.entry(steam_id.clone()).or_default() += 1;
                    }
                }
                // Loaded conversations get the message; others load their
                // history (which contains it) when opened.
                if let Some(chat) = self.chats.get_mut(&steam_id) {
                    add_chat_message(chat, message);
                    if open {
                        return self.mark_read(&steam_id);
                    }
                }
                Task::none()
            }
            Event::FriendTyping { steam_id } => {
                self.typing.insert(steam_id.clone(), Instant::now());
                Task::perform(tokio::time::sleep(TYPING_SHOWN), move |()| {
                    Message::TypingExpired(steam_id.clone())
                })
            }
            Event::FriendRemoved { steam_id } => {
                if let Some(friends) = &mut self.friends {
                    friends.retain(|friend| friend.steam_id != steam_id);
                }
                Task::none()
            }
            Event::FriendUpdated { friend } => {
                if let Some(friends) = &mut self.friends {
                    match friends.iter_mut().find(|f| f.steam_id == friend.steam_id) {
                        Some(known) => *known = friend,
                        None => friends.push(friend),
                    }
                }
                Task::none()
            }
            Event::SteamConnection {
                connected,
                reconnecting,
                reason,
            } => {
                if connected {
                    self.notice = None;
                    // Statuses may have changed while offline.
                    let mut tasks = vec![self.send(Request::Status, Message::StatusLoaded)];
                    if self.friends.is_some() {
                        tasks.push(self.send(Request::Friends, Message::FriendsLoaded));
                    }
                    return Task::batch(tasks);
                }
                let reason = reason.unwrap_or_else(|| "not connected to Steam".to_owned());
                self.notice = Some(if reconnecting {
                    format!("{reason}. Reconnecting ...")
                } else {
                    format!("{reason}. Reborn signs in again on your next action.")
                });
                Task::none()
            }
            Event::SettingsChanged => Task::batch([
                self.send(Request::GetSettings, Message::SettingsLoaded),
                self.send(
                    Request::Installed { library: None },
                    Message::InstalledLoaded,
                ),
            ]),
            Event::AppUninstalled { app_id } => {
                self.installed.remove(&app_id);
                Task::none()
            }
            Event::GameStarted { app_id, .. } => {
                self.running.insert(app_id);
                Task::none()
            }
            Event::GameExited { app_id, .. } => {
                self.running.remove(&app_id);
                Task::none()
            }
            Event::CloudRestore { app_id, result } => {
                let name = self.app_name(app_id);
                self.notice = Some(cloud_restore_notice(&name, &result));
                Task::none()
            }
            Event::ShuttingDown => {
                self.connection = Connection::Failed("rebornd was stopped".to_owned());
                self.client = None;
                Task::none()
            }
            _ => Task::none(),
        }
    }

    fn job(&mut self, job_id: u64) -> Option<&mut Job> {
        self.jobs.iter_mut().find(|job| job.job_id == job_id)
    }

    fn active_jobs(&self) -> usize {
        self.jobs
            .iter()
            .filter(|job| !matches!(job.state, JobState::Finished { .. }))
            .count()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = match &self.connection {
            Connection::Connecting => center_text("Connecting to rebornd ..."),
            Connection::Failed(error) => column![
                text("Cannot reach rebornd").size(22),
                text(error.clone()),
                button("Try again").on_press(Message::Retry),
            ]
            .spacing(12)
            .into(),
            Connection::Connected { .. } => match self.stored_login {
                None => center_text("Loading ..."),
                Some(false) => self.sign_in_view(),
                Some(true) => match self.tab {
                    Tab::Library => self.library_view(),
                    Tab::Downloads => self.downloads_view(),
                    Tab::Friends => self.friends_view(),
                    Tab::Settings => self.settings_view(),
                },
            },
        };

        let mut page = column![self.header(), rule::horizontal(1)].spacing(12);
        if let Some(notice) = &self.notice {
            page = page.push(text(notice.clone()).style(text::warning));
        }
        if let Some(question) = self.cloud_conflict_view() {
            page = page.push(question);
        }
        page = page.push(body);
        if let Some(path) = &self.library_path {
            page = page.push(text(format!("Library: {}", path.display())).size(12));
        }
        container(page).padding(20).width(Fill).height(Fill).into()
    }

    fn header(&self) -> Element<'_, Message> {
        let signed_in = self.client.is_some() && self.stored_login == Some(true);
        let tab_button = |label: String, tab: Tab| {
            let button = button(text(label));
            let button = if self.tab == tab {
                button.style(button::primary)
            } else {
                button.style(button::secondary)
            };
            button.on_press_maybe(signed_in.then_some(Message::SwitchTab(tab)))
        };
        let downloads = match self.active_jobs() {
            0 => "Downloads".to_owned(),
            n => format!("Downloads ({n})"),
        };
        let status = match &self.connection {
            Connection::Connected { version } => match (&self.account, self.stored_login) {
                (Some(account), Some(true)) => format!("rebornd {version} · {account}"),
                _ => format!("rebornd {version} · not signed in"),
            },
            Connection::Connecting => "connecting ...".to_owned(),
            Connection::Failed(_) => "offline".to_owned(),
        };
        let mut header = row![
            text("Reborn").size(26),
            tab_button("Library".to_owned(), Tab::Library),
            tab_button(downloads, Tab::Downloads),
            tab_button("Friends".to_owned(), Tab::Friends),
            tab_button("Settings".to_owned(), Tab::Settings),
            space().width(Fill),
            text(status).size(14),
            button("Refresh")
                .style(button::secondary)
                .on_press_maybe(signed_in.then_some(Message::Refresh)),
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        if signed_in {
            header = header.push(
                button("Sign out")
                    .style(button::secondary)
                    .on_press(Message::SignOut),
            );
        }
        header
            .push(
                button(if self.quit_armed {
                    "Quit anyway"
                } else {
                    "Quit"
                })
                .style(button::danger)
                .on_press(Message::Quit),
            )
            .into()
    }

    fn sign_in_view(&self) -> Element<'_, Message> {
        let content: Column<'_, Message> = match &self.sign_in {
            SignIn::Form | SignIn::Signing => {
                let busy = self.sign_in == SignIn::Signing;
                let submit = (!busy).then_some(Message::SignIn);
                let mut form = column![
                    text("Sign in to Steam").size(22),
                    text(
                        "Use your Steam account name (not your profile name). \
                         The password is sent only to Steam and never stored; \
                         the login is kept in your system's keyring."
                    )
                    .size(13),
                    text_input("Account name", &self.account_input)
                        .on_input_maybe((!busy).then_some(Message::AccountInput))
                        .on_submit_maybe(submit.clone())
                        .padding(8),
                    text_input("Password", &self.password_input)
                        .secure(true)
                        .on_input_maybe(
                            (!busy).then_some(|input| Message::PasswordInput(Secret::new(input)))
                        )
                        .on_submit_maybe(submit)
                        .padding(8),
                    button(if busy { "Signing in ..." } else { "Sign in" })
                        .on_press_maybe(self.can_sign_in().then_some(Message::SignIn)),
                ]
                .spacing(12);
                if let Some(error) = &self.sign_in_error {
                    form = form.push(text(error.clone()).style(text::danger));
                }
                form
            }
            SignIn::Guard {
                code,
                confirm_in_app,
                hint,
            } => {
                let mut guard = column![text("Steam Guard").size(22)].spacing(12);
                let ask = match code {
                    Some(GuardCode::Authenticator) => Some(
                        "Enter the code from the Steam Guard authenticator on your phone."
                            .to_owned(),
                    ),
                    Some(GuardCode::Email) if hint.is_empty() => {
                        Some("Enter the code Steam sent to your email address.".to_owned())
                    }
                    Some(GuardCode::Email) => Some(format!(
                        "Enter the code Steam sent to your email address at {hint}."
                    )),
                    None => None,
                };
                if let Some(ask) = ask {
                    guard = guard.push(text(ask)).push(
                        row![
                            text_input("Code", &self.code_input)
                                .on_input(|input| Message::CodeInput(Secret::new(input)))
                                .on_submit(Message::SubmitCode)
                                .padding(8),
                            button("Submit").on_press_maybe(
                                (!self.code_input.trim().is_empty()).then_some(Message::SubmitCode)
                            ),
                        ]
                        .spacing(8),
                    );
                }
                if *confirm_in_app {
                    guard = guard.push(text(if code.is_some() {
                        "Or approve the sign-in in the Steam mobile app."
                    } else {
                        "Approve the sign-in in the Steam mobile app. Waiting for Steam ..."
                    }));
                }
                if let Some(error) = &self.sign_in_error {
                    guard = guard.push(text(error.clone()).style(text::danger));
                }
                guard.push(
                    button("Cancel")
                        .style(button::secondary)
                        .on_press(Message::CancelSignIn),
                )
            }
            SignIn::CodeSent => column![
                text("Steam Guard").size(22),
                text("Checking the code ..."),
                button("Cancel")
                    .style(button::secondary)
                    .on_press(Message::CancelSignIn),
            ]
            .spacing(12),
        };
        container(container(content).max_width(440))
            .center_x(Fill)
            .padding(40)
            .into()
    }

    fn friends_view(&self) -> Element<'_, Message> {
        if let Some(steam_id) = &self.chat_with {
            return self.chat_view(steam_id);
        }
        let status = StatusChoice(self.settings.as_ref().and_then(|s| s.own_status));
        let status_row = row![
            text("Your status:"),
            pick_list(StatusChoice::ALL, Some(status), Message::OwnStatusPicked),
            text("Visible to your friends. Reborn sets it after every sign-in.").size(12),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let add_row = row![
            text_input(
                "Add a friend: friend code or profile URL",
                &self.add_friend_input
            )
            .on_input(Message::AddFriendInput)
            .on_submit(Message::SendFriendRequest)
            .padding(8),
            button("Send request").on_press_maybe(
                (!self.add_friend_input.trim().is_empty()).then_some(Message::SendFriendRequest)
            ),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        column![status_row, add_row, self.friends_list()]
            .spacing(12)
            .into()
    }

    fn friends_list(&self) -> Element<'_, Message> {
        let Some(friends) = &self.friends else {
            return center_text("Loading your friends ...");
        };
        if friends.is_empty() {
            return center_text("No friends on this account yet.");
        }
        let mut sorted: Vec<&FriendInfo> = friends.iter().collect();
        // Playing first, then online, then the rest; by name within.
        sorted.sort_by_key(|friend| {
            let rank = match (friend.relationship, friend.status) {
                (FriendRelationship::Friend, _)
                    if friend.game_app_id.is_some() || friend.game_name.is_some() =>
                {
                    0
                }
                (FriendRelationship::Friend, FriendStatus::Offline | FriendStatus::Unknown) => 2,
                (FriendRelationship::Friend, _) => 1,
                _ => 3,
            };
            (rank, friend.name.clone().unwrap_or_default().to_lowercase())
        });
        let rows = sorted.into_iter().map(|friend| {
            let name = friend.name.clone().unwrap_or_else(|| "...".to_owned());
            let detail = match (friend.relationship, &friend.game_name, friend.game_app_id) {
                (FriendRelationship::RequestReceived, _, _) => {
                    "Sent you a friend request".to_owned()
                }
                (FriendRelationship::RequestSent, _, _) => "Friend request sent".to_owned(),
                (FriendRelationship::Blocked, _, _) => "Blocked".to_owned(),
                (FriendRelationship::Ignored, _, _) => "Request ignored".to_owned(),
                (_, Some(game), _) => format!("Playing {game}"),
                (_, None, Some(app_id)) => match self.owned.iter().find(|app| app.app_id == app_id)
                {
                    Some(app) => format!("Playing {}", app.name),
                    None => format!("Playing app {app_id}"),
                },
                _ => status_text(friend.status).to_owned(),
            };
            let online = !matches!(friend.status, FriendStatus::Offline | FriendStatus::Unknown);
            let mut detail_text = text(detail).size(12);
            if online {
                detail_text = detail_text.style(text::success);
            }
            let mut entry = row![
                column![text(name).size(16), detail_text]
                    .spacing(2)
                    .width(Fill)
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            if let Some(count) = self.unread.get(&friend.steam_id) {
                entry = entry.push(text(format!("{count} new")).style(text::primary));
            }
            if friend.relationship == FriendRelationship::Friend {
                entry = entry.push(
                    button("Chat")
                        .style(button::secondary)
                        .on_press(Message::OpenChat(friend.steam_id.clone())),
                );
            }
            // No Decline yet: on current Steam it left the sender unable to
            // send a new request (live, 2026-11-20); ignore requests in the
            // official client until this is understood.
            if friend.relationship == FriendRelationship::RequestReceived {
                entry = entry.push(
                    button("Accept")
                        .style(button::success)
                        .on_press(Message::AcceptFriend(friend.steam_id.clone())),
                );
            }
            container(entry)
                .padding(10)
                .width(Fill)
                .style(container::rounded_box)
                .into()
        });
        scrollable(Column::with_children(rows).spacing(6))
            .height(Fill)
            .into()
    }

    fn chat_view<'a>(&'a self, steam_id: &'a str) -> Element<'a, Message> {
        let name = self
            .friends
            .iter()
            .flatten()
            .find(|friend| friend.steam_id == steam_id)
            .and_then(|friend| friend.name.clone())
            .unwrap_or_else(|| "Friend".to_owned());
        let header = row![
            button("< Friends")
                .style(button::secondary)
                .on_press(Message::CloseChat),
            text(name.clone()).size(20),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let body: Element<'_, Message> = match self.chats.get(steam_id) {
            None => center_text("Loading messages ..."),
            Some(messages) if messages.is_empty() => center_text("No messages yet. Say hi!"),
            Some(messages) => {
                let rows = messages.iter().map(|message| {
                    let who = if message.from_me {
                        "You".to_owned()
                    } else {
                        name.clone()
                    };
                    let bubble = container(
                        column![
                            text(who).size(11),
                            text(message.text.as_str().to_owned()).size(15)
                        ]
                        .spacing(2),
                    )
                    .padding(8)
                    .max_width(560)
                    .style(container::rounded_box);
                    let line = if message.from_me {
                        row![space().width(Fill), bubble]
                    } else {
                        row![bubble, space().width(Fill)]
                    };
                    line.into()
                });
                scrollable(Column::with_children(rows).spacing(6))
                    .anchor_bottom()
                    .height(Fill)
                    .into()
            }
        };

        let typing: Element<'_, Message> = if self
            .typing
            .get(steam_id)
            .is_some_and(|at| at.elapsed() < TYPING_SHOWN)
        {
            text(format!("{name} is typing ...")).size(12).into()
        } else {
            space().height(12).into()
        };

        let can_send = !self.chat_input.trim().is_empty() && !self.sending_chat;
        let input = row![
            text_input("Write a message", &self.chat_input)
                .on_input(|text| Message::ChatInput(ChatText::new(text)))
                .on_submit(Message::SendChat)
                .padding(8),
            button(if self.sending_chat {
                "Sending ..."
            } else {
                "Send"
            })
            .on_press_maybe(can_send.then_some(Message::SendChat)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        column![header, body, typing, input].spacing(8).into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let Some(settings) = &self.settings else {
            return center_text("Loading settings ...");
        };
        let changed = self.library_input.trim() != settings.library.display().to_string()
            && !self.library_input.trim().is_empty();
        let mut page = column![
            text("Library folder").size(18),
            text(
                "New games are installed here. Games already installed stay in \
                 their folder; the library list shows the games of this folder."
            )
            .size(13),
            row![
                text_input("Folder for your games", &self.library_input)
                    .on_input(Message::LibraryInput)
                    .on_submit(Message::SaveLibrary)
                    .padding(8),
                button("Save").on_press_maybe(changed.then_some(Message::SaveLibrary)),
                button("Use default")
                    .style(button::secondary)
                    .on_press_maybe(
                        (!settings.library_is_builtin).then_some(Message::ResetLibrary)
                    ),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(10);

        if !settings.protons.is_empty() {
            let mut options = vec![AUTOMATIC_PROTON.to_owned()];
            options.extend(settings.protons.iter().cloned());
            let selected = settings
                .proton
                .clone()
                .unwrap_or_else(|| AUTOMATIC_PROTON.to_owned());
            page = page
                .push(rule::horizontal(1))
                .push(text("Proton").size(18))
                .push(text("Windows games run through this Proton build.").size(13));
            page = page.push(pick_list(options, Some(selected), Message::ProtonPicked));
        } else if !cfg!(windows) {
            page = page
                .push(rule::horizontal(1))
                .push(text("Proton").size(18))
                .push(
                    text(
                        "No Proton build found. Install one in the Steam client to play \
                         Windows games.",
                    )
                    .size(13),
                );
        }
        container(page).max_width(720).into()
    }

    fn library_view(&self) -> Element<'_, Message> {
        if self.library_loading && self.owned.is_empty() {
            return center_text("Loading your library ...");
        }
        if let Some(app_id) = self.achievements_for {
            return self.achievements_view(app_id);
        }
        let filter = self.filter.to_lowercase();
        let rows = self
            .owned
            .iter()
            .filter(|app| matches!(app.app_type.as_str(), "game" | "application" | "demo"))
            .filter(|app| filter.is_empty() || app.name.to_lowercase().contains(&filter))
            .map(|app| self.library_row(app));
        let list = Column::with_children(rows).spacing(6);

        column![
            text_input("Search your library", &self.filter)
                .on_input(Message::Filter)
                .padding(8),
            scrollable(list).height(Fill),
        ]
        .spacing(12)
        .into()
    }

    fn library_row<'a>(&'a self, app: &'a OwnedApp) -> Element<'a, Message> {
        let busy = self
            .jobs
            .iter()
            .any(|job| job.app_id == app.app_id && !matches!(job.state, JobState::Finished { .. }));
        let (status, actions): (String, Element<'_, Message>) =
            match self.installed.get(&app.app_id) {
                Some(installed) => (
                    format!(
                        "{} · build {} · {}",
                        if self.running.contains(&app.app_id) {
                            "Running"
                        } else {
                            "Installed"
                        },
                        installed
                            .build_id
                            .map(|build| build.to_string())
                            .unwrap_or_else(|| "?".to_owned()),
                        installed.size_on_disk.map(human_size).unwrap_or_default()
                    ),
                    row![
                        self.play_button(app.app_id, installed.fully_installed && !busy),
                        achievements_button(app.app_id),
                        button("Update")
                            .style(button::secondary)
                            .on_press_maybe((!busy).then_some(Message::Install(app.app_id))),
                        button("Verify")
                            .style(button::secondary)
                            .on_press_maybe((!busy).then_some(Message::Verify(app.app_id))),
                        self.uninstall_button(app.app_id, busy),
                    ]
                    .spacing(8)
                    .into(),
                ),
                None => (
                    "Not installed".to_owned(),
                    row![
                        achievements_button(app.app_id),
                        button("Install")
                            .on_press_maybe((!busy).then_some(Message::Install(app.app_id))),
                    ]
                    .spacing(8)
                    .into(),
                ),
            };
        let hours = f64::from(app.playtime_minutes) / 60.0;
        container(
            row![
                column![
                    text(app.name.clone()).size(16),
                    text(format!(
                        "{status} · {hours:.1} h played · app {}",
                        app.app_id
                    ))
                    .size(12),
                ]
                .width(Fill),
                actions,
            ]
            .align_y(Alignment::Center)
            .spacing(12),
        )
        .padding(10)
        .style(container::rounded_box)
        .into()
    }

    /// The achievements of one app (ADR 0030): unlocked first, newest
    /// first, then locked ones; locked hidden ones without name or
    /// description, like the official client.
    fn achievements_view(&self, app_id: u32) -> Element<'_, Message> {
        let name = self
            .owned
            .iter()
            .find(|app| app.app_id == app_id)
            .map(|app| app.name.clone())
            .unwrap_or_else(|| format!("App {app_id}"));
        let summary = match &self.achievements {
            Some(Ok(list)) if !list.is_empty() => format!(
                "{} of {} unlocked",
                list.iter().filter(|a| a.unlocked_at.is_some()).count(),
                list.len()
            ),
            _ => String::new(),
        };
        let header = row![
            button("< Library")
                .style(button::secondary)
                .on_press(Message::CloseAchievements),
            column![text(name).size(20), text(summary).size(12)].spacing(2),
        ]
        .spacing(12)
        .align_y(Alignment::Center);

        let body: Element<'_, Message> = match &self.achievements {
            None => center_text("Loading achievements ..."),
            Some(Err(error)) => center_text(error),
            Some(Ok(list)) if list.is_empty() => center_text("This game has no achievements."),
            Some(Ok(list)) => {
                let rows = achievement_order(list).into_iter().map(|achievement| {
                    let secret = achievement.hidden && achievement.unlocked_at.is_none();
                    let (title, description) = if secret {
                        (
                            "Hidden achievement".to_owned(),
                            "Details appear when you unlock it.".to_owned(),
                        )
                    } else {
                        (
                            achievement.display_name.clone(),
                            achievement.description.clone(),
                        )
                    };
                    let state = match achievement.unlocked_at {
                        Some(time) => format!("Unlocked {}", utc_time(time)),
                        None => "Locked".to_owned(),
                    };
                    let mut details = column![text(title).size(16)].spacing(2).width(Fill);
                    if !description.is_empty() {
                        details = details.push(text(description).size(12));
                    }
                    container(
                        row![details, text(state).size(12)]
                            .spacing(12)
                            .align_y(Alignment::Center),
                    )
                    .padding(10)
                    .style(container::rounded_box)
                    .into()
                });
                scrollable(Column::with_children(rows).spacing(6))
                    .height(Fill)
                    .into()
            }
        };
        column![header, body].spacing(12).into()
    }

    /// Two clicks: "Uninstall", then "Really delete?".
    fn uninstall_button(&self, app_id: u32, busy: bool) -> Element<'_, Message> {
        let confirming = self.confirm_uninstall == Some(app_id);
        let blocked = busy || self.running.contains(&app_id);
        button(if confirming {
            "Really delete?"
        } else {
            "Uninstall"
        })
        .style(if confirming {
            button::danger
        } else {
            button::secondary
        })
        .on_press_maybe((!blocked).then_some(Message::Uninstall(app_id)))
        .into()
    }

    fn play_button(&self, app_id: u32, ready: bool) -> Element<'_, Message> {
        let (label, enabled) = if self.running.contains(&app_id) {
            ("Running", false)
        } else if self.starting.contains(&app_id) {
            ("Starting ...", false)
        } else {
            ("Play", ready)
        };
        button(label)
            .style(button::success)
            .on_press_maybe(enabled.then_some(Message::Play(app_id)))
            .into()
    }

    fn downloads_view(&self) -> Element<'_, Message> {
        if self.jobs.is_empty() {
            return center_text("No downloads yet.");
        }
        let rows = self.jobs.iter().rev().map(|job| {
            let name = job
                .name
                .clone()
                .unwrap_or_else(|| format!("App {}", job.app_id));
            let kind = match job.kind {
                JobKind::Install => "Install / update",
                JobKind::Verify => "Verify",
            };
            let (state, progress, cancellable): (String, Option<f32>, bool) = match &job.state {
                JobState::Queued => ("Queued".to_owned(), None, true),
                JobState::Running {
                    bytes_done,
                    bytes_total,
                } => {
                    let fraction = if *bytes_total == 0 {
                        0.0
                    } else {
                        *bytes_done as f32 / *bytes_total as f32
                    };
                    (
                        format!("{} / {}", human_size(*bytes_done), human_size(*bytes_total)),
                        Some(fraction),
                        true,
                    )
                }
                JobState::Finished { result } => (finished_text(result), None, false),
            };
            let mut details = column![
                text(name).size(16),
                text(format!("{kind} · {state}")).size(12)
            ]
            .spacing(4)
            .width(Fill);
            if let Some(fraction) = progress {
                details = details.push(progress_bar(0.0..=1.0, fraction).girth(8));
            }
            let cancel = button("Cancel")
                .style(button::danger)
                .on_press_maybe(cancellable.then_some(Message::Cancel(job.job_id)));
            container(row![details, cancel].spacing(12).align_y(Alignment::Center))
                .padding(10)
                .style(container::rounded_box)
                .into()
        });
        scrollable(Column::with_children(rows).spacing(6))
            .height(Fill)
            .into()
    }
}

/// The question after a true cloud conflict in shim mode (ADR 0055).
fn cloud_conflict_question(name: &str, files: &[CloudConflictFile]) -> String {
    let count = match files.len() {
        1 => "1 save differs".to_owned(),
        count => format!("{count} saves differ"),
    };
    format!(
        "{name} was not started: {count} between Reborn's cloud folder and Steam Cloud, and \
         Reborn cannot tell on its own which one to keep. Reborn's cloud folder was not \
         changed. Which version do you want to play with? The other one is kept as a backup. \
         Saves made here stay on this computer; the official client does not see them."
    )
}

/// One conflicting file, with readable times.
fn cloud_conflict_line(file: &CloudConflictFile) -> String {
    let time = |seconds: u64| u32::try_from(seconds).map_or_else(|_| seconds.to_string(), utc_time);
    let how = if file.had_base {
        "both changed since the last download"
    } else {
        "no earlier download known"
    };
    format!(
        "{}: here {} bytes (changed {}), in Steam Cloud {} bytes (changed {}); {how}",
        file.name,
        file.local_size,
        file.local_modified.map_or_else(|| "?".to_owned(), time),
        file.cloud_size,
        time(file.cloud_modified)
    )
}

/// The notice for saves restored before a game start (ADR 0035).
fn cloud_restore_notice(name: &str, result: &CloudRestoreResult) -> String {
    match result {
        CloudRestoreResult::Restored { files: 1 } => {
            format!("{name}: 1 save restored from Steam Cloud.")
        }
        CloudRestoreResult::Restored { files } => {
            format!("{name}: {files} saves restored from Steam Cloud.")
        }
        CloudRestoreResult::Conflict { message } => format!(
            "{name}: Steam Cloud not applied, {message}. The game started with your local saves."
        ),
        CloudRestoreResult::Failed { message } => {
            format!("{name}: Steam Cloud saves could not be checked ({message}).")
        }
        _ => format!("{name}: Steam Cloud checked."),
    }
}

fn achievements_button(app_id: u32) -> Element<'static, Message> {
    button("Achievements")
        .style(button::secondary)
        .on_press(Message::OpenAchievements(app_id))
        .into()
}

/// Unlocked achievements first, newest first; then locked ones in the
/// game's order.
fn achievement_order(list: &[AchievementInfo]) -> Vec<&AchievementInfo> {
    let mut ordered: Vec<&AchievementInfo> = list.iter().collect();
    ordered.sort_by_key(|achievement| {
        (
            achievement.unlocked_at.is_none(),
            std::cmp::Reverse(achievement.unlocked_at),
        )
    });
    ordered
}

/// `YYYY-MM-DD HH:MM UTC` for seconds since the Unix epoch. Days to dates
/// with Howard Hinnant's `civil_from_days` (public domain,
/// <https://howardhinnant.github.io/date_algorithms.html>).
fn utc_time(seconds: u32) -> String {
    let seconds = i64::from(seconds);
    let days = seconds.div_euclid(86_400);
    let of_day = seconds.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        of_day / 3600,
        of_day % 3600 / 60
    )
}

/// Adds a message to a conversation unless it is already there (the same
/// message can arrive as history and as an event), keeping time order.
fn add_chat_message(chat: &mut Vec<ChatMessageInfo>, message: ChatMessageInfo) {
    let duplicate = chat.iter().any(|known| {
        known.timestamp == message.timestamp
            && known.ordinal == message.ordinal
            && known.from_me == message.from_me
    });
    if !duplicate {
        chat.push(message);
        chat.sort_by_key(|message| (message.timestamp, message.ordinal));
    }
}

fn status_text(status: FriendStatus) -> &'static str {
    match status {
        FriendStatus::Unknown => "...",
        FriendStatus::Offline => "Offline",
        FriendStatus::Online => "Online",
        FriendStatus::Busy => "Busy",
        FriendStatus::Away => "Away",
        FriendStatus::Snooze => "Snooze",
        FriendStatus::LookingToTrade => "Looking to trade",
        FriendStatus::LookingToPlay => "Looking to play",
    }
}

fn finished_text(result: &JobResult) -> String {
    match result {
        JobResult::UpToDate => "Up to date".to_owned(),
        JobResult::Done {
            files_written,
            files_unchanged,
            bytes_downloaded,
            ..
        } => format!(
            "Done · {files_written} files written, {files_unchanged} unchanged, {} downloaded",
            human_size(*bytes_downloaded)
        ),
        JobResult::Cancelled => "Cancelled (resumes next time)".to_owned(),
        JobResult::Failed { message } => format!("Failed: {message}"),
    }
}

fn center_text(message: &str) -> Element<'_, Message> {
    container(text(message.to_owned())).center(Fill).into()
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn achievement(name: &str, unlocked_at: Option<u32>) -> AchievementInfo {
        AchievementInfo {
            name: name.to_owned(),
            display_name: name.to_owned(),
            description: String::new(),
            hidden: false,
            unlocked_at,
        }
    }

    #[test]
    fn unlocked_achievements_come_first_newest_first() {
        let list = [
            achievement("a", None),
            achievement("b", Some(100)),
            achievement("c", None),
            achievement("d", Some(300)),
        ];
        let names: Vec<&str> = achievement_order(&list)
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(names, ["d", "b", "a", "c"]);
    }

    #[test]
    fn the_cloud_conflict_question_says_nothing_changed_and_what_is_kept() {
        let file = CloudConflictFile {
            name: "MainPlayerData.json".to_owned(),
            local_sha1: "aa".repeat(20),
            local_size: 10,
            local_modified: Some(0),
            cloud_sha1: "cc".repeat(20),
            cloud_size: 11,
            cloud_modified: 86_400,
            had_base: false,
        };
        let question = cloud_conflict_question("Cave Crawlers", std::slice::from_ref(&file));
        assert!(question.starts_with("Cave Crawlers was not started: 1 save differs"));
        assert!(question.contains("cloud folder was not changed"));
        assert!(question.contains("kept as a backup"));
        assert!(question.contains("official client does not see them"));
        let two = cloud_conflict_question("Demo", &[file.clone(), file.clone()]);
        assert!(two.contains("2 saves differ"), "{two}");
        let line = cloud_conflict_line(&file);
        assert!(line.contains("1970-01-01 00:00 UTC"), "{line}");
        assert!(line.contains("1970-01-02 00:00 UTC"), "{line}");
        assert!(line.contains("no earlier download known"), "{line}");
    }

    #[test]
    fn a_cloud_answer_names_exactly_the_shown_conflicts() {
        let file = CloudConflictFile {
            name: "a.json".to_owned(),
            local_sha1: "aa".repeat(20),
            local_size: 1,
            local_modified: None,
            cloud_sha1: "cc".repeat(20),
            cloud_size: 1,
            cloud_modified: 1,
            had_base: true,
        };
        let answer = CloudAnswer::for_conflicts(CloudChoice::Local, &[file]);
        assert_eq!(answer.files.len(), 1);
        assert_eq!(answer.files[0].cloud_sha1, "cc".repeat(20));
    }

    #[test]
    fn cloud_restore_notices_name_the_game() {
        assert_eq!(
            cloud_restore_notice("Demo", &CloudRestoreResult::Restored { files: 1 }),
            "Demo: 1 save restored from Steam Cloud."
        );
        assert_eq!(
            cloud_restore_notice("Demo", &CloudRestoreResult::Restored { files: 3 }),
            "Demo: 3 saves restored from Steam Cloud."
        );
        let conflict = cloud_restore_notice(
            "Demo",
            &CloudRestoreResult::Conflict {
                message: "1 saves differ from Steam Cloud; nothing was changed".to_owned(),
            },
        );
        assert!(conflict.contains("local saves"), "{conflict}");
    }

    #[test]
    fn formats_utc_times() {
        assert_eq!(utc_time(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_time(1_795_133_048), "2026-11-20 00:04 UTC");
    }
}
