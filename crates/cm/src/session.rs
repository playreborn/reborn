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

use steam_vent::auth::AuthConfirmationHandler;
use steam_vent::{
    Connection, ConnectionError, ConnectionTrait, NetworkError, ServerDiscoveryError, ServerList,
};
use steam_vent_proto_common::MsgKind;
use steam_vent_proto_steam::enums_clientserver::EMsg;
use steam_vent_proto_steam::steammessages_clientserver::CMsgClientLicenseList;
use steam_vent_proto_steam::steammessages_clientserver_friends::CMsgClientFriendsList;
use tracing::{debug, info};

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

/// Logs in with a refresh token from an earlier [`login_with_password`].
pub async fn login_with_token(account: &str, refresh_token: &Secret) -> Result<Session, Error> {
    let servers = ServerList::discover().await?;
    debug!(account, "starting token login");
    let connection = Connection::access(&servers, account, refresh_token.expose()).await?;
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
