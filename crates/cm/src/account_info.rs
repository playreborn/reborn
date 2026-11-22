//! The account's own persona name, for the shadow comparison of ADR 0056
//! (decision 2).
//!
//! Steam sends `CMsgClientAccountInfo` (EMsg 768) unasked around logon;
//! its field `persona_name` is the account's display name. Source:
//! node-steam-user `components/account.js` (MIT): the
//! `EMsg.ClientAccountInfo` handler reads `body.persona_name` into
//! `accountInfo.name`, sends no request for it, and notes "Steam appears to
//! send this twice on logon". Field and EMsg number: the generated
//! `CMsgClientAccountInfo` of `steam-vent-proto-steam` 0.5.2
//! (`steammessages_clientserver_login`, `persona_name = 1`;
//! `k_EMsgClientAccountInfo` = 768). Reborn sends nothing for it: the name
//! is only kept from what Steam sends anyway (red line 8).
//!
//! The name is account data (ADR 0056): [`PersonaName`] never prints it,
//! and nothing here logs it.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{Stream, StreamExt};
use steam_vent_proto_common::protobuf::Message;
use steam_vent_proto_steam::steammessages_clientserver_login::CMsgClientAccountInfo;
use tokio::sync::Notify;
use tracing::debug;

use crate::Session;

/// How long [`Session::persona_name`] waits for the account info when it
/// has not arrived yet.
const ACCOUNT_INFO_TIMEOUT: Duration = Duration::from_secs(20);

/// The account's persona name. `Debug` never shows it, and there is no
/// `Display`: read it with [`PersonaName::as_str`] only to compare it.
#[derive(Clone, PartialEq, Eq)]
pub struct PersonaName(String);

impl PersonaName {
    /// Wraps a name (for tests and fakes; real names come from
    /// [`Session::persona_name`]).
    pub fn new(name: String) -> Self {
        Self(name)
    }

    /// The name as Steam sent it (UTF-8).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for PersonaName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonaName(<redacted>)")
    }
}

/// The persona name of an account info message, if Steam sent one.
pub fn persona_name_from(message: &CMsgClientAccountInfo) -> Option<PersonaName> {
    message.persona_name.clone().map(PersonaName)
}

/// The latest account info of the session, filled in by a background task.
#[derive(Default)]
pub(crate) struct AccountInfoSlot {
    name: Mutex<Option<PersonaName>>,
    /// The last message, redacted, for [`Session::record_account_info`].
    redacted: Mutex<Option<Vec<u8>>>,
    ready: Notify,
}

impl AccountInfoSlot {
    pub(crate) fn store(&self, message: &CMsgClientAccountInfo) {
        let redacted = redact_account_info(message).write_to_bytes().ok();
        *self
            .name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = persona_name_from(message);
        *self
            .redacted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = redacted;
        self.ready.notify_waiters();
    }

    fn name(&self) -> Option<PersonaName> {
        self.name
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn redacted(&self) -> Option<Vec<u8>> {
        self.redacted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Follows `messages` for as long as the connection lasts; the latest
    /// one wins (node-steam-user handles every `ClientAccountInfo` the same
    /// way). `messages` must be a subscription (`on`), see `FriendsSlot`.
    pub(crate) fn follow_in_background(
        self: &Arc<Self>,
        messages: impl Stream<Item = Result<CMsgClientAccountInfo, steam_vent::NetworkError>>
        + Send
        + 'static,
    ) {
        let slot = Arc::clone(self);
        tokio::spawn(async move {
            let mut messages = Box::pin(messages);
            while let Some(message) = messages.next().await {
                match message {
                    Ok(message) => slot.store(&message),
                    // The error says nothing about the content.
                    Err(error) => debug!(%error, "unreadable account info"),
                }
            }
        });
    }
}

/// Only the field Reborn reads, with the name replaced: a recorded
/// fixture must not hold the account's name, country or flags.
fn redact_account_info(message: &CMsgClientAccountInfo) -> CMsgClientAccountInfo {
    CMsgClientAccountInfo {
        persona_name: message.persona_name.as_ref().map(|_| "Player 1".to_owned()),
        ..CMsgClientAccountInfo::default()
    }
}

impl Session {
    /// The account's persona name, from the account info Steam sent for
    /// this session (no request is sent; module docs). Waits up to 20
    /// seconds if it has not arrived yet; `None` if Steam sent none or no
    /// name.
    pub async fn persona_name(&self) -> Option<PersonaName> {
        let waiting = self.account_info.ready.notified();
        if let Some(name) = self.account_info.name() {
            return Some(name);
        }
        let _ = tokio::time::timeout(ACCOUNT_INFO_TIMEOUT, waiting).await;
        self.account_info.name()
    }

    /// The last account info of this session as a redacted test fixture
    /// (protobuf-encoded): only `persona_name`, replaced by `Player 1`.
    /// `None` if none arrived.
    pub fn record_account_info(&self) -> Option<Vec<u8>> {
        self.account_info.redacted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: Option<&str>) -> CMsgClientAccountInfo {
        CMsgClientAccountInfo {
            persona_name: name.map(str::to_owned),
            ip_country: Some("XX".to_owned()),
            account_flags: Some(7),
            ..CMsgClientAccountInfo::default()
        }
    }

    #[test]
    fn the_name_is_read_but_never_printed() {
        let name =
            persona_name_from(&info(Some("Alice Example"))).unwrap_or_else(|| panic!("no name"));
        assert_eq!(name.as_str(), "Alice Example");
        let shown = format!("{name:?} {:?}", Some(&name));
        assert!(!shown.contains("Alice"), "{shown}");
        assert_eq!(persona_name_from(&info(None)), None);
    }

    #[test]
    fn the_slot_keeps_the_latest_and_a_redacted_copy() -> Result<(), Box<dyn std::error::Error>> {
        let slot = AccountInfoSlot::default();
        assert_eq!(slot.name(), None);
        slot.store(&info(Some("Alice Example")));
        slot.store(&info(Some("Alice Renamed")));
        assert_eq!(
            slot.name().map(|name| name.as_str().to_owned()).as_deref(),
            Some("Alice Renamed")
        );
        let bytes = slot.redacted().unwrap_or_default();
        let redacted = CMsgClientAccountInfo::parse_from_bytes(&bytes)?;
        assert_eq!(redacted.persona_name.as_deref(), Some("Player 1"));
        assert_eq!(redacted.ip_country, None);
        assert_eq!(redacted.account_flags, None);
        assert!(!String::from_utf8_lossy(&bytes).contains("Alice"));
        Ok(())
    }

    #[test]
    fn the_message_kind_is_the_wire_number() {
        use steam_vent_proto_common::MsgKind;
        use steam_vent_proto_steam::enums_clientserver::EMsg;
        // `steam-vent-proto-steam` 0.5.2, `k_EMsgClientAccountInfo // 768`.
        assert_eq!(MsgKind::from(EMsg::k_EMsgClientAccountInfo).0, 768);
    }
}
