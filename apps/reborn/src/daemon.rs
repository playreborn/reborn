//! Reaching `rebornd`: connecting, starting it if needed, and the event stream.

use std::sync::Arc;
use std::time::Duration;

use futures_util::SinkExt;
use iced::futures::Stream;
use reborn_ipc::{Client, ClientError, Request, Response};

use crate::app::Message;

const CLIENT_NAME: &str = concat!("reborn ", env!("CARGO_PKG_VERSION"));

/// Connects to the daemon; if none is running, starts `rebornd` from the
/// app's own folder and waits up to five seconds for it.
pub async fn connect_or_start() -> Result<Arc<Client>, String> {
    match Client::connect(CLIENT_NAME).await {
        Ok(client) => return Ok(Arc::new(client)),
        Err(ClientError::Connect(_)) => {}
        Err(error) => return Err(error.to_string()),
    }
    start_daemon()?;
    let mut last_error = String::new();
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        match Client::connect(CLIENT_NAME).await {
            Ok(client) => return Ok(Arc::new(client)),
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(format!(
        "started rebornd, but cannot reach it: {last_error}"
    ))
}

/// Starts `rebornd` from the folder of the running executable, detached and
/// without a console window.
fn start_daemon() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let name = if cfg!(windows) {
        "rebornd.exe"
    } else {
        "rebornd"
    };
    let daemon = exe
        .parent()
        .map(|dir| dir.join(name))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            format!("rebornd is not running and `{name}` was not found next to the app")
        })?;

    let mut command = std::process::Command::new(&daemon);
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// No console window for the daemon.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .spawn()
        .map(drop)
        .map_err(|error| format!("cannot start {}: {error}", daemon.display()))
}

/// Sends a request; errors (including `Response::Error`) become text.
pub async fn request(client: Arc<Client>, request: Request) -> Result<Response, String> {
    match client.request(request).await {
        Ok(Response::Error { message }) => Err(message),
        Ok(response) => Ok(response),
        Err(error) => Err(error.to_string()),
    }
}

/// Events from the daemon over a second connection. Reconnects every two
/// seconds if the daemon goes away.
pub fn events() -> impl Stream<Item = Message> {
    iced::stream::channel(100, async |mut output| {
        loop {
            if let Ok(client) =
                Client::connect(concat!("reborn ", env!("CARGO_PKG_VERSION"), " events")).await
            {
                let _ = output.send(Message::EventsConnected).await;
                while let Some(event) = client.next_event().await {
                    if output.send(Message::Event(event)).await.is_err() {
                        return;
                    }
                }
                let _ = output.send(Message::EventsLost).await;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
}
