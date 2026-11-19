//! Client for Steam's Connection Manager (CM) protocol.
//!
//! Reborn talks to Steam through this crate only. Internally it currently
//! uses [`steam-vent`](https://codeberg.org/steam-vent/steam-vent) (MIT) for
//! the wire protocol: CM server discovery, the WebSocket transport over TLS,
//! the `IAuthenticationService` login flow and service method calls
//! (see `docs/adr/0007-steam-vent-as-cm-base.md`). The public API of this
//! crate is Reborn's own, so the backend can be replaced later.
//!
//! Supported so far:
//! - login with account name, password and Steam Guard
//!   ([`login_with_password`]),
//! - login with a stored refresh token ([`login_with_token`]),
//! - listing the owned apps from the account's licenses via PICS
//!   ([`Session::owned_apps`]),
//! - app details with depots ([`Session::app_details`]),
//! - depot keys, manifest request codes and content servers
//!   ([`Session::depot_key`], [`Session::manifest_request_code`],
//!   [`Session::content_servers`]).
//!
//! Not yet: QR-code login (needs upstream support in `steam-vent`).
#![forbid(unsafe_code)]

mod apps;
mod content;
mod guard;
mod library;
mod pics;
mod secret;
mod session;

pub use apps::{AppDetails, DepotInfo, PUBLIC_BRANCH, Platform};
pub use content::{ContentServer, DepotDecryptionKey};
pub use library::OwnedApp;
pub use secret::Secret;
pub use session::{Error, PasswordLogin, Session, login_with_password, login_with_token};

/// Ways to confirm a login with Steam Guard.
///
/// These are re-exported from `steam-vent` for now; frontends combine them
/// with [`AuthConfirmationHandler::or`](confirmation::AuthConfirmationHandler::or).
pub mod confirmation {
    pub use steam_vent::auth::{
        AuthConfirmationHandler, ConsoleAuthConfirmationHandler, DeviceConfirmationHandler,
    };
}
