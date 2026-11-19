//! App state, messages, update logic and views.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use iced::widget::{
    Column, button, column, container, progress_bar, row, rule, scrollable, space, text, text_input,
};
use iced::{Alignment, Element, Fill, Subscription, Task};
use reborn_ipc::protocol::{GuardCode, InstalledApp, Job, JobKind, JobResult, JobState, OwnedApp};
use reborn_ipc::{Client, Event, Request, Response, Secret};

use crate::daemon;

/// The two main views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Library,
    Downloads,
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
                        ..
                    }) => {
                        let first = self.stored_login.is_none();
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
            Message::QuitDone(result) => {
                // Close the window even if the daemon did not answer.
                if let Err(error) = result {
                    tracing::warn!(%error, "stopping rebornd failed");
                }
                iced::exit()
            }
        }
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
                self.sign_in = SignIn::Form;
                self.send(Request::Status, Message::StatusLoaded)
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
                },
            },
        };

        let mut page = column![self.header(), rule::horizontal(1)].spacing(12);
        if let Some(notice) = &self.notice {
            page = page.push(text(notice.clone()).style(text::warning));
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

    fn library_view(&self) -> Element<'_, Message> {
        if self.library_loading && self.owned.is_empty() {
            return center_text("Loading your library ...");
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
                        "Installed · build {} · {}",
                        installed
                            .build_id
                            .map(|build| build.to_string())
                            .unwrap_or_else(|| "?".to_owned()),
                        installed.size_on_disk.map(human_size).unwrap_or_default()
                    ),
                    row![
                        button("Update")
                            .on_press_maybe((!busy).then_some(Message::Install(app.app_id))),
                        button("Verify")
                            .style(button::secondary)
                            .on_press_maybe((!busy).then_some(Message::Verify(app.app_id))),
                    ]
                    .spacing(8)
                    .into(),
                ),
                None => (
                    "Not installed".to_owned(),
                    button("Install")
                        .on_press_maybe((!busy).then_some(Message::Install(app.app_id)))
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
