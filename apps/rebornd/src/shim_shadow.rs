//! Read-only shadow sessions (ADR 0055, amending ADR 0047).
//!
//! For shadow mode (ADR 0046, stage 1 of ADR 0055) the observation proxy
//! asks `rebornd` for Reborn's answer next to the official client's. The
//! game runs under the official client, so `rebornd` did not launch it;
//! instead `reborn-observe arm --app <id>` registers that one app here
//! before the game starts, and the proxy inside the game claims the
//! registration with its process ID (`ShadowHello`). Rules:
//!
//! - **One registration, one process.** A registration is claimed once;
//!   a later `ShadowHello` for it is refused. A new `RegisterShadow`
//!   replaces an unclaimed one; `CancelShadow` drops it.
//! - **Data from the current Valve session only.** The registration takes
//!   the ownership set from the licence list `rebornd` loaded for its
//!   current Valve session (`Session::licensed_app_ids`, cached per
//!   session as for shim-mode launches). Without a Valve session it is
//!   refused; `rebornd` does not sign in for it.
//! - **Strict ownership.** Only the session's own app is answered. The
//!   DLC list is left empty: reading the app's PICS info for its DLC is
//!   not needed for `BIsSubscribed`, so DLC get `NotAllowed` like any
//!   other app until a shadow for position 7 needs them.
//! - **Reads only.** A shadow session gets `AppOwnership`,
//!   `RequestStats` and `ComparePersonaName` (only `matches`, ADR 0056);
//!   every write is refused (`shim_server`). Cloud reads
//!   (`CloudFileSize`, `CloudRead`) get `NotAllowed` until the separate
//!   shadow folder of ADR 0055 exists (stage 1).
//! - **Any logoff stops the stage.** When the Valve session ends in any
//!   way (a logoff or kick, a lost connection, a sign-out), every shadow
//!   session and the registration are dropped and later requests get
//!   `NotAllowed`; a reconnect does not bring them back (ADR 0055: "any
//!   logoff or kick of either client stops the stage").
//! - **Kept for the run only.** A claimed session lives as long as the
//!   proxy's connection; its data stays in memory and is never logged.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::shim_server::{RunOwnership, ShimRun};
use crate::shim_stats::RunStats;

/// An app registered by `reborn-observe arm`, not claimed yet.
#[derive(Debug)]
struct Registration {
    app_id: u32,
    ownership: Arc<RunOwnership>,
}

#[derive(Debug, Default)]
struct Inner {
    registration: Option<Registration>,
    /// Claimed shadow sessions, by process ID, each with its claim's
    /// token.
    active: BTreeMap<u32, (u64, ShimRun)>,
    /// The last token handed out.
    last_token: u64,
}

/// The daemon's shadow registration and sessions.
#[derive(Debug, Default)]
pub struct ShadowSessions {
    inner: Mutex<Inner>,
}

impl ShadowSessions {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Registers `app_id` with the account's owned apps, replacing an
    /// unclaimed registration.
    pub fn register(&self, app_id: u32, ownership: RunOwnership) {
        self.inner().registration = Some(Registration {
            app_id,
            ownership: Arc::new(ownership),
        });
    }

    /// Drops an unclaimed registration; claimed sessions stay.
    pub fn cancel(&self) {
        self.inner().registration = None;
    }

    /// Claims the registration for `app_id` for process `pid`: the claim's
    /// token (it names this session, so a stale connection of an older
    /// session with the same process ID cannot reach or end it) and the
    /// shadow run, or `None` if no registration for that app is open or
    /// the process already has a shadow session.
    pub fn claim(&self, pid: u32, app_id: u32) -> Option<(u64, ShimRun)> {
        let mut inner = self.inner();
        if inner.active.contains_key(&pid) {
            return None;
        }
        if inner.registration.as_ref()?.app_id != app_id {
            return None;
        }
        let registration = inner.registration.take()?;
        let run = ShimRun {
            app_id,
            ownership: Some(registration.ownership),
            stats: Some(Arc::new(RunStats::default())),
            // The shadow folder for cloud reads comes with stage 1.
            cloud: None,
            shadow: true,
        };
        inner.last_token += 1;
        let token = inner.last_token;
        inner.active.insert(pid, (token, run.clone()));
        Some((token, run))
    }

    /// The shadow run of `pid` claimed with `token`, while that session
    /// lasts.
    pub fn get(&self, pid: u32, token: u64) -> Option<ShimRun> {
        self.inner()
            .active
            .get(&pid)
            .filter(|(claimed, _)| *claimed == token)
            .map(|(_, run)| run.clone())
    }

    /// Ends the shadow session of `pid` claimed with `token` (its
    /// connection closed); a newer session of the same process stays.
    pub fn end(&self, pid: u32, token: u64) {
        let mut inner = self.inner();
        if inner
            .active
            .get(&pid)
            .is_some_and(|(claimed, _)| *claimed == token)
        {
            inner.active.remove(&pid);
        }
    }

    /// Drops the registration and every shadow session (the Valve session
    /// ended). Whether anything was dropped.
    pub fn stop_all(&self) -> bool {
        let mut inner = self.inner();
        let any = inner.registration.is_some() || !inner.active.is_empty();
        inner.registration = None;
        inner.active.clear();
        any
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const DARK_DECEPTION: u32 = 332_950;

    fn owned(apps: &[u32]) -> RunOwnership {
        RunOwnership {
            owned: apps.iter().copied().collect(),
            dlc: BTreeSet::new(),
        }
    }

    #[test]
    fn a_registration_is_claimed_once_for_its_app() {
        let shadows = ShadowSessions::default();
        assert!(shadows.claim(10, DARK_DECEPTION).is_none(), "nothing open");
        shadows.register(DARK_DECEPTION, owned(&[DARK_DECEPTION]));
        assert!(shadows.claim(10, 480).is_none(), "another app");
        let (token, run) = shadows
            .claim(10, DARK_DECEPTION)
            .unwrap_or_else(|| panic!("not claimed"));
        assert!(run.shadow);
        assert_eq!(run.app_id, DARK_DECEPTION);
        assert!(run.stats.is_some());
        assert_eq!(
            shadows.get(10, token).map(|run| run.app_id),
            Some(DARK_DECEPTION)
        );
        assert!(shadows.get(10, token + 1).is_none(), "another claim");
        assert!(shadows.claim(11, DARK_DECEPTION).is_none(), "claimed once");
        shadows.end(10, token);
        assert!(shadows.get(10, token).is_none());
    }

    #[test]
    fn a_stale_claim_neither_reaches_nor_ends_a_newer_session() {
        let shadows = ShadowSessions::default();
        shadows.register(DARK_DECEPTION, owned(&[]));
        let (old, _) = shadows
            .claim(10, DARK_DECEPTION)
            .unwrap_or_else(|| panic!("not claimed"));
        // The Valve session ended, then the same process claims again.
        shadows.stop_all();
        shadows.register(DARK_DECEPTION, owned(&[]));
        let (new, _) = shadows
            .claim(10, DARK_DECEPTION)
            .unwrap_or_else(|| panic!("not claimed"));
        assert_ne!(old, new);
        assert!(shadows.get(10, old).is_none());
        shadows.end(10, old);
        assert!(shadows.get(10, new).is_some(), "the old close ends nothing");
    }

    #[test]
    fn a_process_gets_one_shadow_session() {
        let shadows = ShadowSessions::default();
        shadows.register(DARK_DECEPTION, owned(&[]));
        assert!(shadows.claim(10, DARK_DECEPTION).is_some());
        shadows.register(DARK_DECEPTION, owned(&[]));
        assert!(shadows.claim(10, DARK_DECEPTION).is_none());
        // The registration stays open for another process.
        assert!(shadows.claim(12, DARK_DECEPTION).is_some());
    }

    #[test]
    fn registrations_are_replaced_and_cancelled() {
        let shadows = ShadowSessions::default();
        shadows.register(480, owned(&[]));
        shadows.register(DARK_DECEPTION, owned(&[]));
        assert!(shadows.claim(10, 480).is_none(), "replaced");
        shadows.cancel();
        assert!(shadows.claim(10, DARK_DECEPTION).is_none(), "cancelled");
    }

    #[test]
    fn stopping_drops_everything() {
        let shadows = ShadowSessions::default();
        assert!(!shadows.stop_all());
        shadows.register(DARK_DECEPTION, owned(&[]));
        assert!(shadows.claim(10, DARK_DECEPTION).is_some());
        shadows.register(480, owned(&[]));
        assert!(shadows.stop_all());
        assert!(shadows.get(10, 1).is_none());
        assert!(shadows.claim(11, 480).is_none());
    }
}
