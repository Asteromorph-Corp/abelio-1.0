//! Human-readable store: one file per id under the base directory, with scope prefixes as
//! subdirectories.
//!
//! The `deps_hash` and hint of each entry live in a sidecar `.resm_file_meta/<id>.meta`, laid out
//! as `[deps_hash: 16 bytes LE][hint_len: 4 bytes LE][hint: utf-8]`.

use std::borrow::Cow;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::blob::{EntityBlob, StoreHint};
use crate::store::Store;
use crate::{Error, Result};

const META_DIR: &str = ".resm_file_meta";
const SIDECAR_FIXED_LEN: usize = 20;

struct Sidecar {
    deps_hash: u128,
    hint: StoreHint,
}

impl Sidecar {
    fn encode(&self) -> Vec<u8> {
        let hint = self.hint.extension().as_bytes();
        let mut buf = Vec::with_capacity(SIDECAR_FIXED_LEN + hint.len());
        buf.extend_from_slice(&self.deps_hash.to_le_bytes());
        buf.extend_from_slice(&(hint.len() as u32).to_le_bytes());
        buf.extend_from_slice(hint);
        buf
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let malformed = || Error::backend("malformed file store sidecar");
        if bytes.len() < SIDECAR_FIXED_LEN {
            return Err(malformed());
        }
        let deps_hash = u128::from_le_bytes(bytes[0..16].try_into().expect("16-byte slice"));
        let hint_len = u32::from_le_bytes(bytes[16..20].try_into().expect("4-byte slice")) as usize;
        let hint = bytes
            .get(SIDECAR_FIXED_LEN..SIDECAR_FIXED_LEN + hint_len)
            .and_then(|hint| std::str::from_utf8(hint).ok())
            .ok_or_else(malformed)?;
        Ok(Self {
            deps_hash,
            hint: StoreHint::custom(hint.to_owned()),
        })
    }
}

/// Removes the temp file on drop, including when the writing future is cancelled mid-flight.
struct TempFileGuard(Option<PathBuf>);

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            // Best effort: the guard runs on cancellation, where there is no caller to report to.
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileStore {
    base_path: PathBuf,
}

impl FileStore {
    pub fn new(base_path: impl Into<PathBuf>) -> Result<Self> {
        let base_path = base_path.into();
        std::fs::create_dir_all(&base_path)?;
        let base_path = std::fs::canonicalize(&base_path)?;
        std::fs::create_dir_all(base_path.join(META_DIR))?;
        Ok(Self { base_path })
    }

    /// Replaces characters outside `[A-Za-z0-9_/-]` with `_`, and rejects ids that could leave the
    /// base directory.
    fn safe_id(id: &str) -> Result<Cow<'_, str>> {
        if id.is_empty() || id.contains("..") || id.starts_with('/') {
            return Err(Error::Unsupported(format!("invalid id '{id}'")));
        }
        let is_safe = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/');
        if id.chars().all(is_safe) {
            Ok(Cow::Borrowed(id))
        } else {
            Ok(Cow::Owned(
                id.chars()
                    .map(|c| if is_safe(c) { c } else { '_' })
                    .collect(),
            ))
        }
    }

    fn entity_path(&self, safe_id: &str, hint: &StoreHint) -> Result<PathBuf> {
        let path = self
            .base_path
            .join(format!("{safe_id}.{}", hint.extension()));
        // An id ending in '/' would name a hidden file without an extension.
        if path.extension() != Some(hint.extension().as_ref()) {
            return Err(Error::Unsupported(format!("invalid id '{safe_id}'")));
        }
        Ok(path)
    }

    fn sidecar_path(&self, safe_id: &str) -> PathBuf {
        self.base_path
            .join(META_DIR)
            .join(format!("{safe_id}.meta"))
    }

    async fn load_sidecar(&self, safe_id: &str) -> Result<Option<Sidecar>> {
        match tokio::fs::read(self.sidecar_path(safe_id)).await {
            Ok(bytes) => Sidecar::decode(&bytes).map(Some),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Returns whether a file was removed.
    async fn remove_file(path: &Path) -> Result<bool> {
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("tmp");
        let tmp = path.with_file_name(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
        let mut guard = TempFileGuard(Some(tmp.clone()));

        let mut file = tokio::fs::File::create(&tmp).await?;
        file.write_all(bytes).await?;
        file.sync_all().await?;
        drop(file);
        tokio::fs::rename(&tmp, path).await?;

        guard.0 = None;
        Ok(())
    }
}

impl Store for FileStore {
    async fn save_blob_with_hash(&self, id: &str, blob: EntityBlob, deps_hash: u128) -> Result<()> {
        let safe_id = Self::safe_id(id)?;
        let path = self.entity_path(&safe_id, &blob.hint)?;

        // The previous file is removed only after the new one is in place, so a failed write loses
        // neither.
        let stale_path = match self.load_sidecar(&safe_id).await? {
            Some(old) if old.hint != blob.hint => Some(self.entity_path(&safe_id, &old.hint)?),
            _ => None,
        };

        Self::atomic_write(&path, &blob.bytes).await?;
        let sidecar = Sidecar {
            deps_hash,
            hint: blob.hint,
        };
        Self::atomic_write(&self.sidecar_path(&safe_id), &sidecar.encode()).await?;

        if let Some(stale_path) = stale_path {
            Self::remove_file(&stale_path).await?;
        }
        Ok(())
    }

    async fn load_blob_with_hash(&self, id: &str, deps_hash: u128) -> Result<Option<EntityBlob>> {
        let safe_id = Self::safe_id(id)?;

        let Some(sidecar) = self.load_sidecar(&safe_id).await? else {
            return Ok(None);
        };
        if sidecar.deps_hash != deps_hash {
            return Ok(None);
        }

        match tokio::fs::read(self.entity_path(&safe_id, &sidecar.hint)?).await {
            Ok(bytes) => Ok(Some(EntityBlob {
                hint: sidecar.hint,
                bytes: Bytes::from(bytes),
            })),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                Self::remove_file(&self.sidecar_path(&safe_id)).await?;
                Ok(None)
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn invalidate_with_hash(&self, id: &str, deps_hash: u128) -> Result<bool> {
        let safe_id = Self::safe_id(id)?;

        let Some(sidecar) = self.load_sidecar(&safe_id).await? else {
            return Ok(false);
        };
        if sidecar.deps_hash != deps_hash {
            return Ok(false);
        }

        Self::remove_file(&self.entity_path(&safe_id, &sidecar.hint)?).await?;
        Self::remove_file(&self.sidecar_path(&safe_id)).await?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::StoreExt;
    use crate::hasher::stable_hash;
    use crate::test_support::load_text;

    fn temp_store() -> (TempDir, FileStore) {
        let dir = TempDir::new().unwrap();
        let store = FileStore::new(dir.path()).unwrap();
        (dir, store)
    }

    fn json_blob(bytes: &'static [u8]) -> EntityBlob {
        EntityBlob {
            hint: StoreHint::JSON,
            bytes: Bytes::from_static(bytes),
        }
    }

    #[tokio::test]
    async fn text_round_trips_and_misses_on_changed_deps() {
        let (_dir, store) = temp_store();

        store.save_text("hello", "world", &"v1").await.unwrap();

        assert_eq!(
            load_text(&store, "hello", &"v1").await.as_deref(),
            Some("world")
        );
        assert_eq!(load_text(&store, "hello", &"v2").await, None);
    }

    #[tokio::test]
    async fn one_visible_file_per_id_overwritten_in_place() {
        let (_dir, store) = temp_store();
        let base = store.base_path.clone();

        store.save_text("entity", "first", &"v1").await.unwrap();
        store.save_text("entity", "second", &"v2").await.unwrap();
        store
            .clone()
            .scoped("run_1")
            .save_text("step", "s", &())
            .await
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(base.join("entity.md")).unwrap(),
            "second"
        );
        assert_eq!(
            std::fs::read_to_string(base.join("run_1/step.md")).unwrap(),
            "s"
        );
        let mut visible: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with('.'))
            .collect();
        visible.sort();
        assert_eq!(visible, ["entity.md", "run_1"]);
    }

    #[tokio::test]
    async fn hint_change_removes_the_old_file() {
        let (_dir, store) = temp_store();
        let base = store.base_path.clone();

        store
            .save_blob_with_hash("data", json_blob(b"{}"), 1)
            .await
            .unwrap();
        assert!(base.join("data.json").exists());

        store.save_text("data", "now text", &"v2").await.unwrap();

        assert!(!base.join("data.json").exists());
        assert!(base.join("data.md").exists());
    }

    #[tokio::test]
    async fn invalidate_needs_the_matching_hash() {
        let (_dir, store) = temp_store();
        store.save_text("key", "value", &"v1").await.unwrap();

        assert!(
            !store
                .invalidate_with_hash("key", stable_hash(&"v2"))
                .await
                .unwrap()
        );
        assert_eq!(
            load_text(&store, "key", &"v1").await.as_deref(),
            Some("value")
        );

        assert!(
            store
                .invalidate_with_hash("key", stable_hash(&"v1"))
                .await
                .unwrap()
        );
        assert_eq!(load_text(&store, "key", &"v1").await, None);
        assert!(!store.base_path.join("key.md").exists());
    }

    #[tokio::test]
    async fn ids_escaping_the_base_directory_are_rejected() {
        let (_dir, store) = temp_store();

        for id in ["../escape", "/absolute", "", "dir/"] {
            let err = store.save_text(id, "bad", &()).await.unwrap_err();
            assert!(matches!(err, Error::Unsupported(_)), "{id}: {err:?}");
        }
    }

    #[tokio::test]
    async fn reads_a_store_in_the_on_disk_format() {
        let dir = TempDir::new().unwrap();
        let deps_hash = stable_hash(&("v1", 1u32));
        assert_eq!(deps_hash, 0x1000063c5c112d3c058a2688db25e19f);

        // Byte layout of the on-disk cache format for `scoped("scope")` + `cached_text!("text",
        // deps: &("v1", 1u32))`.
        let mut sidecar = deps_hash.to_le_bytes().to_vec();
        sidecar.extend_from_slice(&2u32.to_le_bytes());
        sidecar.extend_from_slice(b"md");
        std::fs::create_dir_all(dir.path().join("scope")).unwrap();
        std::fs::write(dir.path().join("scope/text.md"), "legacy text").unwrap();
        std::fs::create_dir_all(dir.path().join(".resm_file_meta/scope")).unwrap();
        std::fs::write(dir.path().join(".resm_file_meta/scope/text.meta"), sidecar).unwrap();

        let store = FileStore::new(dir.path()).unwrap().scoped("scope");
        assert_eq!(
            load_text(&store, "text", &("v1", 1u32)).await.as_deref(),
            Some("legacy text")
        );
    }

    #[tokio::test]
    async fn malformed_sidecar_is_an_error() {
        let (dir, store) = temp_store();
        std::fs::write(dir.path().join(".resm_file_meta/key.meta"), b"short").unwrap();

        let err = store.load_blob_with_hash("key", 0).await.unwrap_err();
        assert!(matches!(err, Error::Backend(_)));
    }
}
