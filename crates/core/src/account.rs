//! Signing in and out.
//!
//! The refresh token and Steam Guard machine data live only in the OS
//! keyring (ADR 0010); the last account that signed in becomes the default.

use reborn_cm::confirmation::AuthConfirmationHandler;
use reborn_cm::{Secret, Session};
use reborn_platform::secrets::{self, SecretKind};
use reborn_platform::settings;

use crate::CoreError;

/// The given account, or the default account.
pub fn resolve_account(account: Option<String>) -> Result<String, CoreError> {
    match account.filter(|account| !account.is_empty()) {
        Some(account) => Ok(account),
        None => settings::default_account()?.ok_or(CoreError::NoAccount),
    }
}

/// Whether a login (refresh token) for the account is stored on this computer.
pub fn has_stored_login(account: &str) -> Result<bool, CoreError> {
    Ok(secrets::load(SecretKind::RefreshToken, account)?.is_some())
}

/// Signs in with the stored refresh token of the account (or the default account).
pub async fn sign_in(account: Option<String>) -> Result<Session, CoreError> {
    let account = resolve_account(account)?;
    let token = secrets::load(SecretKind::RefreshToken, &account)?
        .ok_or_else(|| CoreError::NotSignedIn(account.clone()))?;
    reborn_cm::login_with_token(&account, &token)
        .await
        .map_err(CoreError::TokenRejected)
}

/// Signs in with account name and password, then stores the refresh token
/// and new Steam Guard data in the keyring and makes the account the default.
pub async fn sign_in_with_password(
    account: &str,
    password: &Secret,
    confirmation: impl AuthConfirmationHandler,
) -> Result<Session, CoreError> {
    let guard_data = secrets::load(SecretKind::GuardData, account)?;
    let login =
        reborn_cm::login_with_password(account, password, guard_data.as_ref(), confirmation)
            .await?;
    secrets::store(SecretKind::RefreshToken, account, &login.refresh_token)?;
    if let Some(guard_data) = &login.new_guard_data {
        secrets::store(SecretKind::GuardData, account, guard_data)?;
    }
    settings::set_default_account(account)?;
    Ok(login.session)
}

/// Removes the stored login of the account. Returns `false` if there was none.
pub fn sign_out(account: &str) -> Result<bool, CoreError> {
    let removed_token = secrets::delete(SecretKind::RefreshToken, account)?;
    let removed_guard = secrets::delete(SecretKind::GuardData, account)?;
    if settings::default_account()?.as_deref() == Some(account) {
        settings::clear_default_account()?;
    }
    Ok(removed_token || removed_guard)
}
