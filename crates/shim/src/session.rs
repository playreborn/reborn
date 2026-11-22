//! The session with `rebornd` (ADR 0047): connect to the shim socket once,
//! say `Hello` with this process ID, keep what the daemon answers.

use std::io::{BufRead, BufReader, Write};
use std::sync::OnceLock;

use interprocess::local_socket::Stream;
use interprocess::local_socket::prelude::*;
use reborn_shim_protocol::{
    DaemonMessage, PROTOCOL_VERSION, Request, Response, Session, ShimMessage, decode, encode,
};

use crate::log;

/// The session, asked for once; `None` if there is none.
static SESSION: OnceLock<Option<Session>> = OnceLock::new();

/// The session with `rebornd`, connecting on first use.
pub fn get() -> Option<&'static Session> {
    SESSION.get_or_init(connect).as_ref()
}

/// Sets the session without a daemon, for tests.
#[cfg(test)]
pub fn set_for_test(session: Session) {
    let _ = SESSION.set(Some(session));
}

fn connect() -> Option<Session> {
    match hello() {
        Ok(Some(session)) => {
            log(&format!("session: app {}", session.app_id));
            Some(session)
        }
        Ok(None) => None,
        Err(error) => {
            log(&format!("no session: {error}"));
            None
        }
    }
}

fn hello() -> std::io::Result<Option<Session>> {
    let stream = Stream::connect(socket_name()?)?;
    let mut reader = BufReader::new(&stream);
    let line = encode(&ShimMessage {
        id: 1,
        request: Request::Hello {
            version: PROTOCOL_VERSION,
            pid: std::process::id(),
        },
    })
    .map_err(std::io::Error::other)?;
    (&stream).write_all(line.as_bytes())?;
    let mut reply = String::new();
    reader.read_line(&mut reply)?;
    match decode::<DaemonMessage>(&reply).map_err(std::io::Error::other)? {
        DaemonMessage::Response {
            id: 1,
            response: Response::Session(session),
        } => Ok(Some(session)),
        DaemonMessage::Response {
            response: Response::Refused { reason },
            ..
        } => {
            log(&format!("rebornd refused the session: {reason:?}"));
            Ok(None)
        }
        // No event can come before the session exists.
        DaemonMessage::Response { .. } | DaemonMessage::Event { .. } => {
            Err(std::io::Error::other("unexpected answer"))
        }
    }
}

fn socket_name() -> std::io::Result<interprocess::local_socket::Name<'static>> {
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
