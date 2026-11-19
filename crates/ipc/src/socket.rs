//! Where `rebornd` listens.
//!
//! - **Windows:** named pipe `\\.\pipe\reborn-<user>`. Known gap: the pipe
//!   uses the default security descriptor, which lets other local users open
//!   it for reading; a descriptor limited to the current user is planned.
//! - **Linux and other Unix:** socket file `$XDG_RUNTIME_DIR/reborn.sock`
//!   (fallback: the temp directory with the user name in the file name),
//!   mode `0600`. Abstract sockets are not used: they have no permissions.

use std::io;

use interprocess::local_socket::tokio::Listener;
use interprocess::local_socket::tokio::prelude::*;
use interprocess::local_socket::{ListenerOptions, Name};

/// The socket name of the current user.
pub fn name() -> io::Result<Name<'static>> {
    platform_name()
}

#[cfg(windows)]
fn platform_name() -> io::Result<Name<'static>> {
    use interprocess::local_socket::GenericNamespaced;
    let user = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_owned());
    let user: String = user
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("reborn-{user}").to_ns_name::<GenericNamespaced>()
}

#[cfg(unix)]
fn platform_name() -> io::Result<Name<'static>> {
    use interprocess::local_socket::GenericFilePath;
    let path = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir).join("reborn.sock"),
        _ => {
            let user = std::env::var("USER").unwrap_or_else(|_| "default".to_owned());
            std::env::temp_dir().join(format!("reborn-{user}.sock"))
        }
    };
    path.to_fs_name::<GenericFilePath>()
}

/// Starts listening. Fails with `AddrInUse` if another daemon of this user
/// is running.
pub fn listen() -> io::Result<Listener> {
    let options = ListenerOptions::new().name(name()?);
    #[cfg(unix)]
    let options = {
        use interprocess::os::unix::local_socket::ListenerOptionsExt;
        options.mode(0o600)
    };
    options.create_tokio()
}

/// Connects to the daemon of the current user.
pub async fn connect() -> io::Result<LocalSocketStream> {
    LocalSocketStream::connect(name()?).await
}

/// The Tokio stream type of a connection.
pub type LocalSocketStream = interprocess::local_socket::tokio::Stream;
