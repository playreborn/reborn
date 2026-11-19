//! Secrets in the OS credential store.
//!
//! Entries live under the service name `reborn`, one entry per account and
//! kind, e.g. `refresh-token:<account>`. Nothing secret is ever written to a
//! file, a database or a log. If no credential store is available, the
//! functions return an error instead of falling back to insecure storage.

use reborn_cm::Secret;

const SERVICE: &str = "reborn";

/// What kind of secret an entry holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretKind {
    /// Long-lived Steam refresh token for logging in without a password.
    RefreshToken,
    /// Steam Guard machine data, so Steam Guard does not ask on every login.
    GuardData,
}

impl SecretKind {
    fn entry_name(self, account: &str) -> String {
        let prefix = match self {
            SecretKind::RefreshToken => "refresh-token",
            SecretKind::GuardData => "guard-data",
        };
        format!("{prefix}:{account}")
    }
}

/// The OS credential store failed or is not available.
#[derive(Debug, thiserror::Error)]
#[error("system credential store: {0}")]
pub struct SecretStoreError(#[from] keyring::Error);

/// Loads a secret, or `None` if none is stored.
pub fn load(kind: SecretKind, account: &str) -> Result<Option<Secret>, SecretStoreError> {
    match entry(kind, account)?.get_password() {
        Ok(value) => Ok(Some(Secret::new(value))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Stores a secret, replacing any previous value.
pub fn store(kind: SecretKind, account: &str, secret: &Secret) -> Result<(), SecretStoreError> {
    entry(kind, account)?.set_password(secret.expose())?;
    Ok(())
}

/// Deletes a secret. Returns `false` if there was none.
pub fn delete(kind: SecretKind, account: &str) -> Result<bool, SecretStoreError> {
    match entry(kind, account)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn entry(kind: SecretKind, account: &str) -> Result<keyring::Entry, SecretStoreError> {
    Ok(keyring::Entry::new(SERVICE, &kind.entry_name(account))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_separate_kinds_and_accounts() {
        assert_eq!(
            SecretKind::RefreshToken.entry_name("alice"),
            "refresh-token:alice"
        );
        assert_eq!(
            SecretKind::GuardData.entry_name("alice"),
            "guard-data:alice"
        );
    }

    /// Touches the real OS credential store, so it only runs on request:
    /// `cargo test -p reborn-platform -- --ignored`
    #[test]
    #[ignore = "uses the real OS credential store"]
    fn round_trip_in_the_os_store() {
        let account = "reborn-selftest";
        let kind = SecretKind::GuardData;
        let _ = delete(kind, account);

        assert_eq!(load(kind, account).unwrap(), None);
        store(kind, account, &Secret::new("value-1")).unwrap();
        store(kind, account, &Secret::new("value-2")).unwrap();
        assert_eq!(load(kind, account).unwrap(), Some(Secret::new("value-2")));
        assert!(delete(kind, account).unwrap());
        assert!(!delete(kind, account).unwrap());
        assert_eq!(load(kind, account).unwrap(), None);
    }
}
