# 0010: Credential storage

- **Status:** Accepted
- **Date:** 2026-11-19

## Context

After a password login Steam returns a long-lived refresh token, and
optionally Steam Guard machine data. Both grant access to the account and
must survive restarts. steam-vent's default `FileGuardDataStore` writes guard
data to a JSON file in the user's cache directory.

## Decision

- Refresh tokens and Steam Guard data are stored **only** in the OS
  credential store via the [`keyring`](https://crates.io/crates/keyring)
  crate (v4, `v1` feature): Windows Credential Manager, Secret Service on
  Linux, Keychain on macOS.
- Entries use the service name `reborn` and the user name
  `refresh-token:<account>` / `guard-data:<account>`.
- If no credential store is available, Reborn fails with an error and never
  falls back to files.
- Passwords are never stored; they are only sent to Steam (RSA-encrypted by
  the login flow).
- `reborn-cm` never touches storage: it receives and returns secrets as
  [`Secret`] values (redacted `Debug`, no `Display`); `reborn-platform`
  stores them.
- Non-secret settings (the default account) go to a plain file in the
  user's config directory.

## Consequences

- On Linux a running Secret Service (GNOME Keyring, KWallet) is required.
  Headless setups need one too; documented for users.
- keyring's Linux backend pulls in zbus components that RustSec marks as
  unmaintained (not vulnerable). Ignored in `deny.toml`; revisit with
  keyring updates.
- `reborn-cli logout` removes both entries.
