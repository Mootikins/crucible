//! The kiln index follows the disk without the broadcast bus.
//!
//! Each test changes the files of a kiln in one of the ways a daemon or a
//! user changes them, and then reads the index. An index row for a path that
//! is not on disk is a ghost: it lists, it answers a search, it holds
//! backlinks, and it opens nothing.

use super::*;
use crate::event_emitter::emit_event;
use crate::kiln_manager::request_scope;

/// The kiln of `server`, in the form the index keys it by.
fn kiln(server: &TestServer) -> PathBuf {
    server.kiln_path.canonicalize().expect("the kiln exists")
}

/// Whether the index holds a row for `rel`.
async fn indexed(km: &KilnManager, kiln: &Path, rel: &str) -> bool {
    let handle = km.get(kiln).await.expect("the kiln is open");
    handle
        .as_note_store()
        .get(rel, &request_scope(kiln))
        .await
        .expect("read the note store")
        .is_some()
}

/// The paths that the text index answers for `word`.
async fn text_hits(km: &KilnManager, kiln: &Path, word: &str) -> Vec<String> {
    let handle = km.get(kiln).await.expect("the kiln is open");
    handle
        .text
        .search(word, 10)
        .await
        .expect("search the text index")
        .into_iter()
        .map(|hit| hit.path)
        .collect()
}

/// Wait until `check` is true, or until `within` passes. Returns the last
/// answer of `check`.
async fn eventually<F, Fut>(within: std::time::Duration, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = tokio::time::Instant::now() + within;
    loop {
        if check().await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Write `files` into the kiln and index them.
async fn seed(server: &TestServer, files: &[(&str, &str)]) -> PathBuf {
    let kiln = kiln(server);
    for (rel, text) in files {
        let path = kiln.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, text).expect("write a note");
    }
    server
        .kiln_manager
        .open_and_process(&kiln, false)
        .await
        .expect("index the kiln");
    for (rel, _) in files {
        assert!(
            indexed(&server.kiln_manager, &kiln, rel).await,
            "precondition: {rel} is indexed"
        );
    }
    kiln
}

/// Wait until the watcher reported a change of `path` on the bus.
///
/// The watcher folds a create and a rename inside one debounce window into
/// one create of the new name. A test that renames a file it just wrote waits
/// for this first, so the rename is a rename to the watcher too.
async fn watcher_reported(bus: &mut broadcast::Receiver<SessionEventMessage>, path: &Path) {
    tokio::time::timeout(SETTLE, async {
        loop {
            match bus.recv().await {
                Ok(msg)
                    if msg.event == "file_changed"
                        && msg.data["path"].as_str() == path.to_str() =>
                {
                    break
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => panic!("the bus closed"),
            }
        }
    })
    .await
    .expect("the watcher reports the new note");
}

/// Long enough for the watcher's echo, and for a daemon under load.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(10);

/// `fs.move` of a folder moves every note under it in the index too.
#[tokio::test]
async fn a_folder_move_leaves_no_ghosts() {
    let server = TestServer::start().await;
    let kiln = seed(
        &server,
        &[
            ("folder/alpha.md", "# Alpha\n"),
            ("folder/sub/beta.md", "# Beta\n\n[[alpha]]\n"),
        ],
    )
    .await;

    let mut client = server.connect().await;
    let moved = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "fs.move",
            "params": {
                "root": kiln,
                "kind": "kiln",
                "from_rel": "folder",
                "to_rel": "moved",
            }
        }),
    )
    .await;
    assert!(moved["error"].is_null(), "fs.move failed: {moved}");

    // The move is queued before the reply, and the watcher never reports
    // the notes of a moved folder, so a settled index is the final answer.
    let km = server.kiln_manager.clone();
    km.settle_index().await;
    let rows = [
        indexed(&km, &kiln, "folder/alpha.md").await,
        indexed(&km, &kiln, "folder/sub/beta.md").await,
        indexed(&km, &kiln, "moved/alpha.md").await,
        indexed(&km, &kiln, "moved/sub/beta.md").await,
    ];
    server.shutdown().await;
    assert_eq!(
        rows,
        [false, false, true, true],
        "the index must follow the folder move: [old alpha, old beta, new alpha, new beta]"
    );
}

/// A rename by another program moves the note in the index.
#[tokio::test]
async fn an_external_rename_leaves_no_ghosts() {
    let server = TestServer::start().await;
    let mut bus = server.event_tx.subscribe();
    let kiln = seed(&server, &[("alpha.md", "# Alpha\n\nexternal rename\n")]).await;
    watcher_reported(&mut bus, &kiln.join("alpha.md")).await;

    std::fs::rename(kiln.join("alpha.md"), kiln.join("omega.md")).expect("rename");

    let km = server.kiln_manager.clone();
    let settled =
        eventually(SETTLE, || {
            let (km, kiln) = (km.clone(), kiln.clone());
            async move {
                !indexed(&km, &kiln, "alpha.md").await && indexed(&km, &kiln, "omega.md").await
            }
        })
        .await;
    let ghost = indexed(&km, &kiln, "alpha.md").await;
    server.shutdown().await;
    assert!(
        settled,
        "the index must follow an external rename; the old row is still indexed: {ghost}"
    );
}

/// `note.delete` takes the note out of the text search too.
#[tokio::test]
async fn note_delete_drops_the_text_index_row() {
    let server = TestServer::start().await;
    let kiln = seed(&server, &[("alpha.md", "# Alpha\n\nzephyrquartz\n")]).await;
    let km = server.kiln_manager.clone();
    assert_eq!(
        text_hits(&km, &kiln, "zephyrquartz").await,
        vec!["alpha.md".to_string()],
        "precondition: the text index finds the note"
    );

    let deleted = crate::server::kiln::handle_note_delete(
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "note.delete",
            "params": { "kiln": kiln, "path": "alpha.md" }
        }))
        .expect("a request"),
        &km,
    )
    .await;
    assert!(deleted.error.is_none(), "note.delete failed: {deleted:?}");

    let hits = text_hits(&km, &kiln, "zephyrquartz").await;
    server.shutdown().await;
    assert!(
        hits.is_empty(),
        "a deleted note must not answer a text search: {hits:?}"
    );
}

/// A file change that the bus drops still reaches the index.
///
/// The change enters as the watcher sends it, through the bridge. A
/// synchronous burst then overruns the bus before any receiver runs.
#[tokio::test]
async fn a_bus_lag_does_not_leave_the_index_stale() {
    let server = TestServer::start().await;
    let kiln = seed(&server, &[("alpha.md", "# Alpha\n\nfirstversion\n")]).await;
    let note = kiln.join("alpha.md");
    std::fs::write(&note, "# Alpha\n\nsecondversion\n").expect("edit the note");

    let bridge = server
        .kiln_manager
        .watcher_bridge(&kiln)
        .expect("the daemon's manager has a bus");
    bridge
        .emit(crucible_core::events::SessionEvent::internal(
            crucible_core::events::InternalSessionEvent::FileChanged {
                path: note.clone(),
                kind: crucible_core::events::FileChangeKind::Modified,
            },
        ))
        .await
        .expect("emit");
    for i in 0..EVENT_CHANNEL_CAPACITY + 64 {
        emit_event(
            &server.event_tx,
            SessionEventMessage::model_switched("lag-flood", format!("model-{i}"), "mock"),
        );
    }

    // Settled now, well before the watcher's own report of the edit.
    let km = server.kiln_manager.clone();
    km.settle_index().await;
    let hits = text_hits(&km, &kiln, "secondversion").await;
    server.shutdown().await;
    assert_eq!(
        hits,
        vec!["alpha.md".to_string()],
        "a change dropped by the bus must still reach the index"
    );
}

/// A daemon write reaches the index without the watcher's echo.
#[tokio::test]
async fn a_daemon_write_is_indexed_without_the_watcher() {
    let server = TestServer::start().await;
    let kiln = seed(&server, &[("alpha.md", "# Alpha\n")]).await;
    let target = kiln.join("written.md");

    let mut client = server.connect().await;
    let written = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "fs.write",
            "params": {
                "path": target,
                "operation": "put",
                "content": "# Written\n\nquillfeather\n",
            }
        }),
    )
    .await;
    assert_eq!(
        written["result"]["ok"],
        json!(true),
        "fs.write failed: {written}"
    );

    // The watcher reports the write a second later at the earliest, and a
    // settle waits only for the jobs queued before it.
    let km = server.kiln_manager.clone();
    km.settle_index().await;
    let hits = text_hits(&km, &kiln, "quillfeather").await;
    server.shutdown().await;
    assert_eq!(
        hits,
        vec!["written.md".to_string()],
        "a daemon write must be indexed without the watcher's echo"
    );
}

/// A folder renamed by another program moves each note under it.
///
/// The watcher reports the folder once and the notes under it not at all.
#[tokio::test]
async fn an_external_folder_rename_leaves_no_ghosts() {
    let server = TestServer::start().await;
    let mut bus = server.event_tx.subscribe();
    let kiln = seed(&server, &[("folder/alpha.md", "# Alpha\n")]).await;
    // A file written right after its new folder can land before the watcher
    // watches the folder, and then the watcher never sees it. Write it again
    // until the watcher does, so the folder is known to the watcher.
    let note = kiln.join("folder/alpha.md");
    let mut seen = false;
    for attempt in 0..10 {
        std::fs::write(&note, format!("# Alpha\n\nattempt {attempt}\n")).expect("write");
        let report = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            watcher_reported(&mut bus, &note),
        );
        if report.await.is_ok() {
            seen = true;
            break;
        }
    }
    assert!(
        seen,
        "the watcher never reported the note in the new folder"
    );

    std::fs::rename(kiln.join("folder"), kiln.join("renamed")).expect("rename");

    let km = server.kiln_manager.clone();
    let settled = eventually(SETTLE, || {
        let (km, kiln) = (km.clone(), kiln.clone());
        async move {
            !indexed(&km, &kiln, "folder/alpha.md").await
                && indexed(&km, &kiln, "renamed/alpha.md").await
        }
    })
    .await;
    server.shutdown().await;
    assert!(settled, "the index must follow an external folder rename");
}

/// One daemon write is one `file_changed`, not one from the daemon and a
/// second from the watcher's echo. A Lua `FileChanged` handler reads the same
/// bus, so it runs once.
#[tokio::test]
async fn a_daemon_write_is_announced_once() {
    let server = TestServer::start().await;
    let kiln = seed(&server, &[("alpha.md", "# Alpha\n")]).await;
    let target = kiln.join("written.md");
    let marker = kiln.join("marker.md");
    let mut bus = server.event_tx.subscribe();

    let mut client = server.connect().await;
    let written = rpc_call(
        &mut client,
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "fs.write",
            "params": { "path": target, "operation": "put", "content": "# Written\n" }
        }),
    )
    .await;
    assert_eq!(
        written["result"]["ok"],
        json!(true),
        "fs.write failed: {written}"
    );
    // A change by another program after the write. The watcher reports
    // changes in order, so its report of the marker comes after its report
    // of the write, if it makes one.
    std::fs::write(&marker, "# Marker\n").expect("write the marker");

    let mut announced = 0;
    tokio::time::timeout(SETTLE, async {
        loop {
            match bus.recv().await {
                Ok(msg) if msg.event == "file_changed" => {
                    let path = msg.data["path"].as_str().map(PathBuf::from);
                    if path.as_deref() == Some(target.as_path()) {
                        announced += 1;
                    }
                    if path.as_deref() == Some(marker.as_path()) {
                        break;
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::RecvError::Lagged(n)) => panic!("the test lagged {n}"),
                Err(broadcast::error::RecvError::Closed) => panic!("the bus closed"),
            }
        }
    })
    .await
    .expect("the watcher reports the marker");
    server.shutdown().await;
    assert_eq!(announced, 1, "one daemon write must be announced once");
}

/// `note.upsert` writes the text row and the block rows of the note, as the
/// pipeline does for a file.
#[tokio::test]
async fn note_upsert_writes_the_text_and_block_rows() {
    let server = TestServer::start().await;
    let kiln = seed(&server, &[("alpha.md", "# Alpha\n")]).await;
    let km = server.kiln_manager.clone();
    std::fs::write(kiln.join("upserted.md"), "# Upserted\n\nmarmalade harbor\n").expect("write");

    let mut record = km
        .get(&kiln)
        .await
        .expect("open")
        .as_note_store()
        .get("alpha.md", &request_scope(&kiln))
        .await
        .expect("read")
        .expect("the seeded row");
    record.path = "upserted.md".to_string();
    record.title = "Upserted".to_string();
    let upserted = crate::server::kiln::handle_note_upsert(
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "note.upsert",
            "params": { "kiln": kiln, "note": record }
        }))
        .expect("a request"),
        &km,
    )
    .await;
    assert!(upserted.error.is_none(), "note.upsert failed: {upserted:?}");

    let hits = text_hits(&km, &kiln, "marmalade").await;
    let blocks = km
        .get(&kiln)
        .await
        .expect("open")
        .as_block_store()
        .blocks_for_note("upserted.md")
        .await
        .expect("read the block rows");
    server.shutdown().await;
    assert_eq!(hits, vec!["upserted.md".to_string()], "the text row");
    assert!(
        blocks.iter().any(|block| block.text.contains("marmalade")),
        "the block rows: {blocks:?}"
    );
}

/// The watcher's rescan signal reads the kiln again. The watcher sends it
/// when it lost events, so the change below has no event of its own.
#[tokio::test]
async fn a_watcher_rescan_reindexes_the_kiln() {
    let server = TestServer::start().await;
    let kiln = seed(&server, &[("alpha.md", "# Alpha\n\nbeforerescan\n")]).await;
    std::fs::write(kiln.join("alpha.md"), "# Alpha\n\nafterrescan\n").expect("edit");

    let bridge = server
        .kiln_manager
        .watcher_bridge(&kiln)
        .expect("the daemon's manager has a bus");
    bridge
        .emit(crucible_core::events::SessionEvent::Custom {
            name: crate::watch::WATCH_RESCAN_EVENT.to_string(),
            payload: serde_json::Value::Null,
        })
        .await
        .expect("emit");

    let km = server.kiln_manager.clone();
    km.settle_index().await;
    let hits = text_hits(&km, &kiln, "afterrescan").await;
    server.shutdown().await;
    assert_eq!(hits, vec!["alpha.md".to_string()]);
}

/// `note.upsert` reads the text of a file only inside the kiln. The path is
/// the caller's, and a path out of the kiln must not copy another file into
/// the search index.
#[tokio::test]
async fn note_upsert_reads_no_file_outside_the_kiln() {
    let server = TestServer::start().await;
    let kiln = seed(&server, &[("alpha.md", "# Alpha\n")]).await;
    let km = server.kiln_manager.clone();
    let outside = kiln.parent().expect("a parent").join("outside.md");
    std::fs::write(&outside, "# Outside\n\nsapphirecipher\n").expect("write");

    let mut record = km
        .get(&kiln)
        .await
        .expect("open")
        .as_note_store()
        .get("alpha.md", &request_scope(&kiln))
        .await
        .expect("read")
        .expect("the seeded row");
    record.path = "../outside.md".to_string();
    let _ = crate::server::kiln::handle_note_upsert(
        serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "note.upsert",
            "params": { "kiln": kiln, "note": record }
        }))
        .expect("a request"),
        &km,
    )
    .await;

    let hits = text_hits(&km, &kiln, "sapphirecipher").await;
    server.shutdown().await;
    assert!(
        hits.is_empty(),
        "a file outside the kiln was indexed: {hits:?}"
    );
}
