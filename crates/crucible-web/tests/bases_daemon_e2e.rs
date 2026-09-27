//! The HTTP transport preserves daemon query values and stale-write refusals.
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use crucible_daemon::{DaemonClient, Server};
use crucible_web::test_support::{build_mock_state, build_test_app};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn request(app: &Router, method: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn bases_http_queries_and_refuses_stale_edits_through_real_daemon() {
    let home = tempfile::tempdir().unwrap();
    let kiln = home.path().join("kiln");
    std::fs::create_dir(&kiln).unwrap();
    std::fs::write(kiln.join("Tasks.base"), "filters: 'file.ext == \"md\"'\nviews: [{type: table, name: Tasks, order: [file.name, note.status]}]").unwrap();
    let socket = home.path().join("daemon.sock");
    let server = Server::bind_with_data_home_and_kilns(
        &socket,
        home.path().join("data"),
        &[("Work", &kiln)],
    )
    .await
    .unwrap();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(async move { server.run().await });
    let client = DaemonClient::connect_to(&socket).await.unwrap();
    let app = build_test_app(build_mock_state(client));
    let (status, created) = request(&app, "POST", "/api/bases/entries", json!({"kiln":"Work", "source":{"path":"Tasks.base"}, "name":"First", "content":"# Body\n"})).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["path"], "First.md");
    let uri = "/api/bases/query?kiln=Work&path=Tasks.base&view=Tasks";
    let (status, result) = request(&app, "GET", uri, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["rows"][0]["path"], "First.md");
    assert_eq!(
        result["rows"][0]["values"]["file.name"],
        json!({"type":"string","value":"First.md"})
    );
    let edit = json!({"kiln":"Work", "path":"First.md", "key":"status", "value":"done", "ancestor_hash":result["rows"][0]["ancestor_hash"]});
    let (status, result) = request(&app, "PUT", "/api/bases/property", edit.clone()).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let (status, result) = request(&app, "PUT", "/api/bases/property", edit).await;
    assert_eq!(status, StatusCode::CONFLICT, "{result}");
    assert!(std::fs::read_to_string(kiln.join("First.md"))
        .unwrap()
        .ends_with("# Body\n"));
    let _ = shutdown.send(());
    task.await.unwrap().unwrap();
}
