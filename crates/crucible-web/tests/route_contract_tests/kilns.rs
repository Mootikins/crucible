//! Search/Kiln Route Contract Tests (with mock daemon)

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use super::shared::{build_mock_state, build_test_app, start_mock_daemon};

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
