/// Shared body of `cached_text!` and `cached_json!`.
///
/// Loads `$id` for `$deps`, otherwise evaluates `$block` and saves the result.
/// The block may `.await`, and may evaluate to `T` or to `Result<T, E>`, in which case `Err` is
/// returned as [`Error::User`](crate::Error::User) and nothing is saved.
#[doc(hidden)]
#[macro_export]
macro_rules! cached_impl {
    ($store:expr, $id:expr, $deps:expr, $load:ident, $save:ident, $force:expr, $block:block) => {{
        use $crate::StoreExt;

        async {
            let __resm_store = &$store;
            let __resm_id: std::borrow::Cow<'_, str> = ($id).into();
            let __resm_deps_hash = $crate::__private::stable_hash_of($deps);
            let __resm_force: bool = $force;

            if !__resm_force {
                if let Some(cached) = __resm_store.$load(&*__resm_id, __resm_deps_hash).await? {
                    return Ok(cached);
                }
            }

            let __resm_guard = $crate::__private::ComputeGuard::new(
                __resm_store,
                &*__resm_id,
                __resm_deps_hash,
                !__resm_force,
            );

            let raw_item = async { $block }.await;

            let value = {
                // Each expansion uses only one of the two kind traits.
                #[allow(unused_imports)]
                use $crate::__private::{ResultExecute, ResultKindHigh, ResultKindLow, Wrapper};
                let wrapper = Wrapper::new(&raw_item);
                let tag = (&wrapper).result_kind();
                tag.execute_result(raw_item)?
            };

            __resm_store
                .$save(&*__resm_id, &value, __resm_deps_hash)
                .await?;
            __resm_guard.disarm();
            Ok::<_, $crate::Error>(value)
        }
        .await
    }};
}

/// Caches a `String`, stored as `<id>.md` by `FileStore`.
#[macro_export]
macro_rules! cached_text {
    ($store:expr, $id:expr, deps: $deps:expr, force: $force:expr, $block:block) => {{
        $crate::cached_impl!($store, $id, $deps, load_text_with_hash, save_text_with_hash, $force, $block)
    }};
    ($store:expr, $id:expr, deps: $deps:expr, $block:block) => {{
        $crate::cached_text!($store, $id, deps: $deps, force: false, $block)
    }};
}

/// Caches a serde value as pretty-printed JSON, stored as `<id>.json` by `FileStore`.
#[macro_export]
macro_rules! cached_json {
    ($store:expr, $id:expr, deps: $deps:expr, force: $force:expr, $block:block) => {{
        $crate::cached_impl!($store, $id, $deps, load_json_with_hash, save_json_with_hash, $force, $block)
    }};
    ($store:expr, $id:expr, deps: $deps:expr, $block:block) => {{
        $crate::cached_json!($store, $id, deps: $deps, force: false, $block)
    }};
}
