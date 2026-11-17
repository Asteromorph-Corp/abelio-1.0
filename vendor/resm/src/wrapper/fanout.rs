use crate::blob::EntityBlob;
use crate::{Result, Store};

#[derive(Clone, Debug)]
pub struct Fanout<P, S> {
    primary: P,
    secondary: S,
}

impl<P, S> Fanout<P, S> {
    pub fn new(primary: P, secondary: S) -> Self {
        Self { primary, secondary }
    }
}

impl<P: Store, S: Store> Store for Fanout<P, S> {
    async fn save_blob_with_hash(&self, id: &str, blob: EntityBlob, deps_hash: u128) -> Result<()> {
        self.primary
            .save_blob_with_hash(id, blob.clone(), deps_hash)
            .await?;

        if let Err(error) = self
            .secondary
            .save_blob_with_hash(id, blob, deps_hash)
            .await
        {
            tracing::warn!(%error, "fanout: secondary save failed for '{id}'");
        }

        Ok(())
    }

    async fn load_blob_with_hash(&self, id: &str, deps_hash: u128) -> Result<Option<EntityBlob>> {
        self.primary.load_blob_with_hash(id, deps_hash).await
    }

    async fn invalidate_with_hash(&self, id: &str, deps_hash: u128) -> Result<bool> {
        let removed = self.primary.invalidate_with_hash(id, deps_hash).await?;

        if let Err(error) = self.secondary.invalidate_with_hash(id, deps_hash).await {
            tracing::warn!(%error, "fanout: secondary invalidate failed for '{id}'");
        }

        Ok(removed)
    }

    fn on_compute_error(&self, id: &str, deps_hash: u128) {
        self.primary.on_compute_error(id, deps_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreExt;
    use crate::backend::{FileStore, MemoryStore};
    use crate::hasher::stable_hash;
    use crate::test_support::load_text;

    #[tokio::test]
    async fn writes_reach_both_stores_and_reads_use_the_primary() {
        let primary = MemoryStore::new();
        let secondary = MemoryStore::new();
        let fanout = Fanout::new(primary.clone(), secondary.clone());

        fanout.save_text("key", "value", &()).await.unwrap();
        assert_eq!(
            load_text(&primary, "key", &()).await.as_deref(),
            Some("value")
        );
        assert_eq!(
            load_text(&secondary, "key", &()).await.as_deref(),
            Some("value")
        );

        secondary
            .save_text("only-secondary", "s", &())
            .await
            .unwrap();
        assert_eq!(load_text(&fanout, "only-secondary", &()).await, None);
    }

    #[tokio::test]
    async fn invalidate_reaches_both_stores() {
        let primary = MemoryStore::new();
        let secondary = MemoryStore::new();
        let fanout = Fanout::new(primary.clone(), secondary.clone());

        fanout.save_text("key", "value", &()).await.unwrap();
        assert!(
            fanout
                .invalidate_with_hash("key", stable_hash(&()))
                .await
                .unwrap()
        );

        assert_eq!(load_text(&primary, "key", &()).await, None);
        assert_eq!(load_text(&secondary, "key", &()).await, None);
    }

    #[tokio::test]
    async fn secondary_failure_does_not_fail_the_save() {
        let dir = tempfile::tempdir().unwrap();
        let primary = MemoryStore::new();
        let fanout = Fanout::new(primary.clone(), FileStore::new(dir.path()).unwrap());

        // FileStore rejects ids that escape its directory.
        fanout.save_text("../escape", "value", &()).await.unwrap();
        assert_eq!(
            load_text(&primary, "../escape", &()).await.as_deref(),
            Some("value")
        );
    }
}
