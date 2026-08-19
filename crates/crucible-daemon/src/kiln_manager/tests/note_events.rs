//! The note lifecycle events the kiln manager puts on the broadcast bus.
//!
//! The note store has always returned `NoteCreated`/`NoteModified`/`NoteDeleted`
//! and the pipeline has always dropped them, so `crucible.on("note:created", …)`
//! had nothing to fire on. These assert the manager broadcasts them under the
//! names `crate::event_map` declares — and that the bulk indexer stays quiet
//! for the files it indexes, while the reconciliation sweep in front of it does
//! not.

use super::*;

/// A manager wired to a bus, plus the receiving end.
fn km_with_bus() -> (
    KilnManager,
    broadcast::Receiver<crate::protocol::SessionEventMessage>,
) {
    let (tx, rx) = broadcast::channel(256);
    let km = KilnManager::with_event_tx(
        tx,
        None,
        crucible_core::config::default_max_precognition_chars(),
    );
    (km, rx)
}

/// Every `note:*` message waiting on the bus, as `(event, path)` pairs.
/// Other system events (`classification_required`) are not this test's
/// business.
fn note_events(
    rx: &mut broadcast::Receiver<crate::protocol::SessionEventMessage>,
) -> Vec<(String, String)> {
    let mut seen = Vec::new();
    while let Ok(msg) = rx.try_recv() {
        if msg.event.starts_with("note:") {
            let key = if msg.event == "note:renamed" {
                "to"
            } else {
                "path"
            };
            seen.push((
                msg.event.clone(),
                msg.data
                    .get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            ));
        }
    }
    seen
}

#[tokio::test]
async fn indexing_a_note_announces_it_created_then_modified() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    let note = kiln.join("alpha.md");
    std::fs::write(&note, "# Alpha\n\nfirst\n").unwrap();

    let (km, mut rx) = km_with_bus();
    assert!(km.process_file(kiln, &note).await.unwrap());
    assert_eq!(
        note_events(&mut rx),
        vec![("note:created".to_string(), "alpha.md".to_string())],
        "a path the index had not seen must announce note:created"
    );

    // Content change, so change detection does not skip it.
    std::fs::write(&note, "# Alpha\n\nsecond\n").unwrap();
    assert!(km.process_file(kiln, &note).await.unwrap());
    assert_eq!(
        note_events(&mut rx),
        vec![("note:modified".to_string(), "alpha.md".to_string())],
        "the second write is a modification, not a creation"
    );
}

#[tokio::test]
async fn an_unchanged_note_announces_nothing() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    let note = kiln.join("alpha.md");
    std::fs::write(&note, "# Alpha\n").unwrap();

    let (km, mut rx) = km_with_bus();
    km.process_file(kiln, &note).await.unwrap();
    // Asserted, not drained: without this the test reads the same on code that
    // announces nothing at all, which is what it is supposed to rule out.
    assert_eq!(
        note_events(&mut rx),
        vec![("note:created".to_string(), "alpha.md".to_string())],
        "sanity: the first pass must announce, or the silence below proves nothing"
    );

    assert!(
        !km.process_file(kiln, &note).await.unwrap(),
        "sanity: change detection should skip the second pass"
    );
    assert!(
        note_events(&mut rx).is_empty(),
        "a skipped file wrote nothing, so it must announce nothing"
    );
}

#[tokio::test]
async fn deleting_a_note_announces_note_deleted() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    let note = kiln.join("alpha.md");
    std::fs::write(&note, "# Alpha\n").unwrap();

    let (km, mut rx) = km_with_bus();
    km.process_file(kiln, &note).await.unwrap();
    let _ = note_events(&mut rx);

    std::fs::remove_file(&note).unwrap();
    assert!(km.handle_file_deleted(kiln, &note).await.unwrap());
    assert_eq!(
        note_events(&mut rx),
        vec![("note:deleted".to_string(), "alpha.md".to_string())]
    );
}

/// A full kiln index announces nothing for the files it *indexes*, and one
/// `note:deleted` for each index row whose file has gone.
///
/// The bulk indexer stays silent on purpose: the first pass over a large kiln
/// would otherwise put one broadcast message per note on the bus to say nothing
/// the `process_complete` for the same run does not already say.
///
/// The reconciliation sweep in front of it is the exception, and this test
/// exists in this shape because the first version of it did not have one. It
/// indexed two present files, asserted the bus was empty, and passed — while
/// `open_and_process` was in fact emitting a `note:deleted` per ghost row, so
/// the rule the test was written to pin was already false and the test could
/// not say so. A kiln with no ghosts discriminates nothing here.
#[tokio::test]
async fn a_full_kiln_index_announces_only_the_rows_it_reconciles() {
    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    std::fs::write(kiln.join("alpha.md"), "# Alpha\n").unwrap();
    std::fs::write(kiln.join("beta.md"), "# Beta\n").unwrap();
    std::fs::write(kiln.join("ghost.md"), "# Ghost\n").unwrap();

    let (km, mut rx) = km_with_bus();
    let (_discovered, processed, _skipped, errors) =
        km.open_and_process(kiln, false).await.unwrap();
    assert_eq!(processed, 3, "sanity: all three notes indexed {errors:?}");
    let _ = note_events(&mut rx);

    // Deleted behind the daemon's back — a `git rm`, a branch checkout. Only
    // the reconciliation sweep ever notices.
    std::fs::remove_file(kiln.join("ghost.md")).unwrap();

    // `force`, so alpha and beta are genuinely reprocessed rather than skipped
    // by change detection: the silence below has to be the bulk indexer's
    // choice, not the absence of work.
    let (_discovered, processed, _skipped, errors) = km.open_and_process(kiln, true).await.unwrap();
    assert_eq!(
        processed, 2,
        "sanity: both survivors reprocessed {errors:?}"
    );

    assert_eq!(
        note_events(&mut rx),
        vec![("note:deleted".to_string(), "ghost.md".to_string())],
        "a full index announces nothing for the files it indexes, and exactly \
         one note:deleted for the row it reconciled away"
    );
}

/// The two RPC note handlers write through `NoteStore` directly rather than
/// through the pipeline, so they hold the only copy of the events that write
/// produced. Both bound them and threw them away — `handle_note_upsert`
/// reported `events_count` and dropped the vec, `handle_note_delete` bound
/// `Ok(_event)` — so a note written over RPC fired no `note:created` while the
/// identical note written by the file watcher did.
///
/// This calls the **handlers**, not the store. An earlier draft of this test
/// called `km.announce()` itself and asserted the bus received it, which
/// passes with the handlers unchanged — it proves the bus works, not that the
/// handler uses it. That is the same defect the adversarial review found in
/// two of this item's other tests, so it is named here to stop it recurring:
/// a test for "the caller does X" must invoke the caller.
#[tokio::test]
async fn the_rpc_note_handlers_announce_what_they_wrote() {
    use crate::server::kiln::{handle_note_delete, handle_note_upsert};
    use crucible_core::protocol::rpc::Request;

    let tmp = TempDir::new().unwrap();
    let kiln = tmp.path();
    let (km, mut rx) = km_with_bus();
    let km = std::sync::Arc::new(km);

    let request = |method: &str, params: serde_json::Value| Request {
        jsonrpc: "2.0".to_string(),
        id: Some(crucible_core::protocol::rpc::RequestId::Number(1)),
        method: method.to_string(),
        params,
    };

    handle_note_upsert(
        request(
            "note.upsert",
            serde_json::json!({
                "kiln": kiln.to_string_lossy(),
                // Serialized as `NoteRecord` deserializes it — the handler
                // parses the whole record, so a hand-written subset silently
                // fails INVALID_PARAMS and the test would assert nothing.
                "note": serde_json::to_value(crucible_core::storage::NoteRecord::new(
                    "alpha.md",
                    crucible_core::parser::types::BlockHash::zero(),
                )).unwrap()
            }),
        ),
        &km,
    )
    .await;

    assert_eq!(
        note_events(&mut rx),
        vec![("note:created".to_string(), "alpha.md".to_string())],
        "note.upsert must announce; it used to report events_count and drop the vec"
    );

    handle_note_delete(
        request(
            "note.delete",
            serde_json::json!({ "kiln": kiln.to_string_lossy(), "path": "alpha.md" }),
        ),
        &km,
    )
    .await;

    assert_eq!(
        note_events(&mut rx),
        vec![("note:deleted".to_string(), "alpha.md".to_string())],
        "note.delete must announce; it used to bind the event to `_`"
    );
}
