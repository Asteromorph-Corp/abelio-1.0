mod ext;

use std::sync::Arc;

pub use ext::StoreExt;

use crate::Result;
use crate::blob::EntityBlob;

/// A keyed blob store.
///
/// An entry is addressed by `id` and valid only for the `deps_hash` it was saved with, so a changed
/// hash reads as a miss.
pub trait Store: Send + Sync {
    fn save_blob_with_hash(
        &self,
        id: &str,
        blob: EntityBlob,
        deps_hash: u128,
    ) -> impl Future<Output = Result<()>> + Send;

    fn load_blob_with_hash(
        &self,
        id: &str,
        deps_hash: u128,
    ) -> impl Future<Output = Result<Option<EntityBlob>>> + Send;

    /// Returns whether an entry was removed.
    fn invalidate_with_hash(
        &self,
        id: &str,
        deps_hash: u128,
    ) -> impl Future<Output = Result<bool>> + Send;

    /// Called when a computation fails or is cancelled after a cache miss, so wrappers can release
    /// tasks waiting on its result.
    fn on_compute_error(&self, _id: &str, _deps_hash: u128) {}
}

impl<S: Store + ?Sized> Store for &S {
    async fn save_blob_with_hash(&self, id: &str, blob: EntityBlob, deps_hash: u128) -> Result<()> {
        (**self).save_blob_with_hash(id, blob, deps_hash).await
    }

    async fn load_blob_with_hash(&self, id: &str, deps_hash: u128) -> Result<Option<EntityBlob>> {
        (**self).load_blob_with_hash(id, deps_hash).await
    }

    async fn invalidate_with_hash(&self, id: &str, deps_hash: u128) -> Result<bool> {
        (**self).invalidate_with_hash(id, deps_hash).await
    }

    fn on_compute_error(&self, id: &str, deps_hash: u128) {
        (**self).on_compute_error(id, deps_hash)
    }
}

impl<S: Store + ?Sized> Store for Arc<S> {
    async fn save_blob_with_hash(&self, id: &str, blob: EntityBlob, deps_hash: u128) -> Result<()> {
        self.as_ref().save_blob_with_hash(id, blob, deps_hash).await
    }

    async fn load_blob_with_hash(&self, id: &str, deps_hash: u128) -> Result<Option<EntityBlob>> {
        self.as_ref().load_blob_with_hash(id, deps_hash).await
    }

    async fn invalidate_with_hash(&self, id: &str, deps_hash: u128) -> Result<bool> {
        self.as_ref().invalidate_with_hash(id, deps_hash).await
    }

    fn on_compute_error(&self, id: &str, deps_hash: u128) {
        self.as_ref().on_compute_error(id, deps_hash)
    }
}
