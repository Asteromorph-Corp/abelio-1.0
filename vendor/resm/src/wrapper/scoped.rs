use crate::blob::EntityBlob;
use crate::{Result, Store};

#[derive(Debug, Clone)]
pub struct Scoped<S> {
    inner: S,
    prefix: String,
}

impl<S> Scoped<S> {
    /// The prefix is normalized to `a/b/`: empty and whitespace-only segments are dropped, leaving
    /// no leading slash and exactly one trailing slash.
    pub fn new(inner: S, prefix: impl Into<String>) -> Self {
        let normalized: String = prefix
            .into()
            .split('/')
            .filter(|s| s.chars().any(|c| !c.is_whitespace()))
            .collect::<Vec<_>>()
            .join("/");

        let prefix = if normalized.is_empty() {
            String::new()
        } else {
            format!("{normalized}/")
        };

        Self { inner, prefix }
    }

    fn scoped_id(&self, id: &str) -> String {
        format!("{}{id}", self.prefix)
    }
}

impl<S: Store> Store for Scoped<S> {
    async fn save_blob_with_hash(&self, id: &str, blob: EntityBlob, deps_hash: u128) -> Result<()> {
        self.inner
            .save_blob_with_hash(&self.scoped_id(id), blob, deps_hash)
            .await
    }

    async fn load_blob_with_hash(&self, id: &str, deps_hash: u128) -> Result<Option<EntityBlob>> {
        self.inner
            .load_blob_with_hash(&self.scoped_id(id), deps_hash)
            .await
    }

    async fn invalidate_with_hash(&self, id: &str, deps_hash: u128) -> Result<bool> {
        self.inner
            .invalidate_with_hash(&self.scoped_id(id), deps_hash)
            .await
    }

    fn on_compute_error(&self, id: &str, deps_hash: u128) {
        self.inner.on_compute_error(&self.scoped_id(id), deps_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StoreExt;
    use crate::backend::MemoryStore;
    use crate::test_support::load_text;

    #[test]
    fn prefix_is_normalized() {
        let prefix = |raw: &str| Scoped::new((), raw).prefix;

        assert_eq!(prefix("scope"), "scope/");
        assert_eq!(prefix("scope///"), "scope/");
        assert_eq!(prefix("/leading/slash"), "leading/slash/");
        assert_eq!(prefix("path//with///slashes"), "path/with/slashes/");
        assert_eq!(prefix("  /mixed///  "), "mixed/");
        assert_eq!(prefix("  scope  "), "  scope  /");
        assert_eq!(prefix("   /    "), "");
        assert_eq!(prefix(""), "");
    }

    #[tokio::test]
    async fn ids_are_prefixed_in_the_inner_store() {
        let inner = MemoryStore::new();
        let a = inner.clone().scoped("scope_a");
        let b = inner.clone().scoped("scope_b");

        a.save_text("key", "from_a", &()).await.unwrap();
        b.save_text("key", "from_b", &()).await.unwrap();

        assert_eq!(
            load_text(&inner, "scope_a/key", &()).await.as_deref(),
            Some("from_a")
        );
        assert_eq!(load_text(&b, "key", &()).await.as_deref(), Some("from_b"));
        assert_eq!(load_text(&a, "key_b", &()).await, None);
    }
}
