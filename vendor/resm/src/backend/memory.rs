use std::sync::Arc;

use dashmap::DashMap;

use crate::Result;
use crate::blob::EntityBlob;
use crate::hasher::stable_hash;
use crate::store::Store;

/// Unbounded in-memory store; clones share the same entries.
#[derive(Debug, Clone, Default)]
pub struct MemoryStore {
    entries: Arc<DashMap<u128, EntityBlob>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(id: &str, deps_hash: u128) -> u128 {
        stable_hash(id).wrapping_add(deps_hash)
    }
}

impl Store for MemoryStore {
    async fn save_blob_with_hash(&self, id: &str, blob: EntityBlob, deps_hash: u128) -> Result<()> {
        self.entries.insert(Self::key(id, deps_hash), blob);
        Ok(())
    }

    async fn load_blob_with_hash(&self, id: &str, deps_hash: u128) -> Result<Option<EntityBlob>> {
        Ok(self
            .entries
            .get(&Self::key(id, deps_hash))
            .map(|entry| entry.clone()))
    }

    async fn invalidate_with_hash(&self, id: &str, deps_hash: u128) -> Result<bool> {
        Ok(self.entries.remove(&Self::key(id, deps_hash)).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreExt;
    use crate::test_support::load_text;

    #[tokio::test]
    async fn entries_are_keyed_by_id_and_deps() {
        let store = MemoryStore::new();

        store.save_text("hello", "world", &(1, 2, 3)).await.unwrap();
        assert_eq!(
            load_text(&store, "hello", &(1, 2, 3)).await.as_deref(),
            Some("world")
        );
        assert_eq!(load_text(&store, "hello", &(1, 2, 4)).await, None);

        assert!(
            store
                .invalidate_with_hash("hello", stable_hash(&(1, 2, 3)))
                .await
                .unwrap()
        );
        assert_eq!(load_text(&store, "hello", &(1, 2, 3)).await, None);
    }

    #[tokio::test]
    async fn clones_share_entries() {
        let store = MemoryStore::new();
        store.clone().save_text("key", "value", &()).await.unwrap();
        assert_eq!(
            load_text(&store, "key", &()).await.as_deref(),
            Some("value")
        );
    }
}
