use std::borrow::Cow;
use std::hash::Hash;
use std::sync::Arc;

use bytes::Bytes;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::blob::{EntityBlob, StoreHint};
use crate::codec::JsonCodec;
use crate::hasher::stable_hash;
use crate::wrapper::{Coalesced, Fanout, Scoped};
use crate::{Result, Store};

pub trait StoreExt: Store {
    fn into_arced(self) -> Arc<Self>
    where
        Self: Sized,
    {
        Arc::new(self)
    }

    /// Prefixes every id with `prefix/`, which `FileStore` turns into a directory.
    fn scoped(self, prefix: impl Into<String>) -> Scoped<Self>
    where
        Self: Sized,
    {
        Scoped::new(self, prefix)
    }

    /// Reads from `self` only; writes go to both, and a failing `secondary` is logged, not
    /// propagated.
    fn fanout<S>(self, secondary: S) -> Fanout<Self, S>
    where
        Self: Sized,
        S: Store,
    {
        Fanout::new(self, secondary)
    }

    /// Lets one task compute a missing key while concurrent requests for it wait for the result.
    fn coalesced(self) -> Coalesced<Self>
    where
        Self: Sized,
    {
        Coalesced::new(self)
    }

    fn save_text<'a, I, S, H>(
        &'a self,
        id: I,
        text: S,
        deps: &'a H,
    ) -> impl Future<Output = Result<()>> + Send
    where
        I: Into<Cow<'a, str>> + Send,
        S: Into<Cow<'a, str>> + Send,
        H: Hash + Send + Sync + ?Sized,
    {
        async move {
            let id = id.into();
            let text = text.into();
            self.save_text_with_hash(&id, &text, stable_hash(deps))
                .await
        }
    }

    fn save_with<'a, I, H, T>(
        &'a self,
        id: I,
        value: &'a T,
        deps: &'a H,
        codec: &'a JsonCodec,
    ) -> impl Future<Output = Result<()>> + Send
    where
        I: Into<Cow<'a, str>> + Send,
        H: Hash + Send + Sync + ?Sized,
        T: Serialize + Send + Sync,
    {
        async move {
            let id = id.into();
            let blob = json_blob(value, codec)?;
            self.save_blob_with_hash(&id, blob, stable_hash(deps)).await
        }
    }

    // The `*_with_hash` helpers below are called by the `cached_*!` macros, which hash `deps` once.

    /// An entry that is not valid UTF-8 is invalidated and reported as a miss, so the caller
    /// recomputes it.
    #[doc(hidden)]
    fn load_text_with_hash(
        &self,
        id: &str,
        deps_hash: u128,
    ) -> impl Future<Output = Result<Option<String>>> + Send {
        async move {
            let Some(blob) = self.load_blob_with_hash(id, deps_hash).await? else {
                return Ok(None);
            };
            match String::from_utf8(Vec::from(blob.bytes)) {
                Ok(text) => Ok(Some(text)),
                Err(_) => {
                    self.invalidate_with_hash(id, deps_hash).await?;
                    Ok(None)
                }
            }
        }
    }

    #[doc(hidden)]
    fn save_text_with_hash<'a>(
        &'a self,
        id: &'a str,
        text: &'a str,
        deps_hash: u128,
    ) -> impl Future<Output = Result<()>> + Send + 'a {
        async move {
            let blob = EntityBlob {
                hint: StoreHint::MD,
                bytes: Bytes::copy_from_slice(text.as_bytes()),
            };
            self.save_blob_with_hash(id, blob, deps_hash).await
        }
    }

    /// An entry that does not decode as JSON is invalidated and reported as a miss, so the caller
    /// recomputes it.
    #[doc(hidden)]
    fn load_json_with_hash<'a, T>(
        &'a self,
        id: &'a str,
        deps_hash: u128,
    ) -> impl Future<Output = Result<Option<T>>> + Send + 'a
    where
        T: DeserializeOwned + Send + Sync,
    {
        async move {
            let Some(blob) = self.load_blob_with_hash(id, deps_hash).await? else {
                return Ok(None);
            };
            match serde_json::from_slice(&blob.bytes) {
                Ok(value) => Ok(Some(value)),
                Err(_) => {
                    self.invalidate_with_hash(id, deps_hash).await?;
                    Ok(None)
                }
            }
        }
    }

    #[doc(hidden)]
    fn save_json_with_hash<'a, T>(
        &'a self,
        id: &'a str,
        value: &'a T,
        deps_hash: u128,
    ) -> impl Future<Output = Result<()>> + Send + 'a
    where
        T: Serialize + Send + Sync,
    {
        async move {
            let blob = json_blob(value, &JsonCodec::pretty())?;
            self.save_blob_with_hash(id, blob, deps_hash).await
        }
    }
}

fn json_blob<T: Serialize>(value: &T, codec: &JsonCodec) -> Result<EntityBlob> {
    let bytes = codec.encode(value).map_err(crate::Error::codec)?;
    Ok(EntityBlob {
        hint: StoreHint::JSON,
        bytes: Bytes::from(bytes),
    })
}

impl<S: Store + ?Sized> StoreExt for S {}
