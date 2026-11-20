//! Logging in and the authenticated session.
//!
//! Flow, as implemented by `steam-vent` (`src/connection/unauthenticated.rs`,
//! `src/auth/mod.rs`, v0.5.0):
//! 1. discover CM servers through the Steam Directory Web API,
//! 2. connect over WebSocket (TLS),
//! 3. password login: `Authentication.GetPasswordRSAPublicKey`,
//!    `Authentication.BeginAuthSessionViaCredentials`, Steam Guard
//!    confirmation, then `Authentication.PollAuthSessionStatus` until Steam
//!    returns a refresh token,
//! 4. `CMsgClientLogon` with that refresh token.
//!
//! The refresh token is the long-lived credential: with it,
//! [`login_with_token`] signs in again without a password.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use steam_vent::auth::AuthConfirmationHandler;
use steam_vent::{
    Connection, ConnectionError, ConnectionTrait, NetworkError, ServerDiscoveryError, ServerList,
};
use steam_vent_proto_common::MsgKind;
use steam_vent_proto_steam::enums_clientserver::EMsg;
use steam_vent_proto_steam::steammessages_clientserver::CMsgClientLicenseList;
use steam_vent_proto_steam::steammessages_clientserver_friends::CMsgClientFriendsList;
use steam_vent_proto_steam::steammessages_clientserver_login::{
    CMsgClientHeartBeat, CMsgClientLoggedOff,
};
use tracing::{debug, info, warn};

use crate::Secret;
use crate::friends::FriendsSlot;
use crate::guard::MemoryGuardStore;
use crate::pics::License;

/// How long to wait for the license list after logon.
const LICENSE_LIST_TIMEOUT: Duration = Duration::from_secs(20);

/// Errors while talking to Steam.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// No Steam server could be found.
    #[error("could not find a Steam server: {0}")]
    Discovery(#[from] ServerDiscoveryError),
    /// Connecting or logging in failed (wrong password, rejected Steam Guard
    /// code, expired token, ...).
    #[error("login failed: {0}")]
    Login(#[from] ConnectionError),
    /// A request on an established session failed.
    #[error("request failed: {0}")]
    Network(#[from] NetworkError),
    /// Steam accepted the login but did not hand out a refresh token.
    #[error("Steam did not return a refresh token")]
    MissingRefreshToken,
    /// Steam did not send the account's license list after logon.
    #[error("Steam did not send the license list in time")]
    LicenseListTimeout,
    /// Steam did not send the friends list after logon.
    #[error("Steam did not send the friends list in time")]
    FriendsListTimeout,
    /// Steam sent no app info for this app (unknown app, no access, or only
    /// offered over HTTP, which is not supported yet).
    #[error("no app info available for app {0}")]
    AppInfoUnavailable(u32),
    /// The app info could not be read.
    #[error("app info of app {0} is malformed: {1}")]
    AppInfo(u32, String),
    /// Steam refused a request, with the EResult code it returned.
    #[error("Steam refused the {what} request (EResult {code})")]
    Refused {
        /// Which request.
        what: &'static str,
        /// The EResult code.
        code: i32,
    },
    /// A response lacks a field that is required.
    #[error("Steam's {0} response is missing data")]
    IncompleteResponse(&'static str),
}

impl Error {
    /// Whether Steam refused the login itself (wrong or revoked credentials,
    /// an unusable token), as opposed to a network problem. Only then is
    /// signing in again pointless (live 2026-11-20: a network outage was
    /// reported as a rejected login).
    pub fn is_login_rejected(&self) -> bool {
        matches!(
            self,
            Error::Login(ConnectionError::LoginError(_) | ConnectionError::AccessToken(_))
                | Error::MissingRefreshToken
        )
    }

    /// Whether the connection to Steam is gone: closed, or dropped by Steam
    /// (for example when another session of the same account signed in,
    /// live 2026-11-20). The session cannot be used any more; sign in again.
    pub fn is_connection_lost(&self) -> bool {
        matches!(
            self,
            Error::Network(NetworkError::Ws(_) | NetworkError::EOF)
                | Error::Login(ConnectionError::Network(
                    NetworkError::Ws(_) | NetworkError::EOF
                ))
        )
    }
}

/// Whether Steam's logoff result allows signing in again on our own: the
/// results node-steam-user treats as non-fatal (`components/09-logon.js`,
/// `nonFatalLogOffResults`): 0, Fail (2), NoConnection (3),
/// ServiceUnavailable (20), TryAnotherCM (48); numbers from steam-vent
/// `src/eresult.rs`. LoggedInElsewhere (6) and LogonSessionReplaced (34)
/// are not in it: reconnecting would fight the other session (ADR 0025).
pub fn logoff_allows_reconnect(eresult: i32) -> bool {
    matches!(eresult, 0 | 2 | 3 | 20 | 48)
}

/// An authenticated connection to Steam.
pub struct Session {
    pub(crate) connection: Connection,
    account: String,
    /// Licenses (packages) of the account, sent by Steam right after logon.
    pub(crate) licenses: Vec<License>,
    /// The friends list, sent by Steam after logon (ADR 0020).
    pub(crate) friends: Arc<FriendsSlot>,
}

impl Session {
    /// The account name used to log in.
    pub fn account(&self) -> &str {
        &self.account
    }

    /// Number of licenses (packages) the account holds.
    pub fn license_count(&self) -> usize {
        self.licenses.len()
    }

    /// The 64-bit SteamID of the logged-in account.
    pub fn steam_id(&self) -> u64 {
        self.connection.steam_id().into()
    }

    /// Checks that the connection is open by sending the empty
    /// `CMsgClientHeartBeat` steam-vent's own heartbeat sends
    /// (`src/connection/raw.rs`, `setup_heartbeat`). On a closed socket this
    /// fails at once without network traffic (ADR 0025).
    pub async fn probe(&self) -> Result<(), Error> {
        self.connection.send(CMsgClientHeartBeat::default()).await?;
        Ok(())
    }

    /// Resolves with the EResult when Steam logs this session off
    /// (`CMsgClientLoggedOff`, EMsg 757); see [`logoff_allows_reconnect`].
    pub fn logged_off(&self) -> impl Future<Output = Option<i32>> + Send + 'static {
        let mut messages = Box::pin(self.connection.on::<CMsgClientLoggedOff>());
        async move {
            while let Some(message) = messages.next().await {
                if let Ok(message) = message {
                    return Some(message.eresult.unwrap_or(0));
                }
            }
            None
        }
    }
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("account", &self.account)
            .field("steam_id", &self.steam_id())
            .finish_non_exhaustive()
    }
}

/// Result of a successful password login.
#[derive(Debug)]
pub struct PasswordLogin {
    /// The authenticated session.
    pub session: Session,
    /// Long-lived token for [`login_with_token`]. Store it in the OS keyring.
    pub refresh_token: Secret,
    /// New Steam Guard machine data, if Steam issued some. Store it in the
    /// OS keyring and pass it to the next password login so Steam Guard
    /// does not ask again on this machine.
    pub new_guard_data: Option<Secret>,
}

/// Logs in with account name and password.
///
/// `guard_data` is Steam Guard machine data from a previous login, if any.
/// `confirmation` is asked for a Steam Guard code or waits for approval in
/// the Steam mobile app, see [`crate::confirmation`].
pub async fn login_with_password<H: AuthConfirmationHandler>(
    account: &str,
    password: &Secret,
    guard_data: Option<&Secret>,
    confirmation: H,
) -> Result<PasswordLogin, Error> {
    let servers = ServerList::discover().await?;
    let (guard_store, received) = MemoryGuardStore::new(guard_data.map(|g| g.expose().to_owned()));

    debug!(account, "starting password login");
    let connection = Connection::login(
        &servers,
        account,
        password.expose(),
        guard_store,
        confirmation,
    )
    .await?;

    // After a password login, steam-vent keeps the refresh token as the
    // session's "access token" (it is what CMsgClientLogon expects).
    let refresh_token = connection
        .access_token()
        .map(Secret::new)
        .ok_or(Error::MissingRefreshToken)?;
    let new_guard_data = take(&received).map(Secret::new);

    let (licenses, friends) = receive_initial(&connection).await?;
    let session = Session {
        connection,
        account: account.to_owned(),
        licenses,
        friends,
    };
    info!(account, "logged in with password");
    Ok(PasswordLogin {
        session,
        refresh_token,
        new_guard_data,
    })
}

/// How many Steam servers a token login tries before giving up (ADR 0028).
const LOGIN_SERVERS: u32 = 3;

/// Whether connecting to one Steam server failed in a way another server
/// may not: the connection was reset, closed or timed out before logon.
/// Live 2026-11-20: one server reset every connection (os error 10054),
/// and steam-vent starts each new process at the first server of the list.
fn try_another_server(error: &ConnectionError) -> bool {
    matches!(
        error,
        ConnectionError::Network(
            NetworkError::IO(_) | NetworkError::Ws(_) | NetworkError::EOF | NetworkError::Timeout
        )
    )
}

/// Logs in with a refresh token from an earlier [`login_with_password`].
///
/// Tries up to three servers when a connection fails before
/// logon, a second apart. steam-vent's server list hands out its servers in
/// turn, so each attempt reaches the next one. node-steam-user also avoids
/// a failed server: it picks at random among the five least loaded and
/// skips recently failed ones (`components/09-logon.js`). Password logins
/// are not repeated, because they may ask for Steam Guard again.
pub async fn login_with_token(account: &str, refresh_token: &Secret) -> Result<Session, Error> {
    let servers = ServerList::discover().await?;
    let mut attempt = 1;
    let connection = loop {
        debug!(account, attempt, "starting token login");
        match Connection::access(&servers, account, refresh_token.expose()).await {
            Ok(connection) => break connection,
            Err(error) if attempt < LOGIN_SERVERS && try_another_server(&error) => {
                warn!(%error, attempt, "Steam server failed, trying the next one");
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(error) => return Err(error.into()),
        }
    };
    info!(account, "logged in with refresh token");
    let (licenses, friends) = receive_initial(&connection).await?;
    Ok(Session {
        connection,
        account: account.to_owned(),
        licenses,
        friends,
    })
}

fn take(slot: &Arc<Mutex<Option<String>>>) -> Option<String> {
    slot.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

/// Waits for `CMsgClientLicenseList`, which Steam sends unasked right after
/// logon (node-steam-user `components/apps.js`, `ClientLicenseList` handler).
/// It may already have arrived while logging in, so the messages steam-vent
/// kept unprocessed are checked too. The same holds for the friends list
/// (node-steam-user `components/friends.js`, `ClientFriendsList` handler),
/// which is kept for [`Session::friends`] or awaited in the background.
async fn receive_initial(
    connection: &Connection,
) -> Result<(Vec<License>, Arc<FriendsSlot>), Error> {
    // Subscribe first, so nothing can slip through between the two checks.
    let licenses_incoming = connection.one::<CMsgClientLicenseList>();
    // A subscription, not `one`: an unused one-time wait would swallow the
    // first incremental friends list later (see `FriendsSlot`).
    let friends_incoming = connection.on::<CMsgClientFriendsList>();
    let mut licenses = None;
    let friends = Arc::new(FriendsSlot::default());
    let mut friends_found = false;
    for raw in connection.take_unprocessed() {
        // `into_message` consumes the message, so look at its kind first.
        // `EMsg` variants carry no discriminants (`as i32` would give the
        // position in the enum), so compare through `MsgKind`.
        let kind = raw.kind;
        if kind == MsgKind::from(EMsg::k_EMsgClientLicenseList) {
            if let Ok(list) = raw.into_message::<CMsgClientLicenseList>() {
                licenses = Some(licenses_from(list));
            }
        } else if kind == MsgKind::from(EMsg::k_EMsgClientFriendsList)
            && let Ok(list) = raw.into_message::<CMsgClientFriendsList>()
        {
            friends.store(list);
            friends_found = true;
        }
    }
    if !friends_found {
        friends.wait_in_background(friends_incoming);
    }
    let licenses = match licenses {
        Some(licenses) => licenses,
        None => match tokio::time::timeout(LICENSE_LIST_TIMEOUT, licenses_incoming).await {
            Ok(list) => licenses_from(list?),
            Err(_) => return Err(Error::LicenseListTimeout),
        },
    };
    Ok((licenses, friends))
}

fn licenses_from(list: CMsgClientLicenseList) -> Vec<License> {
    let licenses: Vec<License> = list
        .licenses
        .into_iter()
        .filter_map(|license| {
            Some(License {
                package_id: license.package_id?,
                access_token: license.access_token.unwrap_or(0),
                flags: license.flags.unwrap_or(0),
            })
        })
        .collect();
    debug!(count = licenses.len(), "received license list");
    licenses
}

#[cfg(test)]
mod tests {
    use super::*;
    use steam_vent_proto_steam::steammessages_clientserver::cmsg_client_license_list::License as ProtoLicense;

    #[test]
    fn only_harmless_logoffs_allow_reconnecting() {
        for harmless in [0, 2, 3, 20, 48] {
            assert!(logoff_allows_reconnect(harmless), "{harmless}");
        }
        // LoggedInElsewhere, LogonSessionReplaced, AccessDenied.
        for fatal in [6, 34, 15] {
            assert!(!logoff_allows_reconnect(fatal), "{fatal}");
        }
    }

    #[test]
    fn only_real_rejections_count_as_rejected_logins() {
        let rejected = Error::Login(ConnectionError::LoginError(
            steam_vent::LoginError::InvalidCredentials,
        ));
        assert!(rejected.is_login_rejected());
        let offline = Error::Login(ConnectionError::Network(NetworkError::Timeout));
        assert!(!offline.is_login_rejected());
        assert!(!Error::Network(NetworkError::EOF).is_login_rejected());
        assert!(!Error::LicenseListTimeout.is_login_rejected());
    }

    #[test]
    fn only_connection_failures_try_another_server() {
        let reset = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
        assert!(try_another_server(&ConnectionError::Network(
            NetworkError::IO(reset)
        )));
        assert!(try_another_server(&ConnectionError::Network(
            NetworkError::Timeout
        )));
        assert!(!try_another_server(&ConnectionError::LoginError(
            steam_vent::LoginError::InvalidCredentials
        )));
    }

    #[test]
    fn lost_connections_are_recognized() {
        assert!(Error::Network(NetworkError::EOF).is_connection_lost());
        assert!(!Error::Network(NetworkError::Timeout).is_connection_lost());
        assert!(!Error::LicenseListTimeout.is_connection_lost());
    }

    #[test]
    fn message_kinds_are_the_wire_numbers() {
        // Regression: `EMsg::... as i32` is the enum position, not the EMsg
        // number, so login messages were not recognized (live, 2026-11-20).
        // Numbers: SteamKit2 `enums.steamd` (EMsg ClientLicenseList 780,
        // ClientFriendsList 767).
        assert_eq!(MsgKind::from(EMsg::k_EMsgClientLicenseList).0, 780);
        assert_eq!(MsgKind::from(EMsg::k_EMsgClientFriendsList).0, 767);
    }

    #[test]
    fn converts_the_license_list() {
        let list = CMsgClientLicenseList {
            licenses: vec![
                ProtoLicense {
                    package_id: Some(0),
                    ..ProtoLicense::default()
                },
                ProtoLicense {
                    package_id: Some(12345),
                    access_token: Some(987),
                    flags: Some(8),
                    ..ProtoLicense::default()
                },
                ProtoLicense::default(),
            ],
            ..CMsgClientLicenseList::default()
        };
        assert_eq!(
            licenses_from(list),
            [
                License {
                    package_id: 0,
                    access_token: 0,
                    flags: 0
                },
                License {
                    package_id: 12345,
                    access_token: 987,
                    flags: 8
                },
            ]
        );
    }
}
