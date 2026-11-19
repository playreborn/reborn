//! A Steam Guard prompt that a frontend answers from elsewhere (ADR 0014).
//!
//! steam-vent asks an [`AuthConfirmationHandler`] how to confirm a login and
//! races it against polling for tokens (`src/connection/unauthenticated.rs`,
//! `Connection::login`, v0.5.0). So approval in the Steam mobile app ends the
//! login even while a code prompt is still open.
//!
//! [`GuardPrompt`] reports what Steam asks for as a [`GuardChallenge`] and
//! waits for the code on a channel. steam-vent's `SteamGuardToken` can only
//! be built inside steam-vent, so the code is handed to its public
//! `UserProvidedAuthConfirmationHandler` through an in-memory reader
//! (`src/auth/confirmation.rs`), which builds the token exactly as it does
//! for codes typed into a terminal.

use std::io::Cursor;

use steam_vent::auth::{
    AuthConfirmationHandler, ConfirmationAction, ConfirmationMethod, ConfirmationMethodClass,
    GuardTokenType, UserProvidedAuthConfirmationHandler,
};
use tokio::sync::oneshot;

use crate::Secret;

/// Which kind of code Steam asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardCodeKind {
    /// A code Steam sent by email.
    Email,
    /// A code from the Steam Guard authenticator (mobile app).
    Authenticator,
}

/// What Steam Guard needs to accept a login.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardChallenge {
    /// The code to enter, if a code is accepted.
    pub code: Option<GuardCodeKind>,
    /// Whether approving the login in the Steam mobile app works.
    pub confirm_in_app: bool,
    /// Steam's hint for the code, e.g. the domain of the email address.
    pub hint: String,
}

/// Answers Steam Guard with a code that arrives later through
/// [`GuardPromptHandle::code`].
pub struct GuardPrompt {
    challenge: oneshot::Sender<GuardChallenge>,
    code: oneshot::Receiver<Secret>,
}

/// The frontend side of a [`GuardPrompt`].
pub struct GuardPromptHandle {
    /// Receives what Steam asks for. Closes without a value if Steam Guard
    /// needs nothing (known device) or the login ended first.
    pub challenge: oneshot::Receiver<GuardChallenge>,
    /// Send the code here. Dropping it without sending gives up the login.
    pub code: oneshot::Sender<Secret>,
}

impl GuardPrompt {
    /// A prompt and the handle that answers it.
    pub fn new() -> (Self, GuardPromptHandle) {
        let (challenge_tx, challenge_rx) = oneshot::channel();
        let (code_tx, code_rx) = oneshot::channel();
        (
            Self {
                challenge: challenge_tx,
                code: code_rx,
            },
            GuardPromptHandle {
                challenge: challenge_rx,
                code: code_tx,
            },
        )
    }
}

impl AuthConfirmationHandler for GuardPrompt {
    async fn handle_confirmation(
        self,
        allowed_confirmations: &[ConfirmationMethod],
    ) -> Option<ConfirmationAction> {
        let challenge = challenge(allowed_confirmations)?;
        let wants_code = challenge.code.is_some();
        if self.challenge.send(challenge).is_err() {
            // Nobody is there to answer.
            return Some(ConfirmationAction::Abort);
        }
        if !wants_code {
            // Only approval in the app: steam-vent keeps polling for tokens.
            return Some(ConfirmationAction::None);
        }
        let Ok(code) = self.code.await else {
            return Some(ConfirmationAction::Abort);
        };
        let line = format!("{}\n", code.expose().trim());
        UserProvidedAuthConfirmationHandler::new(Cursor::new(line.into_bytes()), tokio::io::sink())
            .handle_confirmation(allowed_confirmations)
            .await
    }
}

/// The challenge for Steam's allowed confirmations, or `None` if none of
/// them needs the user. The code kind is taken from the first method that
/// takes a code, as `UserProvidedAuthConfirmationHandler` does.
fn challenge(allowed: &[ConfirmationMethod]) -> Option<GuardChallenge> {
    let code_method = allowed.iter().find(|method| method.token_type().is_some());
    let confirm_in_app = allowed
        .iter()
        .any(|method| method.class() == ConfirmationMethodClass::Confirmation);
    if code_method.is_none() && !confirm_in_app {
        return None;
    }
    Some(GuardChallenge {
        code: code_method
            .and_then(ConfirmationMethod::token_type)
            .map(|kind| match kind {
                GuardTokenType::Email => GuardCodeKind::Email,
                GuardTokenType::Device => GuardCodeKind::Authenticator,
            }),
        confirm_in_app,
        hint: code_method
            .map(|method| method.confirmation_details().to_owned())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use steam_vent_proto_steam::steammessages_auth_steamclient::{
        CAuthentication_AllowedConfirmation, EAuthSessionGuardType,
    };

    fn method(kind: EAuthSessionGuardType, message: &str) -> ConfirmationMethod {
        let mut raw = CAuthentication_AllowedConfirmation::new();
        raw.set_confirmation_type(kind);
        raw.set_associated_message(message.to_owned());
        ConfirmationMethod::from(raw)
    }

    fn device_code() -> ConfirmationMethod {
        method(
            EAuthSessionGuardType::k_EAuthSessionGuardType_DeviceCode,
            "",
        )
    }

    fn app_approval() -> ConfirmationMethod {
        method(
            EAuthSessionGuardType::k_EAuthSessionGuardType_DeviceConfirmation,
            "",
        )
    }

    #[tokio::test]
    async fn reports_the_challenge_and_submits_the_code() {
        let (prompt, handle) = GuardPrompt::new();
        let allowed = [device_code(), app_approval()];
        let answer = tokio::spawn(async move {
            let challenge = handle.challenge.await.unwrap();
            handle.code.send(Secret::new(" ab12c ")).unwrap();
            challenge
        });

        let action = prompt.handle_confirmation(&allowed).await;
        assert_eq!(
            answer.await.unwrap(),
            GuardChallenge {
                code: Some(GuardCodeKind::Authenticator),
                confirm_in_app: true,
                hint: String::new(),
            }
        );
        let Some(ConfirmationAction::GuardToken(token, GuardTokenType::Device)) = action else {
            panic!("expected a device code, got {action:?}");
        };
        assert!(format!("{token:?}").contains("\"ab12c\""), "trimmed code");
    }

    #[tokio::test]
    async fn email_code_carries_the_hint() {
        let (prompt, handle) = GuardPrompt::new();
        let allowed = [method(
            EAuthSessionGuardType::k_EAuthSessionGuardType_EmailCode,
            "example.org",
        )];
        let answer = tokio::spawn(async move {
            let challenge = handle.challenge.await.unwrap();
            handle.code.send(Secret::new("XYZ99")).unwrap();
            challenge
        });
        let action = prompt.handle_confirmation(&allowed).await;
        let challenge = answer.await.unwrap();
        assert_eq!(challenge.code, Some(GuardCodeKind::Email));
        assert_eq!(challenge.hint, "example.org");
        assert!(!challenge.confirm_in_app);
        assert!(matches!(
            action,
            Some(ConfirmationAction::GuardToken(_, GuardTokenType::Email))
        ));
    }

    #[tokio::test]
    async fn app_approval_only_keeps_polling() {
        let (prompt, handle) = GuardPrompt::new();
        let action = prompt.handle_confirmation(&[app_approval()]).await;
        assert!(matches!(action, Some(ConfirmationAction::None)));
        let challenge = handle.challenge.await.unwrap();
        assert_eq!(challenge.code, None);
        assert!(challenge.confirm_in_app);
    }

    #[tokio::test]
    async fn dropping_the_handle_aborts() {
        let (prompt, handle) = GuardPrompt::new();
        let GuardPromptHandle { challenge, code } = handle;
        drop(code);
        let action = prompt.handle_confirmation(&[device_code()]).await;
        assert!(matches!(action, Some(ConfirmationAction::Abort)));
        assert!(challenge.await.is_ok(), "the challenge was still reported");
    }

    #[tokio::test]
    async fn nothing_to_confirm_sends_no_challenge() {
        let (prompt, handle) = GuardPrompt::new();
        let none = method(EAuthSessionGuardType::k_EAuthSessionGuardType_None, "");
        assert!(prompt.handle_confirmation(&[none]).await.is_none());
        assert!(handle.challenge.await.is_err());
    }
}
