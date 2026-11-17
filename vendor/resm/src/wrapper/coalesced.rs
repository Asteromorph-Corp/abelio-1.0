use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::watch;

use crate::blob::EntityBlob;
use crate::{Result, Store};

// Arc<Sender> lets a timed-out waiter remove only the entry it waited on, by pointer comparison.
type InflightMap = DashMap<u128, Arc<watch::Sender<InflightState>>>;

/// Merges simultaneous loads of one key: the first task that sees a miss is the leader and computes
/// the value, the others wait for its save.
///
/// A leader that fails or is dropped reports through [`Store::on_compute_error`], and a waiter
/// gives up after the timeout and takes over.
#[derive(Clone, Debug)]
pub struct Coalesced<S> {
    inner: S,
    inflight: Arc<InflightMap>,
    timeout: Duration,
}

#[derive(Clone, Debug)]
enum InflightState {
    Pending,
    Done(Option<EntityBlob>),
    Error(Arc<crate::Error>),
}

enum WaitResult {
    Success(Option<EntityBlob>),
    Error(crate::Error),
    Retry,
}

/// Removes the key from the inflight map on drop, unless `forget` was called.
struct InflightGuard {
    inflight: Option<Arc<InflightMap>>,
    key: u128,
}

impl InflightGuard {
    fn new(inflight: Arc<InflightMap>, key: u128) -> Self {
        Self {
            inflight: Some(inflight),
            key,
        }
    }

    fn forget(mut self) {
        self.inflight = None;
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        if let Some(inflight) = &self.inflight {
            inflight.remove(&self.key);
        }
    }
}

impl<S> Coalesced<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            inflight: Arc::new(DashMap::new()),
            timeout: Duration::from_secs(60),
        }
    }

    /// How long a waiter waits for the leader before taking over; 60 seconds by default.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn coalesce_key(id: &str, deps_hash: u128) -> u128 {
        use crate::hasher::stable_hash;
        stable_hash(id).wrapping_add(deps_hash)
    }

    fn complete(&self, key: u128, state: InflightState) {
        if let Some((_, tx)) = self.inflight.remove(&key) {
            let _ = tx.send(state);
        }
    }

    fn complete_with_guard(&self, guard: InflightGuard, key: u128, state: InflightState) {
        self.complete(key, state);

        guard.forget();
    }
}

impl<S: Store> Store for Coalesced<S> {
    fn on_compute_error(&self, id: &str, deps_hash: u128) {
        let key = Self::coalesce_key(id, deps_hash);
        self.complete(
            key,
            InflightState::Error(Arc::new(crate::Error::Other(
                "computation failed or was cancelled".to_string(),
            ))),
        );
    }

    async fn save_blob_with_hash(&self, id: &str, blob: EntityBlob, deps_hash: u128) -> Result<()> {
        let key = Self::coalesce_key(id, deps_hash);

        let result = self
            .inner
            .save_blob_with_hash(id, blob.clone(), deps_hash)
            .await;

        match result {
            Ok(_) => {
                self.complete(key, InflightState::Done(Some(blob)));
                Ok(())
            }
            Err(e) => {
                let e = Arc::new(e);
                self.complete(key, InflightState::Error(e.clone()));
                Err(crate::Error::Shared(e))
            }
        }
    }

    async fn load_blob_with_hash(&self, id: &str, deps_hash: u128) -> Result<Option<EntityBlob>> {
        let key = Self::coalesce_key(id, deps_hash);

        loop {
            let waiter = if let Some(tx_ref) = self.inflight.get(&key) {
                let tx = tx_ref.clone();
                let rx = tx.subscribe();
                Some((tx, rx))
            } else {
                None
            };

            if let Some((tx, mut rx)) = waiter {
                if let InflightState::Done(ref blob) = *rx.borrow() {
                    return Ok(blob.clone());
                }

                let wait_res = tokio::time::timeout(self.timeout, async {
                    loop {
                        match rx.changed().await {
                            Ok(_) => {
                                let state = rx.borrow().clone();
                                match state {
                                    InflightState::Done(blob) => return WaitResult::Success(blob),
                                    InflightState::Error(e) => {
                                        return WaitResult::Error(crate::Error::Shared(e));
                                    }
                                    InflightState::Pending => continue,
                                }
                            }
                            Err(_) => {
                                return WaitResult::Retry;
                            }
                        }
                    }
                })
                .await;

                match wait_res {
                    Ok(WaitResult::Success(blob)) => return Ok(blob),
                    Ok(WaitResult::Error(e)) => return Err(e),
                    Ok(WaitResult::Retry) | Err(_) => {
                        self.inflight
                            .remove_if(&key, |_, existing_tx| Arc::ptr_eq(existing_tx, &tx));
                    }
                }
            }

            let (tx, _) = watch::channel(InflightState::Pending);
            let tx = Arc::new(tx);

            use dashmap::mapref::entry::Entry;
            match self.inflight.entry(key) {
                Entry::Occupied(_) => {
                    continue;
                }
                Entry::Vacant(v) => {
                    v.insert(tx);

                    let guard = InflightGuard::new(self.inflight.clone(), key);

                    match self.inner.load_blob_with_hash(id, deps_hash).await {
                        Ok(Some(blob)) => {
                            self.complete_with_guard(
                                guard,
                                key,
                                InflightState::Done(Some(blob.clone())),
                            );
                            return Ok(Some(blob));
                        }
                        Ok(None) => {
                            guard.forget();
                            return Ok(None);
                        }
                        Err(e) => {
                            let e = Arc::new(e);
                            self.complete_with_guard(guard, key, InflightState::Error(e.clone()));
                            return Err(crate::Error::Shared(e));
                        }
                    }
                }
            }
        }
    }

    async fn invalidate_with_hash(&self, id: &str, deps_hash: u128) -> Result<bool> {
        let key = Self::coalesce_key(id, deps_hash);

        self.complete(key, InflightState::Done(None));

        self.inner.invalidate_with_hash(id, deps_hash).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::backend::MemoryStore;
    use crate::test_support::load_text;
    use crate::{StoreExt, cached_text};

    #[tokio::test]
    async fn concurrent_requests_compute_once() {
        let store = MemoryStore::new().coalesced();
        let computed = Arc::new(AtomicUsize::new(0));

        let tasks: Vec<_> = (0..20)
            .map(|_| {
                let store = store.clone();
                let computed = computed.clone();
                tokio::spawn(async move {
                    cached_text!(store, "heavy", deps: &"v1", {
                        computed.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        "value".to_string()
                    })
                    .unwrap()
                })
            })
            .collect();

        for task in tasks {
            assert_eq!(task.await.unwrap(), "value");
        }
        assert_eq!(computed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_computation_releases_waiters() {
        let store = MemoryStore::new().coalesced();

        let tasks: Vec<_> = (0..5)
            .map(|i| {
                let store = store.clone();
                tokio::spawn(async move {
                    cached_text!(store, "failing", deps: &"v1", {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        Err::<String, _>(crate::Error::Other(format!("task {i} failed")))
                    })
                })
            })
            .collect();

        // The default timeout is 60s, so finishing within 2s proves waiters were notified.
        let results = tokio::time::timeout(Duration::from_secs(2), async {
            let mut results = Vec::new();
            for task in tasks {
                results.push(task.await.unwrap());
            }
            results
        })
        .await
        .expect("waiters were not released after the leader failed");

        assert!(results.iter().all(Result::is_err));
    }

    #[tokio::test]
    async fn cancelled_leader_releases_waiters() {
        let store = MemoryStore::new().coalesced();

        let leader = tokio::spawn({
            let store = store.clone();
            async move {
                let _ = cached_text!(store, "cancelled", deps: &"v1", {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    "never".to_string()
                });
            }
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        leader.abort();
        let _ = leader.await;

        let recovered = tokio::time::timeout(Duration::from_secs(2), async {
            cached_text!(store, "cancelled", deps: &"v1", { "recovered".to_string() })
        })
        .await
        .expect("waiter was not released after the leader was cancelled");
        assert_eq!(recovered.unwrap(), "recovered");
    }

    #[tokio::test]
    async fn waiter_takes_over_after_timeout() {
        let store = MemoryStore::new()
            .coalesced()
            .with_timeout(Duration::from_millis(50));

        // A leader that misses and never saves nor reports an error.
        assert_eq!(load_text(&store, "stuck", &()).await, None);

        let second =
            tokio::time::timeout(Duration::from_millis(500), load_text(&store, "stuck", &()))
                .await
                .expect("waiter did not take over after the timeout");
        assert_eq!(second, None);
    }
}
