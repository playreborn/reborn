//! App tickets from Valve (ADR 0027): proof that the account owns an app,
//! needed later by the shim (ADR 0012). Ownership always comes from Valve's
//! servers (red line 3); Reborn never builds or alters a ticket.
//!
//! Messages and checks follow node-steam-user `components/appauth.js`
//! (MIT): `getAppOwnershipTicket` and `createEncryptedAppTicket`. Tickets
//! carry the SteamID, the public IP and the licenses, so they are treated
//! like secrets: never logged, replaced in fixtures.

use std::fmt;

use steam_vent::ConnectionTrait;
use steam_vent_proto_common::protobuf::{Message, MessageField};
use steam_vent_proto_steam::encrypted_app_ticket::EncryptedAppTicket;
use steam_vent_proto_steam::steammessages_clientserver::{
    CMsgClientGetAppOwnershipTicket, CMsgClientGetAppOwnershipTicketResponse,
    CMsgClientRequestEncryptedAppTicket, CMsgClientRequestEncryptedAppTicketResponse,
};

use crate::{Error, Session};

/// `EResult::OK`.
const RESULT_OK: i32 = 1;

/// Ticket bytes. `Debug` prints only the length, so a ticket cannot end up
/// in logs by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct Ticket(Vec<u8>);

impl Ticket {
    /// The raw bytes. Never log or print them.
    pub fn expose(&self) -> &[u8] {
        &self.0
    }

    /// Length in bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the ticket is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Ticket(<{} bytes>)", self.0.len())
    }
}

/// An encrypted app ticket: an encrypted ownership ticket plus the game's
/// own data, for the game's servers to check with their key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncryptedTicket {
    /// The `EncryptedAppTicket` message, protobuf-encoded, as games receive
    /// it.
    pub ticket: Ticket,
}

/// Redacted fixture data, see [`Session::record_tickets`].
#[derive(Clone, Debug, Default)]
pub struct TicketRecording {
    /// `CMsgClientGetAppOwnershipTicketResponse`, protobuf-encoded.
    pub ownership: Vec<u8>,
    /// `CMsgClientRequestEncryptedAppTicketResponse`, protobuf-encoded.
    pub encrypted: Vec<u8>,
}

impl Session {
    /// The app ownership ticket for `app_id`, signed by Valve. Steam
    /// refuses it for apps the account does not own.
    pub async fn app_ownership_ticket(&self, app_id: u32) -> Result<Ticket, Error> {
        ownership_ticket_from(&self.ownership_response(app_id).await?, app_id)
    }

    /// An encrypted app ticket for `app_id` with the game's `userdata`.
    /// Only apps that set up an encryption key with Valve get one.
    pub async fn encrypted_app_ticket(
        &self,
        app_id: u32,
        userdata: &[u8],
    ) -> Result<EncryptedTicket, Error> {
        encrypted_ticket_from(&self.encrypted_response(app_id, userdata).await?, app_id)
    }

    /// Asks for both tickets for `app_id` and records the answers as
    /// fixtures: ticket bytes become zeros of the same length, CRCs become
    /// zero; result codes, app IDs, versions and sizes are kept (ADR 0027).
    pub async fn record_tickets(&self, app_id: u32) -> Result<TicketRecording, Error> {
        let ownership = redact_ownership(&self.ownership_response(app_id).await?);
        let encrypted = redact_encrypted(&self.encrypted_response(app_id, &[]).await?);
        Ok(TicketRecording {
            ownership: ownership
                .write_to_bytes()
                .map_err(|_| Error::IncompleteResponse("ownership ticket"))?,
            encrypted: encrypted
                .write_to_bytes()
                .map_err(|_| Error::IncompleteResponse("encrypted app ticket"))?,
        })
    }

    async fn ownership_response(
        &self,
        app_id: u32,
    ) -> Result<CMsgClientGetAppOwnershipTicketResponse, Error> {
        let request = CMsgClientGetAppOwnershipTicket {
            app_id: Some(app_id),
            ..Default::default()
        };
        Ok(self
            .connection
            .job::<_, CMsgClientGetAppOwnershipTicketResponse>(request)
            .await?)
    }

    async fn encrypted_response(
        &self,
        app_id: u32,
        userdata: &[u8],
    ) -> Result<CMsgClientRequestEncryptedAppTicketResponse, Error> {
        let request = CMsgClientRequestEncryptedAppTicket {
            app_id: Some(app_id),
            userdata: Some(userdata.to_vec()),
            ..Default::default()
        };
        Ok(self
            .connection
            .job::<_, CMsgClientRequestEncryptedAppTicketResponse>(request)
            .await?)
    }
}

/// The ticket from Steam's answer, checked like node-steam-user does: OK,
/// the asked app, and a ticket present.
pub fn ownership_ticket_from(
    response: &CMsgClientGetAppOwnershipTicketResponse,
    app_id: u32,
) -> Result<Ticket, Error> {
    let code = response
        .eresult
        .ok_or(Error::IncompleteResponse("ownership ticket"))?;
    // The field is unsigned in this message; EResults are small.
    let code = i32::try_from(code).unwrap_or(i32::MAX);
    if code != RESULT_OK {
        return Err(Error::Refused {
            what: "ownership ticket",
            code,
        });
    }
    if response.app_id != Some(app_id) {
        return Err(Error::IncompleteResponse("ownership ticket"));
    }
    match &response.ticket {
        Some(ticket) if !ticket.is_empty() => Ok(Ticket(ticket.clone())),
        _ => Err(Error::IncompleteResponse("ownership ticket")),
    }
}

/// The encrypted ticket from Steam's answer, with the same checks.
pub fn encrypted_ticket_from(
    response: &CMsgClientRequestEncryptedAppTicketResponse,
    app_id: u32,
) -> Result<EncryptedTicket, Error> {
    let code = response
        .eresult
        .ok_or(Error::IncompleteResponse("encrypted app ticket"))?;
    if code != RESULT_OK {
        return Err(Error::Refused {
            what: "encrypted app ticket",
            code,
        });
    }
    if response.app_id != Some(app_id) {
        return Err(Error::IncompleteResponse("encrypted app ticket"));
    }
    let Some(ticket) = response.encrypted_app_ticket.as_ref() else {
        return Err(Error::IncompleteResponse("encrypted app ticket"));
    };
    let bytes = ticket
        .write_to_bytes()
        .map_err(|_| Error::IncompleteResponse("encrypted app ticket"))?;
    Ok(EncryptedTicket {
        ticket: Ticket(bytes),
    })
}

fn redact_ownership(
    response: &CMsgClientGetAppOwnershipTicketResponse,
) -> CMsgClientGetAppOwnershipTicketResponse {
    CMsgClientGetAppOwnershipTicketResponse {
        eresult: response.eresult,
        app_id: response.app_id,
        ticket: response.ticket.as_ref().map(|t| vec![0; t.len()]),
        ..Default::default()
    }
}

fn redact_encrypted(
    response: &CMsgClientRequestEncryptedAppTicketResponse,
) -> CMsgClientRequestEncryptedAppTicketResponse {
    let ticket = response
        .encrypted_app_ticket
        .as_ref()
        .map(|ticket| EncryptedAppTicket {
            ticket_version_no: ticket.ticket_version_no,
            crc_encryptedticket: ticket.crc_encryptedticket.map(|_| 0),
            cb_encrypteduserdata: ticket.cb_encrypteduserdata,
            cb_encrypted_appownershipticket: ticket.cb_encrypted_appownershipticket,
            encrypted_ticket: ticket.encrypted_ticket.as_ref().map(|t| vec![0; t.len()]),
            ..Default::default()
        });
    CMsgClientRequestEncryptedAppTicketResponse {
        app_id: response.app_id,
        eresult: response.eresult,
        encrypted_app_ticket: MessageField::from_option(ticket),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ownership(
        eresult: u32,
        app_id: u32,
        ticket: &[u8],
    ) -> CMsgClientGetAppOwnershipTicketResponse {
        CMsgClientGetAppOwnershipTicketResponse {
            eresult: Some(eresult),
            app_id: Some(app_id),
            ticket: Some(ticket.to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn ownership_tickets_are_checked() {
        let ticket = ownership_ticket_from(&ownership(1, 480, &[7; 20]), 480).unwrap();
        assert_eq!(ticket.len(), 20);
        assert!(matches!(
            ownership_ticket_from(&ownership(2, 480, &[7; 20]), 480),
            Err(Error::Refused { code: 2, .. })
        ));
        assert!(
            ownership_ticket_from(&ownership(1, 481, &[7; 20]), 480).is_err(),
            "another app"
        );
        assert!(
            ownership_ticket_from(&ownership(1, 480, &[]), 480).is_err(),
            "empty"
        );
    }

    #[test]
    fn encrypted_tickets_are_checked() {
        let response = CMsgClientRequestEncryptedAppTicketResponse {
            app_id: Some(480),
            eresult: Some(1),
            encrypted_app_ticket: MessageField::some(EncryptedAppTicket {
                ticket_version_no: Some(1),
                encrypted_ticket: Some(vec![9; 16]),
                ..Default::default()
            }),
            ..Default::default()
        };
        let ticket = encrypted_ticket_from(&response, 480).unwrap();
        let decoded = EncryptedAppTicket::parse_from_bytes(ticket.ticket.expose()).unwrap();
        assert_eq!(decoded.encrypted_ticket, Some(vec![9; 16]));
        let refused = CMsgClientRequestEncryptedAppTicketResponse {
            eresult: Some(8),
            ..response
        };
        assert!(matches!(
            encrypted_ticket_from(&refused, 480),
            Err(Error::Refused { code: 8, .. })
        ));
    }

    #[test]
    fn redaction_keeps_sizes_but_no_ticket_bytes() {
        let redacted = redact_ownership(&ownership(1, 480, &[7; 20]));
        assert_eq!(redacted.ticket, Some(vec![0; 20]));
        assert_eq!(redacted.eresult, Some(1));
        let encrypted = redact_encrypted(&CMsgClientRequestEncryptedAppTicketResponse {
            app_id: Some(480),
            eresult: Some(1),
            encrypted_app_ticket: MessageField::some(EncryptedAppTicket {
                ticket_version_no: Some(1),
                crc_encryptedticket: Some(0xDEAD_BEEF),
                encrypted_ticket: Some(vec![9; 16]),
                ..Default::default()
            }),
            ..Default::default()
        });
        let ticket = encrypted.encrypted_app_ticket.as_ref().unwrap();
        assert_eq!(ticket.encrypted_ticket, Some(vec![0; 16]));
        assert_eq!(ticket.crc_encryptedticket, Some(0));
        assert_eq!(ticket.ticket_version_no, Some(1));
    }

    /// Recorded with the first test account (see `tests/data/README.md`).
    const OWNED: &[u8] = include_bytes!("../tests/data/ownership_ticket.bin");
    const NO_KEY: &[u8] = include_bytes!("../tests/data/encrypted_app_ticket_no_key.bin");
    const ENCRYPTED: &[u8] = include_bytes!("../tests/data/encrypted_app_ticket.bin");
    const NOT_OWNED: &[u8] = include_bytes!("../tests/data/ownership_ticket_not_owned.bin");
    const NOT_OWNED_ENCRYPTED: &[u8] =
        include_bytes!("../tests/data/encrypted_app_ticket_not_owned.bin");

    #[test]
    fn replays_a_recorded_ownership_ticket() {
        let response = CMsgClientGetAppOwnershipTicketResponse::parse_from_bytes(OWNED).unwrap();
        let ticket = ownership_ticket_from(&response, 3_419_430).unwrap();
        assert_eq!(ticket.len(), 178);
        assert!(ticket.expose().iter().all(|&b| b == 0), "redacted");
        assert!(ownership_ticket_from(&response, 70).is_err(), "other app");
    }

    #[test]
    fn replays_a_recorded_encrypted_ticket() {
        // App 480 (Spacewar) has an encryption key.
        let response =
            CMsgClientRequestEncryptedAppTicketResponse::parse_from_bytes(ENCRYPTED).unwrap();
        let ticket = encrypted_ticket_from(&response, 480).unwrap();
        let decoded = EncryptedAppTicket::parse_from_bytes(ticket.ticket.expose()).unwrap();
        assert_eq!(decoded.ticket_version_no, Some(2));
        assert_eq!(decoded.cb_encrypteduserdata, Some(0), "no userdata sent");
        assert_eq!(decoded.cb_encrypted_appownershipticket, Some(78));
        assert_eq!(decoded.encrypted_ticket.map(|t| t.len()), Some(128));
    }

    #[test]
    fn replays_recorded_refusals() {
        // An owned app without an encryption key: Fail (2).
        let no_key = CMsgClientRequestEncryptedAppTicketResponse::parse_from_bytes(NO_KEY).unwrap();
        assert!(matches!(
            encrypted_ticket_from(&no_key, 3_419_430),
            Err(Error::Refused { code: 2, .. })
        ));
        // An app the account does not own: AccessDenied (15) for both.
        let not_owned =
            CMsgClientGetAppOwnershipTicketResponse::parse_from_bytes(NOT_OWNED).unwrap();
        assert!(matches!(
            ownership_ticket_from(&not_owned, 70),
            Err(Error::Refused { code: 15, .. })
        ));
        let not_owned =
            CMsgClientRequestEncryptedAppTicketResponse::parse_from_bytes(NOT_OWNED_ENCRYPTED)
                .unwrap();
        assert!(matches!(
            encrypted_ticket_from(&not_owned, 70),
            Err(Error::Refused { code: 15, .. })
        ));
    }

    #[test]
    fn debug_output_hides_ticket_bytes() {
        let ticket = Ticket(vec![0xAB; 4]);
        assert_eq!(format!("{ticket:?}"), "Ticket(<4 bytes>)");
    }
}
