//! In-memory Steam Guard machine data, so that this crate never touches
//! the file system. The caller loads the data from the OS keyring before
//! the login and stores whatever new data Steam hands out afterwards.

use std::convert::Infallible;
use std::future::{Future, ready};
use std::sync::{Arc, Mutex};

use steam_vent::auth::GuardDataStore;

/// Hands previously stored guard data to `steam-vent` and captures new data.
pub(crate) struct MemoryGuardStore {
    stored: Option<String>,
    received: Arc<Mutex<Option<String>>>,
}

impl MemoryGuardStore {
    /// Returns the store and a handle that receives new guard data.
    pub(crate) fn new(stored: Option<String>) -> (Self, Arc<Mutex<Option<String>>>) {
        let received = Arc::new(Mutex::new(None));
        let store = Self {
            stored,
            received: Arc::clone(&received),
        };
        (store, received)
    }
}

impl GuardDataStore for MemoryGuardStore {
    type Err = Infallible;

    fn store(
        &mut self,
        _account: &str,
        machine_token: String,
    ) -> impl Future<Output = Result<(), Self::Err>> + Send {
        // A poisoned lock only means another thread panicked while holding
        // it; the slot itself is still usable.
        let mut slot = self
            .received
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(machine_token);
        ready(Ok(()))
    }

    fn load(
        &mut self,
        _account: &str,
    ) -> impl Future<Output = Result<Option<String>, Self::Err>> + Send {
        ready(Ok(self.stored.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryGuardStore;
    use steam_vent::auth::GuardDataStore;

    #[tokio::test]
    async fn loads_stored_and_captures_new_data() {
        let (mut store, received) = MemoryGuardStore::new(Some("old".to_owned()));
        assert_eq!(store.load("account").await, Ok(Some("old".to_owned())));

        store.store("account", "new".to_owned()).await.unwrap();
        assert_eq!(received.lock().unwrap().as_deref(), Some("new"));
    }
}
