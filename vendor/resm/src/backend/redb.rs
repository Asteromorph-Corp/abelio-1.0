//! Persistent store in a single redb file, `<base_path>/resm.redb`.
//!
//! The on-disk cache format, kept stable so existing caches stay readable:
//! - `blobs`: `"{id}\0{deps_hash:032x}"` to `[hint_len: 4 bytes LE][hint][bytes]`.
//! - `meta`: same key to `[created_ms][accessed_ms][size]`, 8 bytes LE each.
//!
//! An entry without a `meta` row reads as a miss in that format, so `meta` is still written.
//! Its TTL, size-limit and statistics tables (`access_idx`, `creation_idx`, `stats`) are neither
//! read nor maintained.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use redb::{Database, ReadableDatabase, TableDefinition};

use crate::blob::{EntityBlob, StoreHint};
use crate::store::Store;
use crate::{Error, Result};

const BLOBS: TableDefinition<&str, &[u8]> = TableDefinition::new("blobs");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");

fn compound_key(id: &str, deps_hash: u128) -> String {
    format!("{id}\x00{deps_hash:032x}")
}

fn encode_blob(blob: &EntityBlob) -> Vec<u8> {
    let hint = blob.hint.extension().as_bytes();
    let mut payload = Vec::with_capacity(4 + hint.len() + blob.bytes.len());
    payload.extend_from_slice(&(hint.len() as u32).to_le_bytes());
    payload.extend_from_slice(hint);
    payload.extend_from_slice(&blob.bytes);
    payload
}

fn decode_blob(payload: &[u8]) -> Result<EntityBlob> {
    let malformed = || Error::backend("malformed redb blob payload");
    let hint_len = payload
        .get(..4)
        .map(|len| u32::from_le_bytes(len.try_into().expect("4-byte slice")) as usize)
        .ok_or_else(malformed)?;
    let hint = payload
        .get(4..4 + hint_len)
        .and_then(|hint| std::str::from_utf8(hint).ok())
        .ok_or_else(malformed)?;
    Ok(EntityBlob {
        hint: StoreHint::custom(hint.to_owned()),
        bytes: Bytes::copy_from_slice(&payload[4 + hint_len..]),
    })
}

fn encode_meta(size_bytes: u64) -> [u8; 24] {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let mut meta = [0u8; 24];
    meta[0..8].copy_from_slice(&now_ms.to_le_bytes());
    meta[8..16].copy_from_slice(&now_ms.to_le_bytes());
    meta[16..24].copy_from_slice(&size_bytes.to_le_bytes());
    meta
}

#[derive(Clone)]
pub struct RedbStore {
    db: Arc<Database>,
}

impl RedbStore {
    /// Opens or creates `<base_path>/resm.redb`; a second open of the same file in one process
    /// fails on redb's file lock.
    pub fn new(base_path: impl Into<PathBuf>) -> Result<Self> {
        let base_path = base_path.into();
        std::fs::create_dir_all(&base_path)?;

        let db = Database::create(base_path.join("resm.redb")).map_err(Error::backend)?;

        let txn = db.begin_write().map_err(Error::backend)?;
        txn.open_table(BLOBS).map_err(Error::backend)?;
        txn.open_table(META).map_err(Error::backend)?;
        txn.commit().map_err(Error::backend)?;

        Ok(Self { db: Arc::new(db) })
    }

    async fn blocking<T, F>(&self, task: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Database) -> Result<T> + Send + 'static,
    {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || task(&db))
            .await
            .map_err(Error::backend)?
    }
}

impl Store for RedbStore {
    async fn save_blob_with_hash(&self, id: &str, blob: EntityBlob, deps_hash: u128) -> Result<()> {
        let key = compound_key(id, deps_hash);
        let payload = encode_blob(&blob);
        let meta = encode_meta(blob.bytes.len() as u64);

        self.blocking(move |db| {
            let txn = db.begin_write().map_err(Error::backend)?;
            {
                let mut blobs = txn.open_table(BLOBS).map_err(Error::backend)?;
                blobs
                    .insert(key.as_str(), payload.as_slice())
                    .map_err(Error::backend)?;
                let mut metas = txn.open_table(META).map_err(Error::backend)?;
                metas
                    .insert(key.as_str(), meta.as_slice())
                    .map_err(Error::backend)?;
            }
            txn.commit().map_err(Error::backend)
        })
        .await
    }

    async fn load_blob_with_hash(&self, id: &str, deps_hash: u128) -> Result<Option<EntityBlob>> {
        let key = compound_key(id, deps_hash);

        self.blocking(move |db| {
            let txn = db.begin_read().map_err(Error::backend)?;
            let blobs = txn.open_table(BLOBS).map_err(Error::backend)?;
            let Some(payload) = blobs.get(key.as_str()).map_err(Error::backend)? else {
                return Ok(None);
            };
            decode_blob(payload.value()).map(Some)
        })
        .await
    }

    async fn invalidate_with_hash(&self, id: &str, deps_hash: u128) -> Result<bool> {
        let key = compound_key(id, deps_hash);

        self.blocking(move |db| {
            let txn = db.begin_write().map_err(Error::backend)?;
            let removed = {
                let mut blobs = txn.open_table(BLOBS).map_err(Error::backend)?;
                let removed = blobs
                    .remove(key.as_str())
                    .map_err(Error::backend)?
                    .is_some();
                let mut metas = txn.open_table(META).map_err(Error::backend)?;
                metas.remove(key.as_str()).map_err(Error::backend)?;
                removed
            };
            txn.commit().map_err(Error::backend)?;
            Ok(removed)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::hasher::stable_hash;
    use crate::test_support::load_text;
    use crate::{StoreExt, cached_json, cached_text};

    fn temp_store() -> (TempDir, RedbStore) {
        let dir = TempDir::new().unwrap();
        let store = RedbStore::new(dir.path()).unwrap();
        (dir, store)
    }

    #[tokio::test]
    async fn entries_are_keyed_by_id_and_deps() {
        let (_dir, store) = temp_store();

        store.save_text("key", "for v1", &"v1").await.unwrap();
        store.save_text("key", "for v2", &"v2").await.unwrap();

        assert_eq!(
            load_text(&store, "key", &"v1").await.as_deref(),
            Some("for v1")
        );
        assert_eq!(
            load_text(&store, "key", &"v2").await.as_deref(),
            Some("for v2")
        );
        assert_eq!(load_text(&store, "key", &"v3").await, None);
    }

    #[tokio::test]
    async fn invalidate_removes_only_the_matching_entry() {
        let (_dir, store) = temp_store();
        store.save_text("key", "v1", &1).await.unwrap();
        store.save_text("key", "v2", &2).await.unwrap();

        assert!(
            store
                .invalidate_with_hash("key", stable_hash(&1))
                .await
                .unwrap()
        );
        assert!(
            !store
                .invalidate_with_hash("key", stable_hash(&1))
                .await
                .unwrap()
        );

        assert_eq!(load_text(&store, "key", &1).await, None);
        assert_eq!(load_text(&store, "key", &2).await.as_deref(), Some("v2"));
    }

    #[tokio::test]
    async fn overwrite_keeps_the_latest_value_and_hint() {
        let (_dir, store) = temp_store();

        store.save_text("key", "first", &()).await.unwrap();
        store.save_text("key", "second", &()).await.unwrap();

        let blob = store
            .load_blob_with_hash("key", stable_hash(&()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(blob.hint, StoreHint::MD);
        assert_eq!(blob.bytes.as_ref(), b"second");
    }

    #[tokio::test]
    async fn resumes_from_cache_after_reopen() {
        let dir = TempDir::new().unwrap();
        let deps = ("workflow", "v1");

        {
            let store = RedbStore::new(dir.path()).unwrap();
            let _: String =
                cached_text!(store, "step1", deps: &deps, { "step1-result".to_string() }).unwrap();
        }

        let store = RedbStore::new(dir.path()).unwrap();
        let mut recomputed = false;
        let step1: String = cached_text!(store, "step1", deps: &deps, {
            recomputed = true;
            "again".to_string()
        })
        .unwrap();

        assert_eq!(step1, "step1-result");
        assert!(!recomputed);
    }

    #[tokio::test]
    async fn json_entries_survive_reopen() {
        let dir = TempDir::new().unwrap();
        let first: Vec<u32> = {
            let store = RedbStore::new(dir.path()).unwrap();
            cached_json!(store, "numbers", deps: &(), { vec![1, 2, 3] }).unwrap()
        };

        let store = RedbStore::new(dir.path()).unwrap();
        let second: Vec<u32> = cached_json!(store, "numbers", deps: &(), { vec![9] }).unwrap();
        assert_eq!(first, second);
    }

    // Layout and hash of the on-disk cache format, written here with redb directly so the test does
    // not depend on RedbStore's own encoder.
    #[tokio::test]
    async fn reads_a_database_in_the_on_disk_format() {
        let dir = TempDir::new().unwrap();
        let deps_hash = 0x1000063c5c112d3c058a2688db25e19f_u128;
        let key = "scope/text\x001000063c5c112d3c058a2688db25e19f";
        let mut payload = 2u32.to_le_bytes().to_vec();
        payload.extend_from_slice(b"md");
        payload.extend_from_slice(b"legacy text");
        {
            let db = Database::create(dir.path().join("resm.redb")).unwrap();
            let txn = db.begin_write().unwrap();
            txn.open_table(BLOBS)
                .unwrap()
                .insert(key, payload.as_slice())
                .unwrap();
            txn.open_table(META)
                .unwrap()
                .insert(key, [0u8; 24].as_slice())
                .unwrap();
            txn.commit().unwrap();
        }
        assert_eq!(stable_hash(&("v1", 1u32)), deps_hash);

        let store = RedbStore::new(dir.path()).unwrap().scoped("scope");
        assert_eq!(
            load_text(&store, "text", &("v1", 1u32)).await.as_deref(),
            Some("legacy text")
        );
    }

    #[tokio::test]
    async fn saves_the_meta_row_the_on_disk_format_requires() {
        let (dir, store) = temp_store();
        store.save_text("key", "value", &()).await.unwrap();
        drop(store);

        let db = Database::open(dir.path().join("resm.redb")).unwrap();
        let txn = db.begin_read().unwrap();
        let key = compound_key("key", stable_hash(&()));
        let metas = txn.open_table(META).unwrap();
        let meta = metas.get(key.as_str()).unwrap().unwrap();
        assert_eq!(meta.value().len(), 24);
        assert_eq!(
            u64::from_le_bytes(meta.value()[16..24].try_into().unwrap()),
            5
        );
    }

    #[tokio::test]
    async fn malformed_payload_is_an_error() {
        let (_dir, store) = temp_store();
        let key = compound_key("key", stable_hash(&()));
        let txn = store.db.begin_write().unwrap();
        txn.open_table(BLOBS)
            .unwrap()
            .insert(key.as_str(), [1u8].as_slice())
            .unwrap();
        txn.commit().unwrap();

        let err = store
            .load_blob_with_hash("key", stable_hash(&()))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::Backend(_)));
    }
}
