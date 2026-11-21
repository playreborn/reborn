//! Where `rebornd` listens.
//!
//! - **Windows:** named pipe `\\.\pipe\reborn-<user>`, with an access list
//!   that admits only the current user (ADR 0014).
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

/// Who may connect besides the file permissions.
#[derive(Clone, Debug, Default)]
pub struct Access {
    /// Windows: the SID (`S-1-5-...`) of the only user allowed to open the
    /// named pipe. Required on Windows; ignored elsewhere, where the socket
    /// file has mode `0600`.
    pub windows_user_sid: Option<String>,
}

/// Starts listening. Fails with `AddrInUse` if another daemon of this user
/// is running (on Windows possibly `PermissionDenied`).
pub fn listen(access: &Access) -> io::Result<Listener> {
    listen_at(name()?, access)
}

/// Starts listening for shims (ADR 0047), with the same access rules as
/// the frontend socket. The name comes from `reborn-shim-protocol`, so the
/// shim finds it without depending on this crate.
pub fn listen_shim(access: &Access) -> io::Result<Listener> {
    listen_at(shim_name()?, access)
}

fn shim_name() -> io::Result<Name<'static>> {
    let name = reborn_shim_protocol::socket_name();
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        name.to_ns_name::<GenericNamespaced>()
    }
    #[cfg(unix)]
    {
        use interprocess::local_socket::GenericFilePath;
        std::path::PathBuf::from(name).to_fs_name::<GenericFilePath>()
    }
}

fn listen_at(name: Name<'_>, access: &Access) -> io::Result<Listener> {
    let options = ListenerOptions::new().name(name);
    #[cfg(unix)]
    let options = {
        use interprocess::os::unix::local_socket::ListenerOptionsExt;
        let _ = access;
        options.mode(0o600)
    };
    #[cfg(windows)]
    let options = {
        use interprocess::os::windows::local_socket::ListenerOptionsExt;
        use interprocess::os::windows::security_descriptor::SecurityDescriptor;
        let sid = access.windows_user_sid.as_deref().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "the user's SID is required on Windows",
            )
        })?;
        let sddl = widestring::U16CString::from_str(user_only_sddl(sid)?)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        options.security_descriptor(SecurityDescriptor::deserialize(&sddl)?)
    };
    options.create_tokio()
}

/// A protected DACL with a single entry: full access for this SID
/// (Microsoft's "Security Descriptor String Format"). The SID is checked
/// first so nothing else can be smuggled into the string.
#[cfg_attr(not(windows), allow(dead_code))]
fn user_only_sddl(sid: &str) -> io::Result<String> {
    let valid = sid.len() > 4
        && sid.starts_with("S-1-")
        && sid[2..].chars().all(|c| c.is_ascii_digit() || c == '-')
        && !sid.ends_with('-')
        && !sid.contains("--");
    if !valid {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a SID"));
    }
    Ok(format!("D:P(A;;GA;;;{sid})"))
}

/// Connects to the daemon of the current user.
pub async fn connect() -> io::Result<LocalSocketStream> {
    LocalSocketStream::connect(name()?).await
}

/// The Tokio stream type of a connection.
pub type LocalSocketStream = interprocess::local_socket::tokio::Stream;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_user_only_access_list() {
        assert_eq!(
            user_only_sddl("S-1-5-21-1-2-3-1001").unwrap(),
            "D:P(A;;GA;;;S-1-5-21-1-2-3-1001)"
        );
    }

    #[test]
    fn rejects_anything_but_a_sid() {
        for bad in [
            "",
            "S-1-",
            "WD",
            "S-1-5)(A;;GA;;;WD",
            "s-1-5-18",
            "S-1-5-18-",
            "S-1--5",
        ] {
            assert!(user_only_sddl(bad).is_err(), "{bad}");
        }
    }

    #[cfg(windows)]
    fn test_name(tag: &str) -> Name<'static> {
        use interprocess::local_socket::GenericNamespaced;
        format!("reborn-test-{tag}-{}", std::process::id())
            .to_ns_name::<GenericNamespaced>()
            .unwrap()
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn the_current_user_can_connect() {
        use interprocess::local_socket::traits::tokio::Listener as _;
        let access = Access {
            windows_user_sid: Some(reborn_platform::user::windows_sid().unwrap()),
        };
        let listener = listen_at(test_name("own"), &access).unwrap();
        let accept = tokio::spawn(async move { listener.accept().await.map(drop) });
        LocalSocketStream::connect(test_name("own")).await.unwrap();
        accept.await.unwrap().unwrap();
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn other_users_cannot_connect() {
        // Only LocalSystem may open this pipe; the test runs as a normal user.
        let access = Access {
            windows_user_sid: Some("S-1-5-18".to_owned()),
        };
        let _listener = listen_at(test_name("other"), &access).unwrap();
        let error = LocalSocketStream::connect(test_name("other"))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{error}");
    }
}
