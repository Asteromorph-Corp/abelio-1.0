//! Resumable memoization: `cached_text!` and `cached_json!` persist results to a store and reuse
//! them while their `deps` hash is unchanged.

pub mod backend;
pub mod blob;
pub mod codec;
mod error;
mod hasher;
mod macros;
mod store;
#[cfg(test)]
mod test_support;
pub mod wrapper;

#[doc(hidden)]
pub mod __private;

pub use error::{Error, Result};
pub use store::{Store, StoreExt};

pub mod prelude {
    pub use crate::store::{Store, StoreExt};
    pub use crate::{cached_json, cached_text};
}
