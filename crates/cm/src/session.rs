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
use steam_vent_proto_steam::steammessages_clientserver::CMsgClientLicenseList;
use tracing::{debug, info};

use crate::Secret;
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

/// An authenticated connection to Steam.
pub struct Session {
    pub(crate) connection: Connection,
    account: String,
    /// Licenses (packages) of the account, sent by Steam right after logon.
    pub(crate) licenses: Vec<License>,
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

    let licenses = receive_licenses(&connection).await?;
    let session = Session {
        connection,
        account: account.to_owned(),
        licenses,
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
    let licenses = receive_licenses(&connection).await?;
    Ok(Session {
        connection,
        account: account.to_owned(),
        licenses,
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
/// kept unprocessed are checked too.
async fn receive_licenses(connection: &Connection) -> Result<Vec<License>, Error> {
    // Subscribe first, so the list cannot slip through between the two checks.
    let incoming = connection.one::<CMsgClientLicenseList>();
    for raw in connection.take_unprocessed() {
        if let Ok(list) = raw.into_message::<CMsgClientLicenseList>() {
            return Ok(licenses_from(list));
        }
    }
    match tokio::time::timeout(LICENSE_LIST_TIMEOUT, incoming).await {
        Ok(list) => Ok(licenses_from(list?)),
        Err(_) => Err(Error::LicenseListTimeout),
    }
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
