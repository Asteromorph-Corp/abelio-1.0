//! Runtime support for the `cached_*!` macros; not a public API.

use std::marker::PhantomData;

pub use crate::hasher::stable_hash as stable_hash_of;

/// Created after a cache miss; unless disarmed it tells the store the computation failed or was
/// cancelled.
pub struct ComputeGuard<'a, S: crate::Store + ?Sized> {
    store: &'a S,
    id: &'a str,
    deps_hash: u128,
    active: bool,
}

impl<'a, S: crate::Store + ?Sized> ComputeGuard<'a, S> {
    pub fn new(store: &'a S, id: &'a str, deps_hash: u128, active: bool) -> Self {
        Self {
            store,
            id,
            deps_hash,
            active,
        }
    }

    pub fn disarm(mut self) {
        self.active = false;
    }
}

impl<S: crate::Store + ?Sized> Drop for ComputeGuard<'_, S> {
    fn drop(&mut self) {
        if self.active {
            self.store.on_compute_error(self.id, self.deps_hash);
        }
    }
}

// Autoref specialization: a block may evaluate to `T` or `Result<T, E>`.
// `Wrapper<Result<T, E>>` implements `ResultKindHigh`, which method resolution prefers over
// `ResultKindLow` on `&Wrapper<T>`.
pub struct TagResult;
pub struct TagItem;

pub struct Wrapper<T: ?Sized>(PhantomData<T>);

impl<T: ?Sized> Wrapper<T> {
    pub fn new(_: &T) -> Self {
        Wrapper(PhantomData)
    }
}

pub trait ResultKindHigh {
    fn result_kind(&self) -> TagResult;
}

pub trait ResultKindLow {
    fn result_kind(&self) -> TagItem;
}

impl<T, E> ResultKindHigh for Wrapper<Result<T, E>> {
    fn result_kind(&self) -> TagResult {
        TagResult
    }
}

impl<T: ?Sized> ResultKindLow for &Wrapper<T> {
    fn result_kind(&self) -> TagItem {
        TagItem
    }
}

pub trait ResultExecute<T> {
    type Output;
    fn execute_result(self, val: T) -> crate::Result<Self::Output>;
}

impl<T, E> ResultExecute<Result<T, E>> for TagResult
where
    E: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Output = T;
    fn execute_result(self, val: Result<T, E>) -> crate::Result<T> {
        val.map_err(crate::Error::user)
    }
}

impl<T> ResultExecute<T> for TagItem {
    type Output = T;
    fn execute_result(self, val: T) -> crate::Result<T> {
        Ok(val)
    }
}
