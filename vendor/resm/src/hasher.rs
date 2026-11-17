//! Stable 128-bit hashing of dependency values.
//!
//! The salt and the algorithm are part of the persisted key format: changing either orphans every
//! existing cache entry.

use std::hash::Hash;

use xxhash_rust::xxh3::Xxh3;

const SALT: &str = "resm-v0.1";

pub fn stable_hash<H: Hash + ?Sized>(value: &H) -> u128 {
    let mut hasher = Xxh3::new();
    SALT.hash(&mut hasher);
    value.hash(&mut hasher);
    hasher.digest128()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins the persisted key format: changing it orphans existing caches.
    #[test]
    fn matches_persisted_key_format() {
        assert_eq!(
            stable_hash(&("v1", 1u32)),
            0x1000063c5c112d3c058a2688db25e19f
        );
        assert_eq!(stable_hash(&"v1"), 0x43556cabf82f2332352b94c55364fbb5);
        assert_eq!(stable_hash(&()), 0x436e615b49876f58d47ed1f2b29e0f3b);
        assert_eq!(
            stable_hash(&("key", 42i32, true)),
            0x9463ff45ed79e22cf3612e12264dfe70
        );
        assert_eq!(
            stable_hash(&vec!["a".to_string(), "b".to_string()]),
            0x813f124584114d62d3c1cfc5923e2118
        );
        assert_eq!(stable_hash(&Some(7u64)), 0x325a79d17951497018c189196f840a4f);
    }
}
