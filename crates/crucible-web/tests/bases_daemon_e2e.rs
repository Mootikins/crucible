//! The HTTP transport preserves daemon query values and stale-write refusals.
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use crucible_daemon::test_support::InProcessDaemonBuilder;
use crucible_web::test_support::{build_state, build_test_app};
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
    std::fs::create_dir(kiln.join("boards")).unwrap();
    std::fs::write(kiln.join("Host.md"), "![[Tasks.base#Tasks]]").unwrap();
    std::fs::write(kiln.join("boards/Tasks.base"), "filters: 'file.ext == \"md\"'\nformulas: {day: \"date('2026-09-27')\"}\nviews: [{type: table, name: Tasks, order: [file.name, note.status, formula.day]}]").unwrap();
    std::fs::write(kiln.join("Board.base"), "views:\n  - type: kanban\n    name: Board\n    groupBy: {property: note.status, direction: ASC}\n").unwrap();
    let server = InProcessDaemonBuilder::at_data_home(home.path().join("data"))
        .with_kiln_at("Work", &kiln)
        .start()
        .await
        .unwrap();
    let client = server.connect().await;
    let app = build_test_app(build_state(client));
    let (status, created) = request(&app, "POST", "/api/bases/entries", json!({"kiln":"Work", "source":{"path":"Tasks.base"}, "name":"First", "content":"# Body\n"})).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["status"], "applied");
    assert_eq!(created["path"], "First.md");
    let uri = "/api/bases/query?kiln=Work&path=Tasks.base&view=Tasks&this=Host.md";
    let (status, result) = request(&app, "GET", uri, Value::Null).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["rows"][0]["path"], "First.md");
    assert_eq!(
        result["rows"][0]["values"]["file.name"],
        json!({"type":"string","value":"First"})
    );
    assert_eq!(
        result["rows"][0]["values"]["formula.day"],
        json!({"type":"dateonly", "value":"2026-09-27"})
    );
    assert_eq!(result["source_path"], "boards/Tasks.base");
    assert_eq!(result["options"]["row_height"], "short");
    let edit = json!({"kiln":"Work", "path":"First.md", "key":"status", "value":"done", "ancestor_hash":result["rows"][0]["ancestor_hash"]});
    let (status, result) = request(&app, "PUT", "/api/bases/property", edit.clone()).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    let (status, result) = request(&app, "PUT", "/api/bases/property", edit).await;
    assert_eq!(status, StatusCode::CONFLICT, "{result}");
    assert!(std::fs::read_to_string(kiln.join("First.md"))
        .unwrap()
        .ends_with("# Body\n"));
    let (status, body) = request(
        &app,
        "PUT",
        "/api/bases/property",
        json!({"kiln":"Work", "path":"First.md", "key":"status", "ancestor_hash":"x"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "no value and no delete: {body}"
    );

    let (status, views) = request(
        &app,
        "GET",
        "/api/bases/views?kiln=Work&path=Board.base",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{views}");
    assert_eq!(views, json!([{"name":"Board", "type":"kanban"}]));

    let (status, board) = request(
        &app,
        "GET",
        "/api/bases/query?kiln=Work&path=Board.base",
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{board}");
    let order = json!({"kiln":"Work", "source":{"path":"Board.base"}, "view":"Board", "group_order":["done", "todo"], "ancestor_hash":board["source_hash"]});
    let (status, body) = request(&app, "PUT", "/api/bases/group-order", order.clone()).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "applied");
    let written = std::fs::read_to_string(kiln.join("Board.base")).unwrap();
    assert!(written.contains("groupOrder"), "{written}");
    let (status, body) = request(&app, "PUT", "/api/bases/group-order", order).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    let current_hash = body["current_hash"].clone();
    assert_eq!(current_hash.as_str().map(str::len), Some(64), "{body}");
    assert_eq!(
        std::fs::read_to_string(kiln.join("Board.base")).unwrap(),
        written
    );

    for (uri, expected) in [
        (
            "/api/bases/query?kiln=Work&path=../x.base",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "/api/bases/query?kiln=Work&path=Tasks.base&this=../x.md",
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "/api/bases/query?kiln=Work&path=Missing.base",
            StatusCode::NOT_FOUND,
        ),
        (
            "/api/bases/views?kiln=Work&path=Missing.base",
            StatusCode::NOT_FOUND,
        ),
        (
            "/api/bases/query?kiln=Absent&path=Tasks.base",
            StatusCode::NOT_FOUND,
        ),
    ] {
        let (status, body) = request(&app, "GET", uri, Value::Null).await;
        assert_eq!(status, expected, "{uri}: {body}");
    }
    let unknown_view = json!({"kiln":"Work", "source":{"path":"Board.base"}, "view":"Absent", "group_order":[], "ancestor_hash":current_hash});
    let (status, body) = request(&app, "PUT", "/api/bases/group-order", unknown_view).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    let (status, body) = request(&app, "POST", "/api/bases/entries", json!({"kiln":"Work", "source":{"path":"Board.base"}, "view":"Absent", "name":"Nope", "group":"todo"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(!kiln.join("Nope.md").exists());
    server.shutdown().await;
}
