//! Search/Kiln Route Contract Tests (with mock daemon)

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{
    build_mock_state, build_test_app, start_mock_daemon, start_mock_daemon_with_kilns,
};

#[tokio::test]
async fn list_kilns_returns_200_with_array() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/kilns")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["kilns"].is_array(), "Response must have 'kilns' array");
}

#[tokio::test]
async fn list_notes_requires_kiln_query_param() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // Missing required 'kiln' query parameter
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/notes")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 400/422 for missing required query parameters
    assert!(
        response.status().is_client_error(),
        "Missing kiln param should return client error, got: {}",
        response.status()
    );
}

#[tokio::test]
async fn list_notes_with_kiln_returns_200() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/notes?kiln=/tmp/test-kiln")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(json["notes"].is_array(), "Response must have 'notes' array");
}

#[tokio::test]
async fn kiln_graph_returns_200_with_notes_and_links() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/kiln/graph?kiln=/tmp/test-kiln")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Route returns the daemon's kiln.graph result verbatim.
    assert!(json["notes"].is_array(), "Response must have 'notes' array");
    assert!(json["links"].is_array(), "Response must have 'links' array");
    let links = json["links"].as_array().unwrap();
    assert!(links.iter().any(|l| l["resolved"] == true));
    assert!(links.iter().any(|l| l["resolved"] == false));
}

#[tokio::test]
async fn kiln_graph_requires_kiln_query_param() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/kiln/graph")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status().is_client_error(),
        "Missing kiln param should return client error, got: {}",
        response.status()
    );
}

#[tokio::test]
async fn search_vectors_returns_200_with_results() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/search/vectors")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kiln": "/tmp/test-kiln",
                        "vector": [0.1, 0.2, 0.3],
                        "limit": 5
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["results"].is_array(),
        "Response must have 'results' array"
    );
}

#[tokio::test]
async fn search_semantic_returns_200_with_results() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/search/semantic")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kiln": "/tmp/test-kiln",
                        "query": "how do links work",
                        "limit": 5
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        json["results"].is_array(),
        "Response must have 'results' array"
    );
}

/// The daemon answers with one row per block. The panel shows notes, so the
/// route keeps one row per note: the best block, with its span.
#[tokio::test]
async fn search_semantic_keeps_one_row_per_note_with_its_best_block() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/search/semantic")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "kiln": "/tmp/test-kiln",
                        "query": "where does knowledge go",
                        "limit": 5
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let results = json["results"].as_array().expect("results array");
    let rel_paths: Vec<&str> = results
        .iter()
        .map(|r| r["rel_path"].as_str().unwrap())
        .collect();
    assert_eq!(
        rel_paths,
        vec!["notes/kilns.md", "notes/projects.md"],
        "one row per note, best first: {json}"
    );
    assert_eq!(results[0]["score"], 0.91, "the kept row is the best block");
    assert_eq!(results[0]["block"]["span_start"], 40);
    assert_eq!(results[0]["snippet"], "A kiln is where knowledge goes.");
}

#[tokio::test]
async fn search_semantic_blank_query_returns_empty() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/search/semantic")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "kiln": "/tmp/test-kiln", "query": "   " }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["results"].as_array().map(|a| a.len()), Some(0));
}

// ============================================================================
// GET /api/backlinks
// ============================================================================

async fn get_json(app: axum::Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    (status, json)
}

#[tokio::test]
async fn backlinks_requires_kiln_and_note_params() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let (status, _) = get_json(app, "/api/backlinks").await;
    assert!(
        status.is_client_error(),
        "Missing params should return client error, got: {status}"
    );
}

#[tokio::test]
async fn backlinks_unknown_note_returns_404() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let (status, _) = get_json(app, "/api/backlinks?kiln=/tmp/test-kiln&note=missing").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn backlinks_rejects_path_traversal_in_note() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    let (status, _) = get_json(app, "/api/backlinks?kiln=/tmp/test-kiln&note=../etc/passwd").await;
    assert!(
        status.is_client_error(),
        "Traversal in note param should return client error, got: {status}"
    );
}

#[tokio::test]
async fn backlinks_returns_linked_and_filtered_unlinked() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // Real kiln dir so the route can read the focused note's content for
    // the suggest_links (unlinked mentions) pass.
    let kiln = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(kiln.path().join("notes")).unwrap();
    std::fs::write(
        kiln.path().join("notes/focused.md"),
        "Other Note is mentioned here. Focused Note names itself.",
    )
    .unwrap();

    let uri = format!(
        "/api/backlinks?kiln={}&note=focused",
        kiln.path().to_string_lossy()
    );
    let (status, json) = get_json(app, &uri).await;
    assert_eq!(status, StatusCode::OK);

    // Focused note metadata with an absolute path for the editor.
    assert_eq!(json["note"]["title"], "Focused Note");
    assert_eq!(json["note"]["path"], "notes/focused.md");
    let abs = json["note"]["abs_path"].as_str().unwrap();
    assert!(abs.starts_with(kiln.path().to_str().unwrap()));

    // Linked mentions carry both kiln-relative and absolute paths.
    let linked = json["linked"].as_array().unwrap();
    assert_eq!(linked.len(), 1);
    assert_eq!(linked[0]["title"], "Linker Note");
    assert_eq!(linked[0]["path"], "notes/linker.md");
    assert!(linked[0]["abs_path"]
        .as_str()
        .unwrap()
        .ends_with("notes/linker.md"));

    // The mock returns two suggestions; the self-mention ("Focused Note")
    // must be filtered, leaving only "Other Note".
    let unlinked = json["unlinked"].as_array().unwrap();
    assert_eq!(unlinked.len(), 1);
    assert_eq!(unlinked[0]["target"], "Other Note");
    assert_eq!(unlinked[0]["offset"], 0);
}

#[tokio::test]
async fn raw_file_rejects_path_traversal() {
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));
    let (status, _) = get_json(app, "/api/file/raw?path=/x/../../etc/passwd").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn raw_file_outside_any_root_is_404() {
    // Mock daemon reports no kilns and no projects, so any real path is
    // outside every root and must be refused (fail-closed).
    let (_mock, client) = start_mock_daemon().await;
    let app = build_test_app(build_mock_state(client));
    let (status, _) = get_json(app, "/api/file/raw?path=/etc/hostname").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn backlinks_missing_note_file_degrades_to_empty_unlinked() {
    let (_mock, client) = start_mock_daemon().await;
    let state = build_mock_state(client);
    let app = build_test_app(state);

    // Kiln path exists as a query param only — notes/focused.md is not on
    // disk, so the unlinked pass degrades to [] instead of failing.
    let (status, json) = get_json(
        app,
        "/api/backlinks?kiln=/tmp/nonexistent-kiln&note=focused",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["linked"].as_array().unwrap().len(), 1);
    assert_eq!(json["unlinked"].as_array().unwrap().len(), 0);
}

/// `PATCH /api/kiln/file` against a real file inside a kiln the mock daemon
/// lists. Answers the status and the JSON body.
async fn patch_file(app: axum::Router, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/kiln/file")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A kiln directory that holds one note. Answers the directory, the note's
/// path, and the hash of the text on disk.
async fn kiln_with_note(text: &str) -> (tempfile::TempDir, std::path::PathBuf, String) {
    let kiln = tempfile::tempdir().unwrap();
    let note = kiln.path().join("Note.md");
    tokio::fs::write(&note, text).await.unwrap();
    let hash = crucible_core::note_edit::disk_hash(text);
    (kiln, note, hash)
}

/// The base a caller names must still be the text on disk, even when every
/// anchor applies. Otherwise the caller adopts the answered hash over a
/// buffer that lacks another writer's change, and its next whole save
/// removes that change without a refusal.
#[tokio::test]
async fn a_patch_with_a_stale_base_is_refused_even_when_its_anchors_apply() {
    let before = "- [ ] task\n\nagent paragraph\n";
    let (kiln, note, current_hash) = kiln_with_note(before).await;
    let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
    let app = build_test_app(build_mock_state(client));

    let (status, body) = patch_file(
        app,
        json!({
            "path": note,
            "base_hash": "0".repeat(64),
            "edits": [{ "expect": "- [ ] task", "replace": "- [x] task" }],
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["ok"], false);
    assert_eq!(body["stale_base"], true);
    assert_eq!(body["failed"], json!([]));
    assert_eq!(body["current_hash"], current_hash);
    assert_eq!(
        tokio::fs::read_to_string(&note).await.unwrap(),
        before,
        "the file is untouched"
    );
}

/// The outbox replay sends no base, by design: the edit anchors on the
/// note's current text, whatever it is now.
#[tokio::test]
async fn a_patch_with_no_base_applies_against_the_current_text() {
    let before = "- [ ] task\n\nagent paragraph\n";
    let after = "- [x] task\n\nagent paragraph\n";
    let (kiln, note, _) = kiln_with_note(before).await;
    let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
    let app = build_test_app(build_mock_state(client));

    let (status, body) = patch_file(
        app,
        json!({
            "path": note,
            "edits": [{ "expect": "- [ ] task", "replace": "- [x] task" }],
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], true);
    assert_eq!(
        body["content_hash"],
        crucible_core::note_edit::disk_hash(after)
    );
    assert_eq!(tokio::fs::read_to_string(&note).await.unwrap(), after);
}

/// `PUT /api/kiln/file` against a real file inside a kiln the mock daemon
/// lists. Answers the status and the JSON body.
async fn put_file(app: axum::Router, body: Value) -> (StatusCode, Value) {
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/kiln/file")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// A caller that names a base but cannot say what that base held has nothing
/// to merge from, so the refusal stands.
#[tokio::test]
async fn a_put_with_a_stale_base_and_no_base_text_is_refused() {
    let disk = "A\nB2\nC\n";
    let (kiln, note, current_hash) = kiln_with_note(disk).await;
    let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
    let app = build_test_app(build_mock_state(client));

    let (status, body) = put_file(
        app,
        json!({
            "path": note,
            "content": "A\nB\nC\nD\n",
            "base_hash": crucible_core::note_edit::disk_hash("A\nB\nC\n"),
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["ok"], false);
    assert_eq!(body["current_hash"], current_hash);
    assert_eq!(
        tokio::fs::read_to_string(&note).await.unwrap(),
        disk,
        "the file is untouched"
    );
}

/// The whole point: two writers changed different lines, so nobody loses an
/// edit and nobody is asked a question.
#[tokio::test]
async fn a_put_with_a_stale_base_and_base_text_merges_and_writes() {
    let base = "A\nB\nC\n";
    let disk = "A\nB2\nC\n";
    let merged = "A\nB2\nC\nD\n";
    let (kiln, note, _) = kiln_with_note(disk).await;
    let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
    let app = build_test_app(build_mock_state(client));

    let (status, body) = put_file(
        app,
        json!({
            "path": note,
            "content": "A\nB\nC\nD\n",
            "base_hash": crucible_core::note_edit::disk_hash(base),
            "base_text": base,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["merged"], true);
    assert_eq!(body["content"], merged);
    assert_eq!(
        body["content_hash"],
        crucible_core::note_edit::disk_hash(merged)
    );
    assert_eq!(tokio::fs::read_to_string(&note).await.unwrap(), merged);
}

/// Both writers changed one line. The merge never decides that for them: the
/// file is untouched and the answer carries both texts.
#[tokio::test]
async fn a_put_whose_merge_has_regions_writes_nothing_and_answers_them() {
    let base = "A\nB\nC\n";
    let disk = "A\ntheirs\nC\n";
    let (kiln, note, current_hash) = kiln_with_note(disk).await;
    let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
    let app = build_test_app(build_mock_state(client));

    let (status, body) = put_file(
        app,
        json!({
            "path": note,
            "content": "A\nours\nC\n",
            "base_hash": crucible_core::note_edit::disk_hash(base),
            "base_text": base,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["ok"], false);
    assert_eq!(body["stale_base"], true);
    assert_eq!(body["current_hash"], current_hash);
    assert_eq!(body["current_content"], disk);
    assert_eq!(body["merged_content"], "A\nours\nC\n");
    assert_eq!(body["regions"].as_array().unwrap().len(), 1);
    assert_eq!(body["regions"][0]["ours"], "ours\n");
    assert_eq!(body["regions"][0]["theirs"], "theirs\n");
    assert_eq!(body["regions"][0]["base"], "B\n");
    assert_eq!(
        tokio::fs::read_to_string(&note).await.unwrap(),
        disk,
        "the file is untouched"
    );
}

/// A base text that is not the base the caller named is not a base at all.
/// Merging from it would invent a change neither writer made.
#[tokio::test]
async fn a_base_text_that_does_not_hash_to_the_base_is_refused_with_422() {
    let disk = "A\nB2\nC\n";
    let (kiln, note, _) = kiln_with_note(disk).await;
    let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
    let app = build_test_app(build_mock_state(client));

    let (status, body) = put_file(
        app,
        json!({
            "path": note,
            "content": "A\nB\nC\nD\n",
            "base_hash": crucible_core::note_edit::disk_hash("A\nB\nC\n"),
            "base_text": "not the base at all\n",
        }),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        tokio::fs::read_to_string(&note).await.unwrap(),
        disk,
        "the file is untouched"
    );
}

/// Read, merge and write are one critical section per note. Without the lock
/// the second writer merges against the text the first one is about to
/// replace, and the first writer's change is gone with no refusal.
///
/// The runtime has ONE blocking thread, busy when the two requests start, so
/// every filesystem call queues in FIFO order: unlocked, both reads land
/// before either write.
#[test]
fn two_concurrent_puts_to_one_note_serialize() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();

    runtime.block_on(async {
        let base = "A\nB\nC\nD\nE\n";
        let disk = "A\nB\nC\nD\nE2\n";
        let (kiln, note, _) = kiln_with_note(disk).await;
        let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
        let app = build_test_app(build_mock_state(client));

        let body = |content: &str| {
            json!({
                "path": note,
                "content": content,
                "base_hash": crucible_core::note_edit::disk_hash(base),
                "base_text": base,
            })
        };

        // Hold the one blocking thread so both requests reach their first
        // filesystem call before either of them can finish one.
        let blocker = tokio::task::spawn_blocking(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
        });

        let (first, second) = tokio::join!(
            put_file(app.clone(), body("A1\nB\nC\nD\nE\n")),
            put_file(app.clone(), body("A\nB\nC1\nD\nE\n")),
        );
        blocker.await.unwrap();

        assert_eq!(first.0, StatusCode::OK, "{}", first.1);
        assert_eq!(second.0, StatusCode::OK, "{}", second.1);
        assert_eq!(
            tokio::fs::read_to_string(&note).await.unwrap(),
            "A1\nB\nC1\nD\nE2\n",
            "both writers' changes survive, and so does the disk's"
        );
    });
}

/// PUT and PATCH write the same note, so they take the same lock. A key that
/// differed between them (the raw path against the canonical one, say) would
/// order each route against itself and neither against the other.
///
/// Either order leaves both changes on disk. Unlocked, the anchored edit lands
/// and the whole write merges against the text it is about to replace, so the
/// anchored edit is gone with a 200 in hand.
#[test]
fn a_patch_and_a_put_to_one_note_take_one_lock() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(1)
        .build()
        .unwrap();

    runtime.block_on(async {
        let base = "A\nB\nC\n";
        let disk = "A\nB\nC2\n";
        let (kiln, note, _) = kiln_with_note(disk).await;
        let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
        let app = build_test_app(build_mock_state(client));

        let blocker = tokio::task::spawn_blocking(|| {
            std::thread::sleep(std::time::Duration::from_millis(50));
        });

        let (put, patch) = tokio::join!(
            put_file(
                app.clone(),
                json!({
                    "path": note,
                    "content": "A1\nB\nC\n",
                    "base_hash": crucible_core::note_edit::disk_hash(base),
                    "base_text": base,
                }),
            ),
            // No base: the outbox replay anchors on the note's current text,
            // whatever it is by the time the lock is free.
            patch_file(
                app.clone(),
                json!({
                    "path": note,
                    "edits": [{ "expect": "B", "replace": "B1" }],
                }),
            ),
        );
        blocker.await.unwrap();

        assert_eq!(put.0, StatusCode::OK, "{}", put.1);
        assert_eq!(patch.0, StatusCode::OK, "{}", patch.1);
        assert_eq!(
            tokio::fs::read_to_string(&note).await.unwrap(),
            "A1\nB1\nC2\n",
            "the whole write, the anchored edit and the disk all survive"
        );
    });
}

/// The ordinary write is unchanged, and says so: `merged` is false and the
/// answer carries no text, because the caller already holds what it sent.
#[tokio::test]
async fn a_put_whose_base_is_current_writes_its_own_text_and_merged_is_false() {
    let disk = "A\nB\nC\n";
    let (kiln, note, current_hash) = kiln_with_note(disk).await;
    let (_mock, client) = start_mock_daemon_with_kilns(vec![kiln.path().to_path_buf()]).await;
    let app = build_test_app(build_mock_state(client));

    let (status, body) = put_file(
        app,
        json!({
            "path": note,
            "content": "A\nB\nC\nD\n",
            "base_hash": current_hash,
            "base_text": disk,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["merged"], false);
    assert_eq!(body["content"], Value::Null, "nothing to send back");
    assert_eq!(
        body["content_hash"],
        crucible_core::note_edit::disk_hash("A\nB\nC\nD\n")
    );
    assert_eq!(
        tokio::fs::read_to_string(&note).await.unwrap(),
        "A\nB\nC\nD\n"
    );
}
