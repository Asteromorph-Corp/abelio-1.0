use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use resm::backend::{FileStore, MemoryStore, RedbStore};
use resm::codec::JsonCodec;
use resm::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Item {
    name: String,
    value: i32,
}

fn item(value: i32) -> Item {
    Item {
        name: "item".into(),
        value,
    }
}

#[tokio::test]
async fn cached_text_reuses_until_deps_change_or_forced() {
    let store = MemoryStore::new();
    let computed = AtomicUsize::new(0);
    let next = || format!("computed-{}", computed.fetch_add(1, Ordering::SeqCst) + 1);

    let first = cached_text!(store, "key", deps: &"v1", { next() }).unwrap();
    let again = cached_text!(store, "key", deps: &"v1", { next() }).unwrap();
    assert_eq!(
        (first.as_str(), again.as_str()),
        ("computed-1", "computed-1")
    );

    let new_deps = cached_text!(store, "key", deps: &"v2", { next() }).unwrap();
    assert_eq!(new_deps, "computed-2");

    let forced = cached_text!(store, "key", deps: &"v2", force: true, { next() }).unwrap();
    let after_force = cached_text!(store, "key", deps: &"v2", { next() }).unwrap();
    assert_eq!(
        (forced.as_str(), after_force.as_str()),
        ("computed-3", "computed-3")
    );
}

#[tokio::test]
async fn cached_json_round_trips_and_reuses() {
    let store = MemoryStore::new();

    let first: Item = cached_json!(store, "item", deps: &(), { item(1) }).unwrap();
    let again: Item = cached_json!(store, "item", deps: &(), { item(2) }).unwrap();

    assert_eq!(first, item(1));
    assert_eq!(again, item(1));
}

#[tokio::test]
async fn block_may_await_and_use_question_mark() {
    let store = MemoryStore::new();

    let value: String = cached_text!(store, "async", deps: &(), {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let n: i32 = "42".parse()?;
        Ok::<_, std::num::ParseIntError>(n.to_string())
    })
    .unwrap();

    assert_eq!(value, "42");
}

#[tokio::test]
async fn failed_block_is_reported_and_not_cached() {
    let store = MemoryStore::new();

    let err = cached_text!(store, "fails", deps: &(), {
        let n: i32 = "not a number".parse()?;
        Ok::<_, std::num::ParseIntError>(n.to_string())
    })
    .unwrap_err();
    assert!(matches!(err, resm::Error::User(_)));

    let value = cached_text!(store, "fails", deps: &(), { "recovered".to_string() }).unwrap();
    assert_eq!(value, "recovered");
}

#[tokio::test]
async fn block_returning_result_is_unwrapped() {
    let store = MemoryStore::new();

    let ok: Item =
        cached_json!(store, "ok", deps: &(), { Ok::<_, std::io::Error>(item(7)) }).unwrap();
    assert_eq!(ok, item(7));

    let err = cached_json!(store, "err", deps: &(), {
        Err::<Item, _>(std::io::Error::other("boom"))
    })
    .unwrap_err();
    assert!(matches!(err, resm::Error::User(_)));
}

#[tokio::test]
async fn undecodable_entry_is_recomputed() {
    let store = MemoryStore::new();
    let deps = "v1";

    store
        .save_text("item", "this is not valid JSON {{{", &deps)
        .await
        .unwrap();

    let value: Item = cached_json!(store, "item", deps: &deps, { item(42) }).unwrap();
    let cached: Item = cached_json!(store, "item", deps: &deps, { item(99) }).unwrap();

    assert_eq!(value, item(42));
    assert_eq!(cached, item(42));
}

#[tokio::test]
async fn save_with_writes_pretty_json_to_the_file_view() {
    let dir = tempfile::tempdir().unwrap();
    let store = FileStore::new(dir.path()).unwrap();

    store
        .save_with("history", &item(3), &(), &JsonCodec::pretty())
        .await
        .unwrap();

    let written = std::fs::read_to_string(dir.path().join("history.json")).unwrap();
    assert_eq!(written, "{\n  \"name\": \"item\",\n  \"value\": 3\n}");
}

// The store stack the pipelines build: redb cache mirrored into a file view, shared by reference
// across scoped tasks.
#[tokio::test]
async fn pipeline_store_stack_caches_and_mirrors() {
    async fn step(store: &(impl Store + Clone)) -> resm::Result<String> {
        let scoped = store.clone().scoped("task/01");
        cached_text!(scoped, "report", deps: &"v1", { "# Report".to_string() })
    }

    let dir = tempfile::tempdir().unwrap();
    let cache = RedbStore::new(dir.path().join("db")).unwrap();
    let view = FileStore::new(dir.path().join("artifacts"))
        .unwrap()
        .coalesced();
    let store = cache.fanout(view).into_arced();

    assert_eq!(step(&store).await.unwrap(), "# Report");
    assert_eq!(step(&store).await.unwrap(), "# Report");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("artifacts/task/01/report.md")).unwrap(),
        "# Report"
    );
}

#[tokio::test]
async fn macro_evaluates_its_id_once() {
    let store = MemoryStore::new();
    let evaluated = Arc::new(AtomicUsize::new(0));
    let make_id = || {
        evaluated.fetch_add(1, Ordering::SeqCst);
        "key".to_string()
    };

    cached_text!(store, make_id(), deps: &(), { "value".to_string() }).unwrap();

    assert_eq!(evaluated.load(Ordering::SeqCst), 1);
}
