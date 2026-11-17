use std::hash::Hash;

use crate::hasher::stable_hash;
use crate::{Store, StoreExt};

pub async fn load_text<S: Store, H: Hash>(store: &S, id: &str, deps: &H) -> Option<String> {
    store
        .load_text_with_hash(id, stable_hash(deps))
        .await
        .unwrap()
}
