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
//!   `RequestStats`, `ComparePersonaName` (only `matches`, ADR 0056) and
//!   the cloud reads `CloudFileSize` and `CloudRead`; every write is
//!   refused (`shim_server`). Cloud reads are answered from the
//!   registration's separate **shadow folder** (ADR 0055: "`CloudRead`
//!   against a separate shadow folder"; `reborn_core::shadow_cloud`),
//!   filled from Valve's cloud once, when the app is registered; never
//!   from the official client's folders or a shim-mode run's folder.
//!   Without a shadow folder (an app without a cloud quota, or the fill
//!   failed) cloud reads get `NotAllowed`.
//! - **Any logoff stops the stage.** When the Valve session ends in any
//!   way (a logoff or kick, a lost connection, a sign-out), every shadow
//!   session and the registration are dropped and later requests get
//!   `NotAllowed`; a reconnect does not bring them back (ADR 0055: "any
//!   logoff or kick of either client stops the stage").
//! - **Kept for the run only.** A claimed session lives as long as the
//!   proxy's connection; its data stays in memory and is never logged.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reborn_core::run_cloud::RunCloud;

use crate::shim_server::{RunOwnership, ShimRun};
use crate::shim_stats::RunStats;

/// An app registered by `reborn-observe arm`, not claimed yet.
#[derive(Debug)]
struct Registration {
    app_id: u32,
    ownership: Arc<RunOwnership>,
    /// The shadow folder for cloud reads, if the app has one.
    cloud: Option<Arc<RunCloud>>,
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

/// Shortest time between the starts of two shadow cloud fills (red line
/// 8): ADR 0052's one minute between game starts, so a real shadow run is
/// never held back, while a local client sending `RegisterShadow` in a
/// loop reaches Valve at most once a minute.
pub const FILL_COOLDOWN: Duration = Duration::from_secs(60);

/// What [`FillGate::run`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Gated<T> {
    /// The fill ran to its end.
    Ran(T),
    /// The fill took longer than its limit and was dropped.
    TimedOut,
    /// Another fill started less than the cooldown ago; nothing ran.
    Cooldown,
}

/// Runs shadow cloud fills one at a time, at most one per cooldown,
/// whatever the account or app (ADR 0055: requests "bounded per run"; red
/// line 8). A fill that failed or timed out counts too, so errors are not
/// retried in a loop either.
#[derive(Debug, Default)]
pub struct FillGate {
    /// When the last fill started; the lock makes fills take turns.
    last: tokio::sync::Mutex<Option<Instant>>,
}

impl FillGate {
    /// Runs `fill` unless one started less than `cooldown` ago, waiting
    /// for a running one first; drops it after `limit`.
    pub async fn run<T>(
        &self,
        cooldown: Duration,
        limit: Duration,
        fill: impl Future<Output = T>,
    ) -> Gated<T> {
        let mut last = self.last.lock().await;
        if last.is_some_and(|started| started.elapsed() < cooldown) {
            return Gated::Cooldown;
        }
        *last = Some(Instant::now());
        match tokio::time::timeout(limit, fill).await {
            Ok(done) => Gated::Ran(done),
            Err(_) => Gated::TimedOut,
        }
    }
}

impl ShadowSessions {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Registers `app_id` with the account's owned apps, replacing an
    /// unclaimed registration.
    #[cfg(test)]
    pub fn register(&self, app_id: u32, ownership: RunOwnership) {
        self.register_with_cloud(app_id, ownership, None);
    }

    /// Registers `app_id` with the account's owned apps (replacing an
    /// unclaimed registration) and the shadow folder `cloud` for cloud reads
    /// (ADR 0055, stage 1). Only a shadow folder is taken
    /// ([`RunCloud::is_shadow`]); any other folder is dropped, so a shadow
    /// session never reads a shim-mode run's saves.
    pub fn register_with_cloud(
        &self,
        app_id: u32,
        ownership: RunOwnership,
        cloud: Option<Arc<RunCloud>>,
    ) {
        self.inner().registration = Some(Registration {
            app_id,
            ownership: Arc::new(ownership),
            cloud: cloud.filter(|cloud| cloud.is_shadow()),
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
            cloud: registration.cloud,
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
    fn a_claim_gets_the_shadow_folder_and_never_a_runs_folder() {
        let shadows = ShadowSessions::default();
        let data = std::path::Path::new("data");
        let shadow = Arc::new(RunCloud::shadow(data, 1, DARK_DECEPTION, 10));
        shadows.register_with_cloud(DARK_DECEPTION, owned(&[]), Some(Arc::clone(&shadow)));
        let (_, run) = shadows
            .claim(10, DARK_DECEPTION)
            .unwrap_or_else(|| panic!("not claimed"));
        assert_eq!(run.cloud, Some(shadow));
        // A shim-mode run's folder is not a shadow folder: dropped.
        let run_folder = Arc::new(RunCloud::new(data, 1, DARK_DECEPTION, 10));
        shadows.register_with_cloud(DARK_DECEPTION, owned(&[]), Some(run_folder));
        let (_, run) = shadows
            .claim(11, DARK_DECEPTION)
            .unwrap_or_else(|| panic!("not claimed"));
        assert_eq!(run.cloud, None);
    }

    #[tokio::test]
    async fn fills_take_turns_once_per_cooldown() {
        let gate = FillGate::default();
        let long = Duration::from_secs(60);
        assert_eq!(gate.run(long, long, async { 1 }).await, Gated::Ran(1));
        // Within the cooldown nothing runs, whatever the app.
        let polled = std::sync::atomic::AtomicBool::new(false);
        let second = gate
            .run(long, long, async {
                polled.store(true, std::sync::atomic::Ordering::SeqCst);
            })
            .await;
        assert_eq!(second, Gated::Cooldown);
        assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
        // Without a cooldown it runs again; a slow fill is dropped.
        let none = Duration::ZERO;
        let slow = gate
            .run(
                none,
                Duration::from_millis(20),
                std::future::pending::<()>(),
            )
            .await;
        assert_eq!(slow, Gated::TimedOut);
        // A timed-out fill counts for the cooldown too.
        assert_eq!(gate.run(long, long, async { 2 }).await, Gated::Cooldown);
    }

    #[tokio::test]
    async fn concurrent_fills_wait_for_each_other() {
        let gate = Arc::new(FillGate::default());
        let long = Duration::from_secs(60);
        let (started, wait) = tokio::sync::oneshot::channel::<()>();
        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let first = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move {
                gate.run(long, long, async move {
                    let _ = started.send(());
                    let _ = released.await;
                    "first"
                })
                .await
            }
        });
        let _ = wait.await;
        let second = tokio::spawn({
            let gate = Arc::clone(&gate);
            async move { gate.run(long, long, async { "second" }).await }
        });
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert!(!second.is_finished(), "waits for the running fill");
        let _ = release.send(());
        assert_eq!(first.await.ok(), Some(Gated::Ran("first")));
        assert_eq!(second.await.ok(), Some(Gated::Cooldown));
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
